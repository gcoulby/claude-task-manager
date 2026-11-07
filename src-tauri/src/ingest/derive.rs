//! Rebuilds a session's tasks and summary row from its stored entries.
//!
//! Everything here is derived, so it can be thrown away and recomputed at any
//! time. Task ids are stable across rebuilds so the UI can keep a selection.

use std::collections::HashMap;

use rusqlite::{Connection, params};

use super::parser::EntryKind;
use super::status::{self, TaskFacts};
use crate::error::AppResult;

const TITLE_MAX_CHARS: usize = 120;

struct Source {
    id: i64,
    agent_id: Option<String>,
    project_dir: String,
    mtime_ms: i64,
    missing: bool,
    parent_tool_use_id: Option<String>,
    agent_description: Option<String>,
}

struct Row {
    source_id: i64,
    byte_offset: i64,
    uuid: Option<String>,
    kind: Option<EntryKind>,
    tool_name: Option<String>,
    tool_use_id: Option<String>,
    stop_reason: Option<String>,
    cwd: Option<String>,
    /// Only loaded for kinds that can name a task or session.
    title_text: String,
    ts: Option<String>,
}

struct TaskBuild {
    id: String,
    agent_id: Option<String>,
    parent_task_id: Option<String>,
    title: String,
    trigger: &'static str,
    source_id: i64,
    first_offset: i64,
    last_offset: i64,
    started_at: Option<String>,
    last_activity_at: Option<String>,
    entry_count: i64,
    facts: TaskFacts,
}

impl TaskBuild {
    fn start(id: String, trigger: &'static str, title: &str, row: &Row) -> Self {
        TaskBuild {
            id,
            agent_id: None,
            parent_task_id: None,
            title: title_from(title),
            trigger,
            source_id: row.source_id,
            first_offset: row.byte_offset,
            last_offset: row.byte_offset,
            started_at: row.ts.clone(),
            last_activity_at: row.ts.clone(),
            entry_count: 0,
            facts: TaskFacts::default(),
        }
    }

    fn add(&mut self, row: &Row) {
        self.last_offset = row.byte_offset;
        let Some(kind) = row.kind else { return };
        if matches!(kind, EntryKind::AiTitle | EntryKind::CustomTitle) {
            return;
        }
        self.entry_count += 1;
        // File order is authoritative but timestamps are not monotonic, so
        // take the latest seen rather than the last.
        if row.ts > self.last_activity_at {
            self.last_activity_at = row.ts.clone();
        }
        self.facts.observe(
            kind,
            row.tool_use_id.as_deref(),
            row.tool_name.as_deref(),
            row.stop_reason.as_deref(),
        );
    }
}

