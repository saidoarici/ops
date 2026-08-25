# Data model

Storage: SQLite (`~/Library/Application Support/PersonalOps/personalops.db`,
WAL, file mode `0600`). Schema migrations are embedded SQL files in
`crates/ops-core/src/db/migrations/` and applied forward-only, tracked in
`schema_migrations`.

## Conventions

* **IDs** are UUID v4 strings.
* **Timestamps** are UTC RFC 3339 with second precision (`2026-08-23T20:15:00Z`).
  The UI renders them in local time; `today.view` takes the UI's UTC offset so
  day boundaries are drawn where the user is.
* **Lists** (`tags`, `local_paths`, `channels`, …) are JSON arrays in `TEXT`.
* **Enums** are `SCREAMING_SNAKE_CASE` strings, identical on the wire and in
  the database. The single source of truth is
  `crates/ops-core/src/models/enums.rs`; `apps/desktop/src/lib/types.ts` mirrors
  it by hand and is updated together with it.
* **Deletion**: tasks are archived (soft delete), projects are set to
  `ARCHIVED`; audit rows are append-only.
* **Secrets**: there is no secret column anywhere in this schema.

## Tables

### `projects`

| Column | Type | Notes |
|---|---|---|
| id | TEXT PK | |
| name | TEXT | required; unique among non-archived projects (case-insensitive) |
| description | TEXT | default `''` |
| state | TEXT | `ACTIVE · PAUSED · ARCHIVED · COMPLETED` |
| health | TEXT | `ACTIVE · QUIET · STALE · BLOCKED · WAITING · AT_RISK · COMPLETED`, computed by the observer |
| priority | INTEGER | 1–5 |
| local_paths | JSON | approved folders; the observer looks nowhere else |
| git_repositories | JSON | optional repo roots; must resolve inside `local_paths` |
| keywords, related_contacts | JSON | free metadata |
| last_activity_at | TEXT? | advanced by task edits, commits and file activity |
| stale_threshold_days | INTEGER | default 4, clamped to 1–90 |
| created_at, updated_at | TEXT | |

### `tasks`

| Column | Type | Notes |
|---|---|---|
| id | TEXT PK | |
| title | TEXT | required, ≤ 500 chars; always data, never a command |
| description | TEXT | ≤ 10 000 chars |
| project_id | TEXT? → projects | |
| status | TEXT | `INBOX · PLANNED · NEXT · IN_PROGRESS · WAITING · BLOCKED · SOMEDAY · DONE · CANCELLED` |
| priority, importance, urgency | INTEGER | 1–5 each; the Today engine combines them |
| due_at, scheduled_at | TEXT? | deadline vs. planned day |
| created_at, updated_at, completed_at | TEXT | |
| parent_task_id | TEXT? → tasks | |
| tags | JSON | |
| source | TEXT | `LOCAL_UI · QUICK_CAPTURE · TELEGRAM · WHATSAPP · AGENT_CHAT · AI_DETECTED` |
| waiting_for, waiting_since, followup_at | TEXT? | first-class "waiting on someone" tracking |
| blocked_by | TEXT? | |
| estimated_minutes | INTEGER? | |
| energy_level | TEXT? | `LOW · MEDIUM · HIGH` |
| archived | INTEGER | soft delete |

Status transitions applied in the store: `→ DONE` sets `completed_at`;
`DONE →` clears it; `→ WAITING` stamps `waiting_since` if empty; `WAITING →`
clears it. Any task edit advances the project's `last_activity_at`.

### `reminders`

| Column | Notes |
|---|---|
| id, task_id?, title, notes | |
| remind_at | UTC |
| repeat_rule | `NONE · DAILY · WEEKDAYS · WEEKLY · MONTHLY` |
| channels | JSON list of `MACOS · TELEGRAM · WHATSAPP`, default `["MACOS"]` |
| status | `SCHEDULED · FIRED · DISMISSED · MISSED` |
| fired_at, created_at, updated_at | |

The scheduler fires `SCHEDULED` reminders whose `remind_at` has passed; a
repeating reminder is moved to its next occurrence after `now`, a one-off
becomes `FIRED`. Reminders more than 24 hours overdue at daemon start are
marked `MISSED`.

### `routines`

`id, name, enabled, schedule, action_type, last_run_at, next_run_at,
last_result (JSON), created_at, updated_at`

`schedule` is `HH:MM` (daily) or `MON HH:MM` (weekly) in the machine's local
time zone. `action_type` is `MORNING_BRIEF · EVENING_REVIEW · WEEKLY_REVIEW`;
the three built-ins are created on daemon start.

