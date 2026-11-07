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

/// The file shrank, so it was rewritten rather than appended to. The old
/// source row and its entries are retired under a new path, never deleted,
/// and a fresh source row takes over the real path.
pub fn retire_source(conn: &Connection, source_id: i64, now_ms: i64) -> AppResult<SourceRow> {
    conn.execute(
        "INSERT INTO sources (path, session_id, agent_id, project_dir, parent_tool_use_id,
                              agent_type, agent_description)
         SELECT path || '.pending', session_id, agent_id, project_dir, parent_tool_use_id,
                agent_type, agent_description
         FROM sources WHERE id = ?1",
        [source_id],
    )?;
    let new_id = conn.last_insert_rowid();
    conn.execute(
        "UPDATE sources SET path = path || '#replaced-' || ?2, replaced_at_ms = ?2,
                            source_missing = 1
         WHERE id = ?1",
        params![source_id, now_ms],
    )?;
    // Only now is the real path free for the new row.
    conn.execute(
        "UPDATE sources SET path = substr(path, 1, length(path) - length('.pending'))
         WHERE id = ?1",
        [new_id],
    )?;
    Ok(SourceRow {
        id: new_id,
        byte_offset: 0,
        size: 0,
        mtime_ms: 0,
    })
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
             summary, raw_json, ts, input_tokens, output_tokens, cache_read_tokens,
             cache_write_tokens)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18,
                 ?19, ?20)",
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
        e.tokens.map(|t| t.input),
        e.tokens.map(|t| t.output),
        e.tokens.map(|t| t.cache_read),
        e.tokens.map(|t| t.cache_write),
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
        .prepare(
            "SELECT id, path, session_id, source_missing FROM sources
             WHERE replaced_at_ms IS NULL",
        )?
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
            "SELECT path FROM sources
             WHERE session_id = ?1 AND agent_id IS NULL AND replaced_at_ms IS NULL",
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
             stop_reason = ?6, is_error = ?7, cwd = ?8, summary = ?9, raw_json = ?10,
             input_tokens = ?11, output_tokens = ?12, cache_read_tokens = ?13,
             cache_write_tokens = ?14
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
        e.tokens.map(|t| t.input),
        e.tokens.map(|t| t.output),
        e.tokens.map(|t| t.cache_read),
        e.tokens.map(|t| t.cache_write),
    ])?;
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveCounts {
    pub sessions: i64,
    /// Sessions whose transcript Claude Code has deleted.
    pub sessions_archived_only: i64,
    pub tasks: i64,
    pub entries: i64,
}

pub fn archive_counts(conn: &Connection) -> AppResult<ArchiveCounts> {
    Ok(conn.query_row(
        "SELECT (SELECT COUNT(*) FROM sessions),
                (SELECT COUNT(*) FROM sessions WHERE source_missing = 1),
                (SELECT COUNT(*) FROM tasks),
                (SELECT COUNT(*) FROM entries)",
        [],
        |r| {
            Ok(ArchiveCounts {
                sessions: r.get(0)?,
                sessions_archived_only: r.get(1)?,
                tasks: r.get(2)?,
                entries: r.get(3)?,
            })
        },
    )?)
}

/// Turns free text into a safe FTS5 query: each word quoted (so FTS syntax
/// characters are literal), all words required, the last one a prefix so
/// results appear while typing. None if there is nothing to search for.
pub fn fts_query(input: &str) -> Option<String> {
    let words: Vec<String> = input
        .split_whitespace()
        .map(|w| w.replace('"', ""))
        .filter(|w| !w.is_empty())
        .collect();
    let (last, rest) = words.split_last()?;
    let mut parts: Vec<String> = rest.iter().map(|w| format!("\"{w}\"")).collect();
    parts.push(format!("\"{last}\"*"));
    Some(parts.join(" "))
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub entry_id: i64,
    pub task_id: String,
    pub task_title: String,
    pub task_status: String,
    pub kind: String,
    pub tool_name: Option<String>,
    /// Matched words are wrapped in \u{1} and \u{2} for the UI to highlight.
    pub snippet: String,
    pub ts: Option<String>,
}

