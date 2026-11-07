-- Initial schema. Entries and sources are the archive; sessions and tasks are
-- derived from them and can be rebuilt at any time.

-- One row per transcript file we have read. The byte offset lets us resume
-- after the last complete line instead of re-reading the file.
CREATE TABLE sources (
    id                  INTEGER PRIMARY KEY,
    path                TEXT NOT NULL UNIQUE,
    session_id          TEXT NOT NULL,
    -- Set for subagent runs, NULL for the main session transcript.
    agent_id            TEXT,
    project_dir         TEXT NOT NULL,
    byte_offset         INTEGER NOT NULL DEFAULT 0,
    size                INTEGER NOT NULL DEFAULT 0,
    mtime_ms            INTEGER NOT NULL DEFAULT 0,
    -- From the subagent's meta.json sidecar.
    parent_tool_use_id  TEXT,
    agent_type          TEXT,
    agent_description   TEXT,
    source_missing      INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX sources_session ON sources (session_id);

-- Append-only. One row per kept transcript line.
CREATE TABLE entries (
    id            INTEGER PRIMARY KEY,
    source_id     INTEGER NOT NULL REFERENCES sources (id),
    byte_offset   INTEGER NOT NULL,
    session_id    TEXT NOT NULL,
    agent_id      TEXT,
    task_id       TEXT,
    uuid          TEXT,
    parent_uuid   TEXT,
    kind          TEXT NOT NULL,
    tool_name     TEXT,
    tool_use_id   TEXT,
    message_id    TEXT,
    stop_reason   TEXT,
    is_error      INTEGER NOT NULL DEFAULT 0,
    cwd           TEXT,
    -- Redacted and truncated text for lists and search.
    summary       TEXT NOT NULL,
    -- The original line, with embedded image and document data removed.
    raw_json      TEXT NOT NULL,
    ts            TEXT,
    UNIQUE (source_id, byte_offset)
);
CREATE INDEX entries_session ON entries (session_id, source_id, byte_offset);
CREATE INDEX entries_task ON entries (task_id);

CREATE TABLE sessions (
    id                TEXT PRIMARY KEY,
    project_cwd       TEXT,
    project_dir       TEXT NOT NULL,
    title             TEXT,
    started_at        TEXT,
    last_activity_at  TEXT,
    -- Last time the main transcript file changed, for staleness.
    source_mtime_ms   INTEGER NOT NULL DEFAULT 0,
    status            TEXT NOT NULL,
    status_reason     TEXT NOT NULL,
    task_count        INTEGER NOT NULL DEFAULT 0,
    source_missing    INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE tasks (
    -- Stable across rebuilds: the uuid of the entry that started the task,
    -- or "agent:<agent id>" for a subagent run.
    id               TEXT PRIMARY KEY,
    session_id       TEXT NOT NULL,
    agent_id         TEXT,
    parent_task_id   TEXT,
    title            TEXT NOT NULL,
    -- prompt, notification, command or subagent.
    trigger          TEXT NOT NULL,
    status           TEXT NOT NULL,
    status_reason    TEXT NOT NULL,
    started_at       TEXT,
    ended_at         TEXT,
    last_activity_at TEXT,
    entry_count      INTEGER NOT NULL DEFAULT 0,
    seq              INTEGER NOT NULL
);
CREATE INDEX tasks_session ON tasks (session_id, seq);
CREATE INDEX tasks_activity ON tasks (last_activity_at);

CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE VIRTUAL TABLE entries_fts USING fts5 (
    summary,
    content = 'entries',
    content_rowid = 'id'
);

CREATE TRIGGER entries_fts_insert AFTER INSERT ON entries BEGIN
    INSERT INTO entries_fts (rowid, summary) VALUES (new.id, new.summary);
END;

CREATE TRIGGER entries_fts_delete AFTER DELETE ON entries BEGIN
    INSERT INTO entries_fts (entries_fts, rowid, summary) VALUES ('delete', old.id, old.summary);
END;

CREATE TRIGGER entries_fts_update AFTER UPDATE OF summary ON entries BEGIN
    INSERT INTO entries_fts (entries_fts, rowid, summary) VALUES ('delete', old.id, old.summary);
    INSERT INTO entries_fts (rowid, summary) VALUES (new.id, new.summary);
END;
