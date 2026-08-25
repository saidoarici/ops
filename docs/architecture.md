# Architecture

Personal Ops is a local-first macOS application split into a **background
daemon** (Rust) and a **thin desktop shell** (Tauri 2 + React). All state,
scheduling, observation and remote-channel handling live in the daemon so that
reminders, routines and Telegram intake keep working while the window is closed.

```text
┌──────────────────────────────────────────────┐
│  Desktop shell — Tauri 2 window              │  React 18 + TypeScript (strict)
│  (typed IPC client, polling, no business     │  TanStack Query
│   logic)                                     │
└───────────────────────┬──────────────────────┘
                        │ Unix domain socket, mode 0600
                        │ NDJSON request/response
┌───────────────────────▼──────────────────────┐
│  personal-opsd — Rust daemon (launchd agent) │
│                                              │
│  dispatch ── typed method table              │
│  store ───── SQLite + hash-chained audit     │
│  today ───── deterministic focus/attention   │
│  scheduler ─ reminders + routines (30 s)     │
│  observer ── git2 + FSEvents, read-only      │
│  agent ───── Claude Code / Codex CLI runner  │
│  remote ──── Telegram long-poll gateway      │
└───┬──────────────┬───────────────┬───────────┘
    │              │               │
 local git     claude / codex   Telegram API (outbound long-poll)
 repositories  CLIs (user's     WhatsApp bot API (outbound only)
 (approved     own login)
  folders)
```

The remote world reaches the daemon only through the gateway in `ops-remote`,
which can do nothing but write typed records into the store. There is no code
path from a remote message to a process, a file write or an agent session; see
[threat-model.md](threat-model.md).

## Repository layout

```text
crates/
  ops-core       Domain models, SQLite store (audit inside every mutation),
                 Today engine, IPC types, path security gate. No network,
                 no subprocesses.
  ops-keychain   macOS Keychain access through /usr/bin/security.
  ops-observer   Read-only observer: git2 snapshots + FSEvents (notify).
                 Produces evidence, repo state, detected work, project health.
  ops-agent      Claude Code / Codex orchestration: CLI detection, mode →
                 allowlist mapping, sandboxed process driver, stream parsing.
  ops-remote     Telegram long-poll gateway, deterministic intent parser,
                 WhatsApp outbound adapter. Depends on ops-core and
                 ops-keychain only — never on ops-agent or the daemon.
  ops-daemon     personal-opsd (UDS server, dispatch, scheduler, routines,
                 notifications, launchd install, Full Access password gate)
                 and personal-opsctl (typed CLI used by the assistant).
apps/desktop/
  src/           React UI (screens, components, typed query hooks)
  src-tauri/     Tauri shell: UDS proxy command, tray icon, ⌥Space shortcut
resources/       launchd plist template, icon source
docs/            this document, threat-model.md, data-model.md
```

Dependency direction is strictly downward: `ops-daemon → {ops-agent,
ops-observer, ops-remote} → ops-core`. The desktop shell depends on `ops-core`
only to compute the socket path.

## Design decisions

| Decision | Why |
|---|---|
| Separate daemon instead of logic in the app process | Reminders, routines, observation and Telegram intake must run with the window closed. The daemon is a launchd **user** agent (`gui/<uid>`), never a system daemon. |
| Unix domain socket, not localhost TCP | No listening port at all. The socket is `0600` inside a `0700` directory, so only the same macOS user can talk to the daemon. |
| NDJSON protocol | One JSON document per line; trivial to debug with `nc`, language-agnostic, no framing library. |
| SQLite via `rusqlite` (bundled), explicit SQL | Local-first, zero services, WAL for concurrent reads. Row mappers are hand-written; there is no ORM. |
| Deterministic core | Due dates, waiting time, stale detection, focus scoring, briefs and project health are plain code. No LLM is consulted for state. |
| AI through the user's installed CLIs | `claude` and `codex` run under the user's own login; the app never reads or copies their credentials and needs no API key. |
| Secrets only in Keychain | The database schema has no secret column and the settings table has a key allowlist that rejects secret-looking keys. |
| Hash-chained audit log | Every mutation appends an audit row inside the same transaction; `personal-opsd verify-audit` recomputes the chain. |