/// Recomputes tasks and the session row for one session.
pub fn rebuild_session(conn: &mut Connection, session_id: &str) -> AppResult<()> {
    let tx = conn.transaction()?;
    let sources = load_sources(&tx, session_id)?;
    let rows = load_rows(&tx, session_id)?;

    let main = sources.iter().find(|s| s.agent_id.is_none());
    let mut tasks: Vec<TaskBuild> = Vec::new();
    let mut task_by_tool_use: HashMap<String, usize> = HashMap::new();
    let mut ai_title = None;
    let mut custom_title = None;
    let mut project_cwd = None;
    let mut started_at: Option<String> = None;
    let mut last_activity_at: Option<String> = None;

    for row in &rows {
        if row.ts.is_some() {
            if started_at.is_none() || row.ts < started_at {
                started_at = row.ts.clone();
            }
            if row.ts > last_activity_at {
                last_activity_at = row.ts.clone();
            }
        }
    }

    if let Some(main) = main {
        for row in rows.iter().filter(|r| r.source_id == main.id) {
            if project_cwd.is_none() {
                project_cwd = row.cwd.clone();
            }
            match row.kind {
                Some(EntryKind::AiTitle) => ai_title = Some(row.title_text.clone()),
                Some(EntryKind::CustomTitle) => custom_title = Some(row.title_text.clone()),
                Some(kind) if kind.starts_task() => {
                    let id = row
                        .uuid
                        .clone()
                        .unwrap_or_else(|| format!("{}:{}", row.source_id, row.byte_offset));
                    let trigger = match kind {
                        EntryKind::Notification => "notification",
                        EntryKind::Command => "command",
                        _ => "prompt",
                    };
                    let title = if kind == EntryKind::Notification {
                        "Background task finished"
                    } else {
                        &row.title_text
                    };
                    tasks.push(TaskBuild::start(id, trigger, title, row));
                }
                _ => {}
            }
            if let Some(task) = tasks.last_mut() {
                task.add(row);
                if row.kind == Some(EntryKind::ToolUse)
                    && let Some(id) = &row.tool_use_id
                {
                    task_by_tool_use.insert(id.clone(), tasks.len() - 1);
                }
            }
        }
    }

    // Each subagent run is one child task of whichever task launched it.
    for src in sources.iter().filter(|s| s.agent_id.is_some()) {
        let mut src_rows = rows.iter().filter(|r| r.source_id == src.id);
        let Some(first) = src_rows.next() else {
            continue;
        };
        if project_cwd.is_none() {
            project_cwd = first.cwd.clone();
        }
        let agent_id = src.agent_id.clone().unwrap_or_default();
        let title = src
            .agent_description
            .clone()
            .unwrap_or_else(|| first.title_text.clone());
        let mut task = TaskBuild::start(format!("agent:{agent_id}"), "subagent", &title, first);
        task.agent_id = Some(agent_id);
        task.parent_task_id = src
            .parent_tool_use_id
            .as_ref()
            .and_then(|id| task_by_tool_use.get(id))
            .and_then(|&i| tasks.get(i))
            .map(|t| t.id.clone());
        task.add(first);
        for row in src_rows {
            task.add(row);
        }
        tasks.push(task);
    }

    // Only the newest top-level task can still be in progress; earlier ones
    // were ended by the prompt that followed them.
    let last_top_level = tasks.iter().rposition(|t| t.agent_id.is_none());
    let source_mtime_ms = main.map(|m| m.mtime_ms).unwrap_or(0);
    for (i, task) in tasks.iter_mut().enumerate() {
        let superseded = task.agent_id.is_none() && Some(i) != last_top_level;
        task.facts.superseded = superseded;
    }

    tx.execute("DELETE FROM tasks WHERE session_id = ?1", [session_id])?;
    {
        let mut insert = tx.prepare(
            "INSERT INTO tasks (id, session_id, agent_id, parent_task_id, title, trigger, status,
                 status_reason, started_at, ended_at, last_activity_at, entry_count, seq)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        )?;
        for (seq, t) in tasks.iter().enumerate() {
            let s = status::infer_task(&t.facts);
            let ended_at = if s.is_finished() {
                t.last_activity_at.clone()
            } else {
                None
            };
            insert.execute(params![
                t.id,
                session_id,
                t.agent_id,
                t.parent_task_id,
                t.title,
                t.trigger,
                s.status.as_str(),
                s.reason,
                t.started_at,
                ended_at,
                t.last_activity_at,
                t.entry_count,
                seq as i64,
            ])?;
        }
    }

    assign_entries(&tx, &sources, &tasks)?;

    let title = custom_title.or(ai_title).or_else(|| {
        tasks
            .iter()
            .find(|t| t.agent_id.is_none())
            .map(|t| t.title.clone())
    });
    let project_dir = main
        .or(sources.first())
        .map(|s| s.project_dir.clone())
        .unwrap_or_default();
    let session_status =
        status::infer_session(last_top_level.and_then(|i| tasks.get(i)).map(|t| &t.facts));
    let top_level_tasks = tasks.iter().filter(|t| t.agent_id.is_none()).count() as i64;
    tx.execute(
        "INSERT INTO sessions (id, project_cwd, project_dir, title, started_at, last_activity_at,
             source_mtime_ms, status, status_reason, task_count, source_missing)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
         ON CONFLICT (id) DO UPDATE SET
             project_cwd = excluded.project_cwd, project_dir = excluded.project_dir,
             title = excluded.title, started_at = excluded.started_at,
             last_activity_at = excluded.last_activity_at,
             source_mtime_ms = excluded.source_mtime_ms, status = excluded.status,
             status_reason = excluded.status_reason, task_count = excluded.task_count,
             source_missing = excluded.source_missing",
        params![
            session_id,
            project_cwd,
            project_dir,
            title,
            started_at,
            last_activity_at,
            source_mtime_ms,
            session_status.status.as_str(),
            session_status.reason,
            top_level_tasks,
            main.is_some_and(|m| m.missing),
        ],
    )?;
    tx.commit()?;
    Ok(())
}

