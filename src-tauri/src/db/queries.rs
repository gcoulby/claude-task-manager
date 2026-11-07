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
