-- Agent chat: oturum mesajları ve oturum meta genişletmeleri.
-- Provider credential'ları burada saklanmaz (kurulu resmi CLI kullanıcı
-- hesabıyla çalışır; OAuth secret'larına dokunulmaz).

CREATE TABLE agent_messages (
    id         TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES agent_sessions(id),
    seq        INTEGER NOT NULL,
    role       TEXT NOT NULL,
    content    TEXT NOT NULL DEFAULT '',
    payload    TEXT,
    created_at TEXT NOT NULL
);

CREATE UNIQUE INDEX idx_agent_messages_seq ON agent_messages(session_id, seq);

ALTER TABLE agent_sessions ADD COLUMN provider_session_id TEXT;
ALTER TABLE agent_sessions ADD COLUMN last_activity_at TEXT;
ALTER TABLE agent_sessions ADD COLUMN title TEXT;
