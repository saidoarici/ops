-- Personal Ops — ilk şema. Konvansiyonlar için docs/data-model.md.
-- Bu şemada secret alanı yoktur ve eklenmez (secret'lar macOS Keychain'de tutulur).

CREATE TABLE projects (
    id                   TEXT PRIMARY KEY,
    name                 TEXT NOT NULL,
    description          TEXT NOT NULL DEFAULT '',
    state                TEXT NOT NULL DEFAULT 'ACTIVE',
    health               TEXT NOT NULL DEFAULT 'ACTIVE',
    priority             INTEGER NOT NULL DEFAULT 3,
    local_paths          TEXT NOT NULL DEFAULT '[]',
    git_repositories     TEXT NOT NULL DEFAULT '[]',
    keywords             TEXT NOT NULL DEFAULT '[]',
    related_contacts     TEXT NOT NULL DEFAULT '[]',
    last_activity_at     TEXT,
    stale_threshold_days INTEGER NOT NULL DEFAULT 4,
    created_at           TEXT NOT NULL,
    updated_at           TEXT NOT NULL
);

CREATE TABLE tasks (
    id                    TEXT PRIMARY KEY,
    title                 TEXT NOT NULL,
    description           TEXT NOT NULL DEFAULT '',
    project_id            TEXT REFERENCES projects(id),
    status                TEXT NOT NULL DEFAULT 'INBOX',
    priority              INTEGER NOT NULL DEFAULT 3,
    importance            INTEGER NOT NULL DEFAULT 3,
    urgency               INTEGER NOT NULL DEFAULT 3,
    due_at                TEXT,
    scheduled_at          TEXT,
    created_at            TEXT NOT NULL,
    updated_at            TEXT NOT NULL,
    completed_at          TEXT,
    parent_task_id        TEXT REFERENCES tasks(id),
    tags                  TEXT NOT NULL DEFAULT '[]',
    source                TEXT NOT NULL DEFAULT 'LOCAL_UI',
    waiting_for           TEXT,
    waiting_since         TEXT,
    followup_at           TEXT,
    blocked_by            TEXT,
    estimated_minutes     INTEGER,
    energy_level          TEXT,
    confidence            REAL,
    last_evidence_at      TEXT,
    inferred_status       TEXT,
    user_confirmed_status INTEGER NOT NULL DEFAULT 1,
    archived              INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_tasks_status     ON tasks(status);
CREATE INDEX idx_tasks_project    ON tasks(project_id);
CREATE INDEX idx_tasks_due        ON tasks(due_at);
CREATE INDEX idx_tasks_archived   ON tasks(archived);

CREATE TABLE reminders (
    id          TEXT PRIMARY KEY,
    task_id     TEXT REFERENCES tasks(id),
    title       TEXT NOT NULL,
    notes       TEXT NOT NULL DEFAULT '',
    remind_at   TEXT NOT NULL,
    repeat_rule TEXT NOT NULL DEFAULT 'NONE',
    channels    TEXT NOT NULL DEFAULT '["MACOS"]',
    status      TEXT NOT NULL DEFAULT 'SCHEDULED',
    fired_at    TEXT,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE INDEX idx_reminders_due ON reminders(status, remind_at);

CREATE TABLE routines (
    id                   TEXT PRIMARY KEY,
    name                 TEXT NOT NULL,
    enabled              INTEGER NOT NULL DEFAULT 1,
    schedule             TEXT NOT NULL,
    timezone             TEXT NOT NULL DEFAULT 'Europe/Istanbul',
    action_type          TEXT NOT NULL,
    parameters           TEXT NOT NULL DEFAULT '{}',
    allowed_capabilities TEXT NOT NULL DEFAULT '[]',
    approval_policy      TEXT NOT NULL DEFAULT 'ASK',
    last_run_at          TEXT,
    next_run_at          TEXT,
    last_result          TEXT,
    created_at           TEXT NOT NULL,
    updated_at           TEXT NOT NULL
);

CREATE TABLE evidence (
    id               TEXT PRIMARY KEY,
    task_id          TEXT REFERENCES tasks(id),
    project_id       TEXT REFERENCES projects(id),
    type             TEXT NOT NULL,
    source           TEXT NOT NULL,
    timestamp        TEXT NOT NULL,
    summary          TEXT NOT NULL,
    confidence       REAL,
    source_reference TEXT,
    content_hash     TEXT,
    created_at       TEXT NOT NULL
);

CREATE INDEX idx_evidence_project ON evidence(project_id, timestamp);

CREATE TABLE remote_messages (
    id                       TEXT PRIMARY KEY,
    channel                  TEXT NOT NULL,
    external_message_id      TEXT NOT NULL,
    sender_id                TEXT NOT NULL,
    received_at              TEXT NOT NULL,
    raw_text                 TEXT NOT NULL,
    attachment_meta          TEXT NOT NULL DEFAULT '[]',
    authentication_state     TEXT NOT NULL,
    replay_state             TEXT NOT NULL DEFAULT 'NEW',
    parsed_intent            TEXT,
    resulting_inbox_item_id  TEXT,
    processing_status        TEXT NOT NULL DEFAULT 'PENDING',
    created_at               TEXT NOT NULL
);

CREATE UNIQUE INDEX idx_remote_dedupe ON remote_messages(channel, external_message_id);

CREATE TABLE agent_sessions (
    id                TEXT PRIMARY KEY,
    provider          TEXT NOT NULL,
    project_id        TEXT REFERENCES projects(id),
    started_at        TEXT NOT NULL,
    ended_at          TEXT,
    mode              TEXT NOT NULL DEFAULT 'ASK',
    working_directory TEXT,
    status            TEXT NOT NULL DEFAULT 'RUNNING',
    summary           TEXT,
    evidence_ids      TEXT NOT NULL DEFAULT '[]',
    created_at        TEXT NOT NULL
);

CREATE TABLE audit_events (
    id            TEXT PRIMARY KEY,
    seq           INTEGER NOT NULL UNIQUE,
    timestamp     TEXT NOT NULL,
    actor         TEXT NOT NULL,
    origin        TEXT NOT NULL,
    action        TEXT NOT NULL,
    target        TEXT,
    risk_level    TEXT NOT NULL DEFAULT 'R0',
    capability    TEXT,
    result        TEXT NOT NULL,
    metadata      TEXT NOT NULL DEFAULT '{}',
    previous_hash TEXT NOT NULL,
    hash          TEXT NOT NULL
);

CREATE INDEX idx_audit_seq ON audit_events(seq);

CREATE TABLE settings (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
