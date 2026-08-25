# Personal Ops

A local-first personal operations manager for macOS: tasks, projects, waiting
items, reminders and routines, with a read-only observer that turns git and
file activity into evidence, an assistant that runs your own Claude Code /
Codex CLI under strict capability modes, and a Telegram inbox that can never
execute anything.

I built it for my own daily use because the expensive part of running several
projects at once is not entering tasks — it is carrying their state in your
head. Personal Ops keeps that state on the machine, infers what it can from
real signals (commits, file changes, agent sessions), and asks before it
changes anything.

The product UI is in Turkish (it is a personal tool); code identifiers,
documentation and this README are in English, code comments are in Turkish.

## Screenshots

Screenshots are not included yet. To capture your own against a fictional
workspace without touching real data:

```bash
export PERSONAL_OPS_DATA_DIR=/tmp/po-demo          # short path (socket length limit)
cargo run -p ops-daemon -- seed-demo && cargo run -p ops-daemon &
cd apps/desktop && pnpm tauri dev                  # add #tasks, #assistant, … to the dev URL
```

Good candidates: Today, Tasks, Assistant, Routines, Security Center.

## Highlights

* **Local-first.** SQLite in `~/Library/Application Support/PersonalOps`, a
  Rust daemon under launchd, a Tauri desktop shell. No cloud backend, no
  account, no listening network port.
* **Tasks, projects, waiting, reminders, routines.** Nine task statuses with
  first-class "waiting on someone" tracking; one-off and repeating reminders
  that fire while the window is closed; morning / evening / weekly briefs.
* **Deterministic Today view.** Focus (at most three tasks, each with a
  "why now"), attention list (overdue, waiting too long, blocked, stale) and a
  timeline — all computed from data, no model in the loop.
* **Observer.** Watches only the folders you approve (FSEvents + periodic
  git2 scans), records commits and file activity as evidence, computes project
  health, and surfaces half-finished work (uncommitted changes, unpushed
  commits, stale in-progress tasks) as suggestions you can convert or dismiss.
* **Assistant with capability modes.** Runs the `claude` or `codex` CLI you
  already have, in ASK / READ / EDIT / ACT / FULL modes mapped to explicit tool
  allowlists and sandboxes. ACT needs a local confirmation, FULL a local
  password stored as an Argon2 digest in Keychain.
* **Constrained remote intake.** Telegram messages from one allowlisted sender
  can create inbox tasks, notes, reminder *proposals* and run task queries.
  Nothing else exists on that surface. WhatsApp is outbound-only.
* **Tamper-evident audit log.** Every mutation writes an audit row inside the
  same transaction; rows are SHA-256 hash-chained and verifiable from the CLI
  and the Security Center screen.
* **Desktop polish.** ⌘K command palette, ⌘N / ⌥Space quick capture, menu bar
  icon, light and dark themes, a Security Center that shows what is connected
  and what was denied.

## How it works

```text
┌──────────────────────────────┐
│  Desktop shell (Tauri 2)     │  React + TypeScript, thin IPC client
└──────────────┬───────────────┘
               │ Unix domain socket (0600), NDJSON
┌──────────────▼───────────────┐
│  personal-opsd (Rust daemon) │  launchd user agent, never root
│  store · today · scheduler   │
│  observer · agent · remote   │
└──┬──────────┬──────────┬─────┘
   │          │          │
 git repos  claude /   Telegram (outbound long-poll)
 (approved  codex CLI  WhatsApp bot (outbound only)
  folders)
```

The daemon owns all state and background work; the window is optional. The
Rust workspace is split into six crates plus the Tauri shell so that
responsibilities stay separate — the remote gateway crate cannot even link
against the agent runner. Details, the protocol method table and the
design decisions are in [docs/architecture.md](docs/architecture.md).

## Trust & permission model

A remote message cannot become an operating-system command. The path is:

```text
Telegram update → sender + chat allowlist → replay check → deterministic
intent parser → one of four typed intents → store write → audit row
```

* The intent type has exactly four variants (`CREATE_TASK`,
  `CREATE_REMINDER_PROPOSAL`, `QUERY_TASK`, `ADD_NOTE`); execution-like
  variants do not exist in the data model, and the `ops-remote` crate has no
  dependency on the agent runner or the daemon.
* Messages from other senders are not stored, parsed or answered.
* Mode changes and approvals only exist on the local socket, and the risky
  ones (ACT, FULL) additionally need a local confirmation or password.
* Agents run with a minimal environment, an approved working directory, a
  timeout and an output cap; `sudo` is never allowed.
* Secrets (bot token, WhatsApp key, Full Access digest) live only in the macOS
  Keychain; the settings table rejects secret-looking keys.

The full catalogue of threats, mitigations and the regression tests that pin
them is in [docs/threat-model.md](docs/threat-model.md).

## Tech stack

* **Rust** workspace: `tokio`, `rusqlite` (bundled SQLite),
  `git2`, `notify`, `reqwest` (rustls), `argon2`, `clap`, `serde`, `chrono`
* **Desktop:** Tauri 2, React 18, TypeScript (strict), TanStack Query, Vite
* **Platform:** macOS 12+ — launchd, Keychain (`/usr/bin/security`),
  FSEvents, `osascript` notifications
* **AI providers:** Claude Code CLI and Codex CLI, optional, used through
  their own login

## Installation