## Daemon protocol

Transport: Unix domain socket at `<data dir>/daemon.sock`. Framing: one UTF-8
JSON document per `\n`-terminated line, at most 1 MiB per line.

```jsonc
{ "id": 1, "method": "task.create", "params": { "title": "Follow up on the contract" } }
{ "id": 1, "result": { "id": "…", "title": "…", "status": "INBOX", "…": "…" } }
{ "id": 2, "error": { "code": "VALIDATION", "message": "başlık boş olamaz" } }
```

Error codes: `VALIDATION`, `NOT_FOUND`, `CONFLICT`, `SECURITY`, `DB`,
`BAD_REQUEST`, `IO`, `INTERNAL`, `UNKNOWN_METHOD`.

| Group | Methods |
|---|---|
| Health | `health.check` |
| Today | `today.view` |
| Tasks | `task.create` · `task.get` · `task.list` · `task.update` · `task.complete` · `task.archive` |
| Projects | `project.create` · `project.get` · `project.list` · `project.update` · `project.archive` · `project.overview` |
| Reminders | `reminder.create` · `reminder.list` · `reminder.update` · `reminder.dismiss` |
| Observer | `observer.status` · `observer.scan` · `evidence.list` · `detected.list` · `detected.dismiss` · `detected.convert` |
| Agent | `agent.detect` · `agent.chat` · `agent.sessions` · `agent.session` · `agent.messages` · `agent.cancel` · `agent.fullAccess.status` · `agent.fullAccess.configure` · `agent.fullAccess.lock` |
| Routines | `routine.list` · `routine.update` · `routine.run` |
| Remote | `remote.status` · `remote.telegram.configure` / `.test` / `.disable` · `remote.whatsapp.configure` / `.test` / `.disable` · `remote.messages` |
| Audit | `audit.list` · `audit.verify` |
| Settings | `settings.get` · `settings.set` |
| Data | `data.backup` · `data.backups` |

Every method deserialises its parameters into a typed struct
(`crates/ops-core/src/ipc.rs` and the model `*Create` / `*Patch` types);
mutation schemas reject unknown fields. Long-running work (`agent.chat`)
returns immediately and the UI polls `agent.messages`.

There is no server-push channel. The UI polls at 5 s (1.2 s while an agent
session is running); a single-user local socket makes this cheap enough that a
subscription mechanism was not worth its complexity.

## Daemon lifecycle

1. Create the data directory (`0700`), open the database, run embedded
   migrations (`crates/ops-core/src/db/migrations`), seed the three built-in
   routines if missing.
2. If a socket file already exists, try to connect: a live daemon means "exit"
   (single instance); a dead socket is removed.
3. Bind the socket, `chmod 0600`, start the scheduler, observer and remote
   gateway tasks. `SIGINT`/`SIGTERM` broadcast a shutdown signal; the socket is
   removed on exit.
4. **Scheduler** (every 30 s): fire due reminders (repeating ones are
   rescheduled), run due routines. On start, reminders more than 24 h overdue
   are marked `MISSED`; routines missed by more than 3 h are silently moved to
   their next slot.
5. **Observer**: FSEvents on every approved project folder plus a 5-minute full
   scan. Events are debounced for 3 s, mapped to their project, and turned into
   `FILE_CHANGE` evidence (rate-limited to one per 15 min per project) and a
   git rescan. Watchers are refreshed immediately when project paths change.
6. **Remote gateway**: Telegram `getUpdates` long-poll (50 s) when a token and
   allowlist are configured; otherwise it sleeps and re-checks every 30 s.

## Storage layout