pub fn search_entries(conn: &Connection, input: &str, limit: i64) -> AppResult<Vec<SearchHit>> {
    let Some(query) = fts_query(input) else {
        return Ok(Vec::new());
    };
    Ok(conn
        .prepare(
            "SELECT e.id, e.task_id, t.title, t.status, e.kind, e.tool_name,
                    snippet(entries_fts, 0, char(1), char(2), '…', 16), e.ts
             FROM entries_fts
             JOIN entries e ON e.id = entries_fts.rowid
             JOIN tasks t ON t.id = e.task_id
             WHERE entries_fts MATCH ?1 AND e.kind NOT IN ('ai_title', 'custom_title')
             ORDER BY rank
             LIMIT ?2",
        )?
        .query_map(params![query, limit], |r| {
            Ok(SearchHit {
                entry_id: r.get(0)?,
                task_id: r.get(1)?,
                task_title: r.get(2)?,
                task_status: r.get(3)?,
                kind: r.get(4)?,
                tool_name: r.get(5)?,
                snippet: r.get(6)?,
                ts: r.get(7)?,
            })
        })?
        .collect::<Result<_, _>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_responses_count_once() {
        let mut conn = crate::db::open_in_memory();
        let root =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/projects");
        crate::ingest::scanner::scan(&mut conn, &root, |_| {}).unwrap();
        let distinct: i64 = conn
            .query_row(
                "SELECT COUNT(DISTINCT message_id) FROM entries WHERE output_tokens IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let entries: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM entries WHERE output_tokens IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            entries > distinct,
            "fixtures should include split responses"
        );

        let all = token_totals_since(&conn, None).unwrap();
        // Every fixture response reports 10 input and 20 output tokens.
        assert_eq!(all.messages, distinct);
        assert_eq!((all.input, all.output), (10 * distinct, 20 * distinct));

        let none = token_totals_since(&conn, Some("2099-01-01T00:00:00Z")).unwrap();
        assert_eq!(none.messages, 0);

        let first_task: String = conn
            .query_row(
                "SELECT id FROM tasks WHERE session_id LIKE '%001' AND seq = 0",
                [],
                |r| r.get(0),
            )
            .unwrap();
        // Three own responses plus the subagent's two.
        assert_eq!(
            token_totals_for_task(&conn, &first_task).unwrap().messages,
            5
        );
    }

    #[test]
    fn fts_query_quotes_words_and_prefixes_the_last() {
        assert_eq!(
            fts_query("dark mode").as_deref(),
            Some("\"dark\" \"mode\"*")
        );
        assert_eq!(fts_query("  ").as_deref(), None);
        // FTS syntax in user input is treated as plain text.
        assert_eq!(
            fts_query("NOT a-b \"c\" col:x").as_deref(),
            Some("\"NOT\" \"a-b\" \"c\" \"col:x\"*")
        );
    }

    #[test]
    fn search_finds_entries_with_snippets_and_tolerates_odd_input() {
        let mut conn = crate::db::open_in_memory();
        let root =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/projects");
        crate::ingest::scanner::scan(&mut conn, &root, |_| {}).unwrap();

        let hits = search_entries(&conn, "toggl", 10).unwrap();
        assert!(!hits.is_empty());
        assert!(hits.iter().any(|h| h.snippet.contains("\u{1}toggle\u{2}")));
        assert!(hits.iter().all(|h| !h.task_title.is_empty()));

        for odd in ["\"", "AND", "a OR", "(", "*", "col:", "NEAR(x y)"] {
            assert!(search_entries(&conn, odd, 10).is_ok(), "{odd}");
        }
        assert!(search_entries(&conn, "", 10).unwrap().is_empty());
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenTotals {
    pub input: i64,
    pub output: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    /// Model responses counted.
    pub messages: i64,
}

/// One row per model response: a response split across entries repeats its
/// counts (output grows as it streams), so take the maximum of each.
const MESSAGES_CTE: &str = "
    WITH messages AS (
        SELECT message_id, MAX(ts) AS ts, MAX(task_id) AS task_id,
               MAX(input_tokens) AS input, MAX(output_tokens) AS output,
               MAX(cache_read_tokens) AS cache_read, MAX(cache_write_tokens) AS cache_write
        FROM entries
        WHERE message_id IS NOT NULL AND output_tokens IS NOT NULL
        GROUP BY message_id
    )";

fn totals_row(r: &rusqlite::Row) -> rusqlite::Result<TokenTotals> {
    Ok(TokenTotals {
        input: r.get::<_, Option<i64>>(0)?.unwrap_or(0),
        output: r.get::<_, Option<i64>>(1)?.unwrap_or(0),
        cache_read: r.get::<_, Option<i64>>(2)?.unwrap_or(0),
        cache_write: r.get::<_, Option<i64>>(3)?.unwrap_or(0),
        messages: r.get(4)?,
    })
}

/// Totals for responses at or after `since` (an ISO timestamp), or all.
pub fn token_totals_since(conn: &Connection, since: Option<&str>) -> AppResult<TokenTotals> {
    let sql = format!(
        "{MESSAGES_CTE}
         SELECT SUM(input), SUM(output), SUM(cache_read), SUM(cache_write), COUNT(*)
         FROM messages WHERE ?1 IS NULL OR ts >= ?1"
    );
    Ok(conn.query_row(&sql, [since], totals_row)?)
}

/// Totals for one task, including any subagent runs it launched.
pub fn token_totals_for_task(conn: &Connection, task_id: &str) -> AppResult<TokenTotals> {
    let sql = format!(
        "{MESSAGES_CTE}
         SELECT SUM(input), SUM(output), SUM(cache_read), SUM(cache_write), COUNT(*)
         FROM messages
         WHERE task_id = ?1 OR task_id IN (SELECT id FROM tasks WHERE parent_task_id = ?1)"
    );
    Ok(conn.query_row(&sql, [task_id], totals_row)?)
}