/// Rebuilds every session, for recovery or after a derivation change.
pub fn rebuild_all(conn: &mut Connection) -> AppResult<usize> {
    let ids = crate::db::queries::all_session_ids(conn)?;
    for id in &ids {
        rebuild_session(conn, id)?;
    }
    Ok(ids.len())
}

/// Points each entry at its task. Only rows whose task changed are written,
/// which keeps live rebuilds of long sessions cheap.
fn assign_entries(conn: &Connection, sources: &[Source], tasks: &[TaskBuild]) -> AppResult<()> {
    let mut stmt = conn.prepare_cached(
        "UPDATE entries SET task_id = ?1
         WHERE source_id = ?2 AND byte_offset BETWEEN ?3 AND ?4 AND task_id IS NOT ?1",
    )?;
    for src in sources {
        let mut ranges: Vec<(Option<&str>, i64, i64)> = Vec::new();
        let mine: Vec<&TaskBuild> = tasks.iter().filter(|t| t.source_id == src.id).collect();
        // Entries before the first prompt belong to no task.
        let first = mine.first().map(|t| t.first_offset).unwrap_or(i64::MAX);
        ranges.push((None, 0, first.saturating_sub(1)));
        for (i, t) in mine.iter().enumerate() {
            let end = mine
                .get(i + 1)
                .map(|n| n.first_offset - 1)
                .unwrap_or(i64::MAX);
            ranges.push((Some(&t.id), t.first_offset, end));
        }
        for (task_id, start, end) in ranges {
            stmt.execute(params![task_id, src.id, start, end])?;
        }
    }
    Ok(())
}

fn load_sources(conn: &Connection, session_id: &str) -> AppResult<Vec<Source>> {
    Ok(conn
        .prepare(
            "SELECT id, agent_id, project_dir, mtime_ms, source_missing, parent_tool_use_id,
                    agent_description
             FROM sources WHERE session_id = ?1 ORDER BY agent_id IS NOT NULL, id",
        )?
        .query_map([session_id], |r| {
            Ok(Source {
                id: r.get(0)?,
                agent_id: r.get(1)?,
                project_dir: r.get(2)?,
                mtime_ms: r.get(3)?,
                missing: r.get(4)?,
                parent_tool_use_id: r.get(5)?,
                agent_description: r.get(6)?,
            })
        })?
        .collect::<Result<_, _>>()?)
}

fn load_rows(conn: &Connection, session_id: &str) -> AppResult<Vec<Row>> {
    Ok(conn
        .prepare(
            "SELECT source_id, byte_offset, uuid, kind, tool_name, tool_use_id, stop_reason, cwd,
                    CASE WHEN kind IN ('prompt', 'command', 'ai_title', 'custom_title')
                         THEN substr(summary, 1, 400) ELSE '' END,
                    ts
             FROM entries WHERE session_id = ?1 ORDER BY source_id, byte_offset",
        )?
        .query_map([session_id], |r| {
            let kind: String = r.get(3)?;
            Ok(Row {
                source_id: r.get(0)?,
                byte_offset: r.get(1)?,
                uuid: r.get(2)?,
                kind: EntryKind::parse(&kind),
                tool_name: r.get(4)?,
                tool_use_id: r.get(5)?,
                stop_reason: r.get(6)?,
                cwd: r.get(7)?,
                title_text: r.get(8)?,
                ts: r.get(9)?,
            })
        })?
        .collect::<Result<_, _>>()?)
}