### `evidence`

`id, task_id?, project_id?, type, source, timestamp, summary, confidence?,
source_reference?, content_hash?, created_at`

`type` is `GIT_COMMIT · FILE_CHANGE · AI_SESSION · ROUTINE_RESULT`. A partial
unique index on `content_hash` makes observations idempotent (the same commit
is recorded once). The observer stores summaries, names and counts — never file
contents.

### `repo_states`

`(project_id, repo_path)` primary key; `branch, head_commit, dirty_files,
dirty_since, ahead, last_commit_at, last_scan_at`. Differences between scans
produce evidence and detections.

### `detected_work`

`id, project_id?, task_id?, kind, title, detail, evidence_ids (JSON),
confidence, status, suggested_task_title?, dedupe_key (UNIQUE),
first_detected_at, last_seen_at, resolved_at?, created_at`

`kind`: `UNCOMMITTED_CHANGES · UNPUSHED_COMMITS · STALE_TASK`. `status`:
`OPEN · DISMISSED · CONVERTED · RESOLVED`. State machine: a signal that
disappears resolves an open detection; a signal that returns reopens a
resolved one; `DISMISSED` and `CONVERTED` are user decisions and are never
overridden by the system.

### `remote_messages`

`id, channel, external_message_id, sender_id, received_at, raw_text,
authentication_state, replay_state, parsed_intent (JSON),
resulting_inbox_item_id?, processing_status, created_at`

* `channel`: `TELEGRAM · WHATSAPP` — `(channel, external_message_id)` is unique.
* `authentication_state`: `AUTHENTICATED · REJECTED_SENDER`; rejected senders
  are stored with an empty `raw_text`.
* `replay_state`: `NEW · REPLAYED`.
* `processing_status`: `PENDING · PROCESSED · REJECTED`.
* `parsed_intent` is one of `CREATE_TASK`, `CREATE_REMINDER_PROPOSAL`,
  `QUERY_TASK`, `ADD_NOTE`; nothing else can be represented.

### `agent_sessions` and `agent_messages`

Sessions: `id, provider (CLAUDE · CODEX), project_id?, started_at, ended_at?,
mode (ASK · READ · EDIT · ACT · FULL), working_directory?, status (RUNNING ·
COMPLETED · FAILED · CANCELLED), summary?, evidence_ids (JSON), created_at,
provider_session_id?, last_activity_at?, title?`.

Messages: `id, session_id, seq (unique per session), role (USER · ASSISTANT ·
TOOL · SYSTEM · ERROR), content (≤ 60 000 chars), payload?, created_at`.

A completed session with a project produces an `AI_SESSION` evidence row.

### `audit_events`

| Column | Notes |
|---|---|
| id, seq (UNIQUE) | `seq` is `max + 1` inside the writing transaction |
| timestamp | |
| actor | `USER · DAEMON · SCHEDULER · REMOTE` |
| origin | `LOCAL_UI · DAEMON · CLI · TELEGRAM · WHATSAPP` |
| action | e.g. `TASK_CREATE`, `REMINDER_FIRE`, `SEND_NOTIFICATION`, `REMOTE_MESSAGE_REJECTED` |
| target | e.g. `task:<id>` |
| risk_level | `R0 … R4` |
| capability | e.g. `CREATE_TASK`, `FULL_LOCAL_ACCESS` |
| result | `OK · DENIED · ERROR` |
| metadata | JSON; field names and counts, never content dumps or secrets |
| previous_hash, hash | see below |

### `settings`

`key TEXT PK, value TEXT (JSON), updated_at`. Keys are allowlisted in
`crates/ops-core/src/store/settings.rs` (`display_name`, `telegram_enabled`,
`telegram_allowed_user_id`, `telegram_allowed_chat_id`,
`telegram_last_update_id`, `whatsapp_config`). Secret-looking keys are refused.

## Audit hash chain

```text
canonical = JSON of (id, seq, timestamp, actor, origin, action, target,
                     risk_level, capability, result, metadata-as-stored-text,
                     previous_hash) in fixed field order
hash      = hex(SHA-256(previous_hash + "\n" + canonical))
```

The first row uses `previous_hash = "GENESIS"`. `metadata` is hashed as the
exact text stored in the row so verification is byte-stable across serde
versions. `personal-opsd verify-audit` (and `audit.verify` from the UI) walks
the chain and reports the first `seq` that fails.

## Backups

`data.backup` uses SQLite's online backup API to write
`Backups/personalops-YYYYMMDD-HHMMSS.db` (mode `0600`); the ten most recent
files are kept. Because the schema holds no secrets, backups hold none either.
