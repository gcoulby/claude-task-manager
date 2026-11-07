//! SQL for the archive tables (sources and entries).

use rusqlite::{Connection, OptionalExtension, params};

use crate::error::AppResult;
use crate::ingest::parser::ParsedEntry;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRow {
    pub id: i64,
    pub byte_offset: u64,
    pub size: u64,
    pub mtime_ms: i64,
}

pub struct NewSource<'a> {
    pub path: &'a str,
    pub session_id: &'a str,
    pub agent_id: Option<&'a str>,
    pub project_dir: &'a str,
}

#[derive(Debug, Default, Clone)]
pub struct AgentMeta {
    pub parent_tool_use_id: Option<String>,
    pub agent_type: Option<String>,
    pub description: Option<String>,
}

pub fn get_source(conn: &Connection, path: &str) -> AppResult<Option<SourceRow>> {
    Ok(conn
        .query_row(
            "SELECT id, byte_offset, size, mtime_ms FROM sources WHERE path = ?1",
            [path],
            |r| {
                Ok(SourceRow {
                    id: r.get(0)?,
                    byte_offset: r.get::<_, i64>(1)?.max(0) as u64,
                    size: r.get::<_, i64>(2)?.max(0) as u64,
                    mtime_ms: r.get(3)?,
                })
            },
        )
        .optional()?)
}

pub fn insert_source(conn: &Connection, s: &NewSource) -> AppResult<SourceRow> {
    conn.execute(
        "INSERT INTO sources (path, session_id, agent_id, project_dir) VALUES (?1, ?2, ?3, ?4)",
        params![s.path, s.session_id, s.agent_id, s.project_dir],
    )?;
    Ok(SourceRow {
        id: conn.last_insert_rowid(),
        byte_offset: 0,
        size: 0,
        mtime_ms: 0,
    })
}

pub fn set_agent_meta(conn: &Connection, source_id: i64, meta: &AgentMeta) -> AppResult<()> {
    conn.execute(
        "UPDATE sources SET parent_tool_use_id = ?2, agent_type = ?3, agent_description = ?4
         WHERE id = ?1",
        params![
            source_id,
            meta.parent_tool_use_id,
            meta.agent_type,
            meta.description
        ],
    )?;
    Ok(())
}

pub fn set_source_progress(
    conn: &Connection,
    source_id: i64,
    byte_offset: u64,
    size: u64,
    mtime_ms: i64,
) -> AppResult<()> {
    conn.execute(
        "UPDATE sources SET byte_offset = ?2, size = ?3, mtime_ms = ?4, source_missing = 0
         WHERE id = ?1",
        params![source_id, byte_offset as i64, size as i64, mtime_ms],
    )?;
    Ok(())
}

/// The file shrank, so it was replaced rather than appended to. Drop what we
/// read from it and start again. This is the one case where entries are deleted.
pub fn reset_source(conn: &Connection, source_id: i64) -> AppResult<()> {
    conn.execute("DELETE FROM entries WHERE source_id = ?1", [source_id])?;
    conn.execute(
        "UPDATE sources SET byte_offset = 0, size = 0, mtime_ms = 0 WHERE id = ?1",
        [source_id],
    )?;
    Ok(())
}

/// Returns false if the line was already stored.
pub fn insert_entry(
    conn: &Connection,
    source_id: i64,
    byte_offset: u64,
    session_id: &str,
    agent_id: Option<&str>,
    e: &ParsedEntry,
) -> AppResult<bool> {
    let mut stmt = conn.prepare_cached(
        "INSERT OR IGNORE INTO entries (source_id, byte_offset, session_id, agent_id, uuid,
             parent_uuid, kind, tool_name, tool_use_id, message_id, stop_reason, is_error, cwd,
             summary, raw_json, ts)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
    )?;
    let n = stmt.execute(params![
        source_id,
        byte_offset as i64,
        session_id,
        agent_id,
        e.uuid,
        e.parent_uuid,
        e.kind.as_str(),
        e.tool_name,
        e.tool_use_id,
        e.message_id,
        e.stop_reason,
        e.is_error,
        e.cwd,
        e.summary,
        e.raw_json,
        e.ts,
    ])?;
    Ok(n > 0)
}

/// Flags sources whose file has gone (or come back). Returns the sessions
/// whose flag changed.
pub fn refresh_missing<F>(conn: &Connection, exists: F) -> AppResult<Vec<String>>
where
    F: Fn(&str) -> bool,
{
    let rows: Vec<(i64, String, String, bool)> = conn
        .prepare("SELECT id, path, session_id, source_missing FROM sources")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<Result<_, _>>()?;
    let mut changed = Vec::new();
    for (id, path, session_id, was_missing) in rows {
        let missing = !exists(&path);
        if missing != was_missing {
            conn.execute(
                "UPDATE sources SET source_missing = ?2 WHERE id = ?1",
                params![id, missing],
            )?;
            if !changed.contains(&session_id) {
                changed.push(session_id);
            }
        }
    }
    Ok(changed)
}

pub fn all_session_ids(conn: &Connection) -> AppResult<Vec<String>> {
    Ok(conn
        .prepare("SELECT DISTINCT session_id FROM sources ORDER BY session_id")?
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?)
}

pub fn get_setting(conn: &Connection, key: &str) -> AppResult<Option<String>> {
    Ok(conn
        .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
            r.get(0)
        })
        .optional()?)
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> AppResult<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

pub fn delete_setting(conn: &Connection, key: &str) -> AppResult<()> {
    conn.execute("DELETE FROM settings WHERE key = ?1", [key])?;
    Ok(())
}

