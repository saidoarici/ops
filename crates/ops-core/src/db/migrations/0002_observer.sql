-- Observer: repo durumu takibi, tespit edilen işler, evidence dedupe.
-- Observer yalnızca metadata saklar; dosya içeriği hiçbir tabloya yazılmaz
-- (yalnızca ad/sayı/özet).

CREATE TABLE repo_states (
    project_id     TEXT NOT NULL REFERENCES projects(id),
    repo_path      TEXT NOT NULL,
    branch         TEXT,
    head_commit    TEXT,
    dirty_files    INTEGER NOT NULL DEFAULT 0,
    dirty_since    TEXT,
    ahead          INTEGER NOT NULL DEFAULT 0,
    last_commit_at TEXT,
    last_scan_at   TEXT NOT NULL,
    PRIMARY KEY (project_id, repo_path)
);

CREATE TABLE detected_work (
    id                   TEXT PRIMARY KEY,
    project_id           TEXT REFERENCES projects(id),
    task_id              TEXT REFERENCES tasks(id),
    kind                 TEXT NOT NULL,
    title                TEXT NOT NULL,
    detail               TEXT NOT NULL DEFAULT '',
    evidence_ids         TEXT NOT NULL DEFAULT '[]',
    confidence           REAL NOT NULL DEFAULT 0.5,
    status               TEXT NOT NULL DEFAULT 'OPEN',
    suggested_task_title TEXT,
    dedupe_key           TEXT NOT NULL UNIQUE,
    first_detected_at    TEXT NOT NULL,
    last_seen_at         TEXT NOT NULL,
    resolved_at          TEXT,
    created_at           TEXT NOT NULL
);

CREATE INDEX idx_detected_status ON detected_work(status);

-- Evidence tekilleştirme: aynı gözlem (ör. aynı commit) bir kez kaydedilir.
CREATE UNIQUE INDEX idx_evidence_hash ON evidence(content_hash)
    WHERE content_hash IS NOT NULL;