/// First non-empty line, shortened.
fn title_from(text: &str) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    if line.is_empty() {
        return "(untitled)".into();
    }
    match line.char_indices().nth(TITLE_MAX_CHARS) {
        Some((i, _)) => format!("{}…", &line[..i]),
        None => line.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::ingest::scanner::scan;
    use std::path::PathBuf;

    fn ingested() -> Connection {
        let mut conn = db::open_in_memory();
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/projects");
        scan(&mut conn, &root, |_| {}).unwrap();
        conn
    }

    fn tasks(conn: &Connection, session: &str) -> Vec<(String, String, Option<String>, i64)> {
        conn.prepare(
            "SELECT title, trigger, parent_task_id, entry_count FROM tasks
             WHERE session_id LIKE ?1 ORDER BY seq",
        )
        .unwrap()
        .query_map([format!("%{session}")], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect()
    }

    #[test]
    fn splits_session_into_tasks_with_subagent_child() {
        let conn = ingested();
        let t = tasks(&conn, "001");
        let titles: Vec<(&str, &str)> = t.iter().map(|x| (x.0.as_str(), x.1.as_str())).collect();
        assert_eq!(
            titles,
            vec![
                ("Add a dark mode toggle to the settings page.", "prompt"),
                ("Now run the tests.", "prompt"),
                ("/clear", "command"),
                ("Fix the failing test.", "prompt"),
                ("Background task finished", "notification"),
                ("Review theme code", "subagent"),
            ]
        );
        let first_id: String = conn
            .query_row(
                "SELECT id FROM tasks WHERE session_id LIKE '%001' AND seq = 0",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(t[5].2.as_deref(), Some(first_id.as_str()));
        assert_eq!(t[5].3, 4);
    }

    #[test]
    fn session_row_uses_custom_title_and_real_cwd() {
        let conn = ingested();
        let (title, cwd, dir, count): (String, String, String, i64) = conn
            .query_row(
                "SELECT title, project_cwd, project_dir, task_count FROM sessions WHERE id LIKE '%001'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(title, "Settings work");
        assert_eq!(cwd, "/Users/alex/code/demo-app");
        assert_eq!(dir, "-Users-alex-code-demo-app");
        assert_eq!(count, 5);

        // The folder name is lossy; the cwd from the entries is not.
        let cwd: String = conn
            .query_row(
                "SELECT project_cwd FROM sessions WHERE id LIKE '%003'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cwd, "/Users/alex/code/my.app");
    }

    #[test]
    fn last_activity_uses_latest_timestamp_not_last_line() {
        let conn = ingested();
        let last: String = conn
            .query_row(
                "SELECT last_activity_at FROM tasks WHERE session_id LIKE '%001' AND seq = 0",
                [],
                |r| r.get(0),
            )
            .unwrap();
        // The final assistant line is stamped 10:02:50 but a result came at 10:03:00.
        assert_eq!(last, "2026-11-01T10:03:00.000Z");
    }

    #[test]
    fn every_entry_after_the_first_prompt_has_a_task() {
        let conn = ingested();
        let orphans: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM entries e JOIN tasks t0
                   ON t0.session_id = e.session_id AND t0.seq = 0 AND t0.agent_id IS NULL
                 WHERE e.task_id IS NULL AND e.agent_id IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(orphans, 0);
    }

    #[test]
    fn rebuild_is_repeatable_and_keeps_ids() {
        let mut conn = ingested();
        let ids = |c: &Connection| -> Vec<String> {
            c.prepare("SELECT id FROM tasks ORDER BY session_id, seq")
                .unwrap()
                .query_map([], |r| r.get(0))
                .unwrap()
                .map(Result::unwrap)
                .collect()
        };
        let before = ids(&conn);
        conn.execute("DELETE FROM tasks", []).unwrap();
        conn.execute("DELETE FROM sessions", []).unwrap();
        conn.execute("UPDATE entries SET task_id = NULL", [])
            .unwrap();
        assert_eq!(rebuild_all(&mut conn).unwrap(), 4);
        assert_eq!(ids(&conn), before);
    }

    #[test]
    fn titles_take_the_first_line_and_are_shortened() {
        assert_eq!(title_from("\n  first line \nsecond"), "first line");
        assert_eq!(title_from("   "), "(untitled)");
        assert_eq!(
            title_from(&"x".repeat(200)).chars().count(),
            TITLE_MAX_CHARS + 1
        );
    }
}