pub fn stale_minutes(conn: &Connection) -> AppResult<i64> {
    Ok(get_setting(conn, "stale_minutes")?
        .and_then(|v| v.parse::<i64>().ok())
        .filter(|m| *m > 0)
        .unwrap_or(crate::ingest::status::DEFAULT_STALE_MINUTES))
}

/// A task as stored, joined with its session. Status is the stored one,
/// before the read-time staleness check.
#[derive(Debug, Clone)]
pub struct StoredTask {
    pub id: String,
    pub session_id: String,
    pub parent_task_id: Option<String>,
    pub agent_id: Option<String>,
    pub title: String,
    pub trigger: String,
    pub status: String,
    pub status_reason: String,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub last_activity_at: Option<String>,
    pub entry_count: i64,
    pub seq: i64,
    pub activity_ms: i64,
    pub session_title: Option<String>,
    pub project_cwd: Option<String>,
    pub project_dir: String,
    pub source_missing: bool,
}

const STORED_TASK_SELECT: &str = "
    SELECT t.id, t.session_id, t.parent_task_id, t.agent_id, t.title, t.trigger, t.status,
           t.status_reason, t.started_at, t.ended_at, t.last_activity_at, t.entry_count, t.seq,
           t.activity_ms, s.title, s.project_cwd, s.project_dir, s.source_missing
    FROM tasks t JOIN sessions s ON s.id = t.session_id";

fn stored_task(r: &rusqlite::Row) -> rusqlite::Result<StoredTask> {
    Ok(StoredTask {
        id: r.get(0)?,
        session_id: r.get(1)?,
        parent_task_id: r.get(2)?,
        agent_id: r.get(3)?,
        title: r.get(4)?,
        trigger: r.get(5)?,
        status: r.get(6)?,
        status_reason: r.get(7)?,
        started_at: r.get(8)?,
        ended_at: r.get(9)?,
        last_activity_at: r.get(10)?,
        entry_count: r.get(11)?,
        seq: r.get(12)?,
        activity_ms: r.get(13)?,
        session_title: r.get(14)?,
        project_cwd: r.get(15)?,
        project_dir: r.get(16)?,
        source_missing: r.get(17)?,
    })
}

pub fn all_tasks(conn: &Connection) -> AppResult<Vec<StoredTask>> {
    Ok(conn
        .prepare(STORED_TASK_SELECT)?
        .query_map([], stored_task)?
        .collect::<Result<_, _>>()?)
}

pub fn session_tasks_for(conn: &Connection, task_id: &str) -> AppResult<Vec<StoredTask>> {
    let sql = format!(
        "{STORED_TASK_SELECT} WHERE t.session_id = (SELECT session_id FROM tasks WHERE id = ?1)"
    );
    Ok(conn
        .prepare(&sql)?
        .query_map([task_id], stored_task)?
        .collect::<Result<_, _>>()?)
}

#[derive(Debug, Clone)]
pub struct StoredEntry {
    pub id: i64,
    pub kind: String,
    pub tool_name: Option<String>,
    pub tool_use_id: Option<String>,
    pub preview: String,
    pub summary_chars: i64,
    pub is_error: bool,
    pub ts: Option<String>,
}

pub fn task_entries(
    conn: &Connection,
    task_id: &str,
    preview_chars: i64,
) -> AppResult<Vec<StoredEntry>> {
    Ok(conn
        .prepare(
            "SELECT id, kind, tool_name, tool_use_id, substr(summary, 1, ?2), length(summary),
                    is_error, ts
             FROM entries
             WHERE task_id = ?1 AND kind NOT IN ('ai_title', 'custom_title')
             ORDER BY source_id, byte_offset",
        )?
        .query_map(params![task_id, preview_chars], |r| {
            Ok(StoredEntry {
                id: r.get(0)?,
                kind: r.get(1)?,
                tool_name: r.get(2)?,
                tool_use_id: r.get(3)?,
                preview: r.get(4)?,
                summary_chars: r.get(5)?,
                is_error: r.get(6)?,
                ts: r.get(7)?,
            })
        })?
        .collect::<Result<_, _>>()?)
}

pub fn entry_raw_json(conn: &Connection, entry_id: i64) -> AppResult<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT raw_json FROM entries WHERE id = ?1",
            [entry_id],
            |r| r.get(0),
        )
        .optional()?)
}

pub fn main_source_path(conn: &Connection, session_id: &str) -> AppResult<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT path FROM sources WHERE session_id = ?1 AND agent_id IS NULL",
            [session_id],
            |r| r.get(0),
        )
        .optional()?)
}

/// Every stored line, for re-parsing after a parser change.
pub fn all_raw_entries(conn: &Connection) -> AppResult<Vec<(i64, String)>> {
    Ok(conn
        .prepare("SELECT id, raw_json FROM entries")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?)
}

pub fn update_parsed_entry(conn: &Connection, id: i64, e: &ParsedEntry) -> AppResult<()> {
    let mut stmt = conn.prepare_cached(
        "UPDATE entries SET kind = ?2, tool_name = ?3, tool_use_id = ?4, message_id = ?5,
             stop_reason = ?6, is_error = ?7, cwd = ?8, summary = ?9, raw_json = ?10
         WHERE id = ?1",
    )?;
    stmt.execute(params![
        id,
        e.kind.as_str(),
        e.tool_name,
        e.tool_use_id,
        e.message_id,
        e.stop_reason,
        e.is_error,
        e.cwd,
        e.summary,
        e.raw_json,
    ])?;
    Ok(())
}