```text
~/Library/Application Support/PersonalOps/   0700
├── personalops.db                           SQLite, WAL, 0600
├── daemon.sock                              UDS, 0600 while the daemon runs
└── Backups/                                 rolling online backups (last 10)
~/Library/Logs/PersonalOps/daemon.log        launchd stdout/stderr
~/Library/LaunchAgents/com.personalops.daemon.plist
```

`PERSONAL_OPS_DATA_DIR` overrides the data directory (used by tests and for
demo profiles). Keep it short: macOS limits socket paths to ~100 characters.

## Deterministic core

* **Today engine** (`ops-core::today`): scores open tasks from importance,
  urgency, priority, project priority, due-date proximity and scheduling, and
  picks at most three focus items, each with a one-line "why now". Overdue,
  long-waiting, blocked and stale tasks go to a separate attention list.
* **Reminders** (`ops-core::store::reminders`): one-off or repeating
  (daily / weekdays / weekly / monthly); the next occurrence is advanced past
  `now` so a daemon that was offline does not replay a backlog.
* **Routines** (`ops-daemon::routines`): Morning Brief, Evening Review and
  Weekly Review are built from the Today view, evidence counts and waiting
  tasks; they are delivered as macOS notifications and to every configured
  remote channel. Schedules are `HH:MM` or `MON HH:MM` in the machine's local
  time zone.
* **Project health** (`ops-observer::health`): `ACTIVE · QUIET · STALE ·
  BLOCKED · WAITING · AT_RISK · COMPLETED` from task counts and last activity.
* **Detected work** (`ops-observer::detect`): uncommitted changes older than
  24 h, unpushed commits older than 24 h, in-progress tasks idle past the
  project's stale threshold. Detections are suggestions; the user converts them
  into tasks or dismisses them, and a dismissed detection is never reopened by
  the system.

## Agent sessions

`agent.chat` starts or resumes a session with the selected provider and mode:

| Mode | Claude Code flags | Codex flags | Capability / risk |
|---|---|---|---|
| ASK | `--tools ""` (no tools) | `--sandbox read-only` | — / R0 |
| READ | `--tools Read,Glob,Grep` | `--sandbox read-only` | `READ_PROJECT_FILES` / R1 |
| EDIT | `--tools Read,Glob,Grep,Edit,Write --permission-mode acceptEdits` | `--sandbox workspace-write` | `WRITE_PROJECT_FILES` / R2 |
| ACT | `acceptEdits` + `--allowedTools Bash(git *),Bash(cargo *),…` | `--sandbox workspace-write` | `RUN_APPROVED_TEST` / R2 |
| FULL | `--dangerously-skip-permissions` | `--sandbox danger-full-access` | `FULL_LOCAL_ACCESS` / R4 |

`Bash(sudo *)`, `Bash(su *)` and `Bash(rm -rf /*)` are disallowed in every
mode. READ/EDIT/ACT require a project with an approved, existing folder and run
with that folder as the working directory. ACT needs an explicit confirmation
flag from the local UI; FULL needs the local Full Access password (Argon2
digest in Keychain, 5 failures → 60 s lockout, 30 min idle lock, cleared on
daemon restart). The CLI runs with a minimal environment (`HOME`, `USER`,
`LOGNAME`, `TMPDIR`, a fixed `PATH`), a 15-minute timeout and an 8 MiB output
cap; the prompt is passed on stdin, never through a shell.

The assistant is told to use `personal-opsctl` for application data
(`context`, `project add`, `task add`, `task complete`), which goes through
the same store validation and audit as the UI. `/görev <title>` in the chat is
handled locally and creates a task without calling the provider.

## Desktop shell

The Tauri process is a proxy: one `ops_call(method, params)` command writes the
request to the socket and returns the response. It also owns the tray icon and
the global ⌥Space shortcut. If the daemon is not reachable the UI shows a
connection screen; in debug builds it can launch the `personal-opsd` binary
sitting next to it. Production installs rely on launchd.