Prerequisites: macOS 12 or newer, Rust stable (developed with 1.97),
Node.js 20+ with pnpm 10, Xcode Command Line Tools. Optional: `claude` and/or
`codex` on your `PATH` for the assistant screen.

```bash
git clone https://github.com/saidoarici/ops.git
cd ops

# 1. Build and start the daemon (first terminal)
cargo run -p ops-daemon              # = personal-opsd run

# 2. Optional: load a fictional demo workspace into an empty database
cargo run -p ops-daemon -- seed-demo

# 3. Start the desktop app (second terminal)
cd apps/desktop
pnpm install
pnpm tauri dev
```

Production-style install:

```bash
cargo build --release -p ops-daemon
./target/release/personal-opsd install-launchd   # starts at login, gui/<uid> domain

cd apps/desktop && pnpm install && pnpm tauri build
# → target/release/bundle/macos/Personal Ops.app
```

Daemon commands:

```bash
personal-opsd run                 # foreground (default)
personal-opsd seed-demo [--force]
personal-opsd install-launchd | uninstall-launchd | launchd-status
personal-opsd verify-audit        # recompute the audit hash chain
personal-opsd backup              # online SQLite backup, keeps the last 10
personal-opsctl context           # JSON context for the assistant
personal-opsctl project add --name "Name" --path /local/folder
personal-opsctl task add --title "Title" [--project-id <id>]
personal-opsctl task list | task complete --id <id>
```

## Configuration

There is no `.env` file. Everything with a secret is entered once in
**Settings** inside the app and stored in the macOS Keychain; the rest lives
in the SQLite `settings` table.

| Setting | Where | Notes |
|---|---|---|
| Telegram bot token, allowed user ID, allowed chat ID | Settings → Telegram | Token → Keychain (`com.personalops.daemon` / `telegram_bot_token`), IDs → settings |
| WhatsApp bot URL, API key, phone number | Settings → WhatsApp | Key → Keychain; URL must be `https://` unless loopback |
| Full Access password | Settings → Full Access | Stored as an Argon2 digest in Keychain; 10–128 characters |
| Display name, theme | Settings → General | Theme is kept in the web view's local storage |

Environment variables (both optional):

| Variable | Effect |
|---|---|
| `PERSONAL_OPS_DATA_DIR` | Overrides the data directory (database, socket, backups). Keep the path short: macOS limits socket paths to ~100 characters. Used by tests and for demo profiles. |
| `RUST_LOG` | `tracing` filter for the daemon, default `info`. |

Data and logs:

```text
~/Library/Application Support/PersonalOps/{personalops.db,daemon.sock,Backups/}
~/Library/Logs/PersonalOps/daemon.log
~/Library/LaunchAgents/com.personalops.daemon.plist
```

## Development

```bash
cargo run -p ops-daemon                      # daemon with debug logging (RUST_LOG=debug for more)
cd apps/desktop && pnpm tauri dev            # UI with hot reload; can spawn the debug daemon itself

cargo fmt --all                              # rustfmt (config in rustfmt.toml)
cargo clippy --workspace --all-targets -- -D warnings
cd apps/desktop && pnpm lint && pnpm typecheck && pnpm format
```

The desktop app talks to the daemon only through `ops_call(method, params)`;
every method is listed in [docs/architecture.md](docs/architecture.md). Rust
enums are the source of truth for wire strings; `apps/desktop/src/lib/types.ts`
mirrors them by hand.

Icons are generated from `resources/icon-source.png`
(`python3 scripts/make_icon.py`, then `pnpm tauri icon`).

## Testing

```bash
cargo test --workspace                       # unit + integration, incl. security regressions
cd apps/desktop && pnpm lint && pnpm typecheck && pnpm format:check && pnpm build
```

The Rust suite covers the store and status transitions, the Today engine,
reminder scheduling, routine schedules, the observer against real git
repositories (created with `git2`, no shell), the intent parser, the remote
gateway (injection, replay, allowlist, rate limit), agent launch plans and
sandbox rules, Keychain input validation, the UDS server (permissions, request
cap) and the dispatch-level permission gates. Tests never touch the real
Keychain or the network. The same commands run in CI
(`.github/workflows/ci.yml`, macOS runner for Rust, Ubuntu for the UI).

## Project structure

```text
crates/
  ops-core/       models, SQLite store + audit chain, Today engine, IPC types, path guard
  ops-keychain/   macOS Keychain access
  ops-observer/   git2 + FSEvents observer → evidence, detections, project health
  ops-agent/      Claude Code / Codex runner with mode → allowlist mapping
  ops-remote/     Telegram gateway, intent parser, WhatsApp outbound adapter
  ops-daemon/     personal-opsd (server, dispatch, scheduler, routines) and personal-opsctl
apps/desktop/
  src/            React screens, components, typed query hooks
  src-tauri/      Tauri shell (UDS proxy, tray, global shortcut)
docs/             architecture.md · threat-model.md · data-model.md
resources/        launchd plist template, icon source
scripts/          icon generator
```

## Current status

Personal Ops is a working single-user application that I run every day. It is
not packaged for distribution: builds are unsigned and not notarized, there is
no auto-update, and the UI is Turkish-only. Things I would do next: an English
UI locale, signing/notarization, richer briefs, and linking evidence to tasks
by keyword.

## License

[MIT](LICENSE)
