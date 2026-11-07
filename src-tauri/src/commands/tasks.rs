//! Task list and detail. Staleness is applied here, at read time, because it
//! depends on the current time.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::queries::{self, StoredTask};
use crate::error::{AppError, AppResult};
use crate::ingest::parser;
use crate::ingest::status::{self, Status};
use crate::state::AppState;

/// Characters of each entry sent with the timeline. The rest is fetched on
/// demand when a row is expanded.
const PREVIEW_CHARS: i64 = 600;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    /// Real cwd when known, otherwise the encoded folder name.
    pub key: String,
    pub label: String,
    /// False when only the lossy folder name is known.
    pub is_real_path: bool,
}

impl Project {
    pub fn from_parts(cwd: Option<&str>, dir: &str) -> Self {
        match cwd {
            Some(cwd) => Project {
                key: cwd.to_string(),
                label: Path::new(cwd)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| cwd.to_string()),
                is_real_path: true,
            },
            None => Project {
                key: dir.to_string(),
                label: dir.to_string(),
                is_real_path: false,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRow {
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
    /// 0 for top-level tasks, 1 for subagent runs shown under their parent.
    pub depth: u8,
    pub child_count: usize,
    pub project: Project,
    pub session_title: Option<String>,
    pub source_missing: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskFilter {
    #[serde(default)]
    pub statuses: Vec<String>,
    pub project: Option<String>,
    pub query: Option<String>,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn to_row(t: StoredTask, now: i64, stale_ms: i64, child_count: usize) -> TaskRow {
    let (status, reason) = match Status::parse(&t.status) {
        Some(s) => {
            let (s, r) = status::with_staleness(s, &t.status_reason, t.activity_ms, now, stale_ms);
            (s.as_str().to_string(), r)
        }
        None => (t.status, t.status_reason),
    };
    TaskRow {
        project: Project::from_parts(t.project_cwd.as_deref(), &t.project_dir),
        depth: u8::from(t.parent_task_id.is_some()),
        id: t.id,
        session_id: t.session_id,
        parent_task_id: t.parent_task_id,
        agent_id: t.agent_id,
        title: t.title,
        trigger: t.trigger,
        status,
        status_reason: reason,
        started_at: t.started_at,
        ended_at: t.ended_at,
        last_activity_at: t.last_activity_at,
        entry_count: t.entry_count,
        child_count,
        session_title: t.session_title,
        source_missing: t.source_missing,
    }
}

/// Newest top-level tasks first, each followed by its subagent runs in the
/// order they started.
pub fn order_and_convert(tasks: Vec<StoredTask>, now: i64, stale_ms: i64) -> Vec<TaskRow> {
    let ids: HashSet<String> = tasks.iter().map(|t| t.id.clone()).collect();
    let mut children: HashMap<String, Vec<StoredTask>> = HashMap::new();
    let mut top: Vec<StoredTask> = Vec::new();
    for t in tasks {
        match t.parent_task_id.clone().filter(|p| ids.contains(p)) {
            Some(parent) => children.entry(parent).or_default().push(t),
            None => top.push(t),
        }
    }
    top.sort_by(|a, b| {
        b.last_activity_at
            .cmp(&a.last_activity_at)
            .then_with(|| a.seq.cmp(&b.seq))
    });
    let mut out = Vec::new();
    for t in top {
        let mut kids = children.remove(&t.id).unwrap_or_default();
        kids.sort_by_key(|k| k.seq);
        out.push(to_row(t, now, stale_ms, kids.len()));
        out.extend(kids.into_iter().map(|k| to_row(k, now, stale_ms, 0)));
    }
    out
}

fn matches(row: &TaskRow, filter: &TaskFilter, query: Option<&str>) -> bool {
    (filter.statuses.is_empty() || filter.statuses.contains(&row.status))
        && filter
            .project
            .as_ref()
            .is_none_or(|p| *p == row.project.key)
        && query.is_none_or(|q| row.title.to_lowercase().contains(q))
}

#[tauri::command]
pub fn list_tasks(
    state: State<'_, AppState>,
    filter: Option<TaskFilter>,
) -> AppResult<Vec<TaskRow>> {
    let filter = filter.unwrap_or_default();
    let query = filter
        .query
        .as_deref()
        .map(str::trim)
        .filter(|q| !q.is_empty())
        .map(str::to_lowercase);
    state.with_db(|conn| {
        let stale_ms = queries::stale_minutes(conn)? * 60_000;
        let rows = order_and_convert(queries::all_tasks(conn)?, now_ms(), stale_ms);
        Ok(rows
            .into_iter()
            .filter(|r| matches(r, &filter, query.as_deref()))
            .collect())
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryRow {
    pub id: i64,
    pub kind: String,
    pub tool_name: Option<String>,
    pub preview: String,
    /// True when the full text is longer than the preview or is a tool call
    /// whose full input is worth showing.
    pub expandable: bool,
    pub is_error: bool,
    pub ts: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskDetail {
    pub task: TaskRow,
    pub parent: Option<TaskRow>,
    pub children: Vec<TaskRow>,
    pub entries: Vec<EntryRow>,
    pub source_path: Option<String>,
    /// This task's responses plus any subagent runs it launched.
    pub tokens: queries::TokenTotals,
}

#[tauri::command]
pub fn get_task(state: State<'_, AppState>, task_id: String) -> AppResult<TaskDetail> {
    state.with_db(|conn| {
        let stale_ms = queries::stale_minutes(conn)? * 60_000;
        let rows = order_and_convert(
            queries::session_tasks_for(conn, &task_id)?,
            now_ms(),
            stale_ms,
        );
        let task = rows
            .iter()
            .find(|r| r.id == task_id)
            .cloned()
            .ok_or_else(|| AppError::Other("That task is no longer in the archive.".into()))?;
        let parent = task
            .parent_task_id
            .as_ref()
            .and_then(|p| rows.iter().find(|r| &r.id == p).cloned());
        let children = rows
            .iter()
            .filter(|r| r.parent_task_id.as_deref() == Some(task_id.as_str()))
            .cloned()
            .collect();

        let stored = queries::task_entries(conn, &task_id, PREVIEW_CHARS)?;
        // Results do not carry the tool's name, so borrow it from the call.
        let tool_names: HashMap<&str, &str> = stored
            .iter()
            .filter(|e| e.kind == "tool_use")
            .filter_map(|e| Some((e.tool_use_id.as_deref()?, e.tool_name.as_deref()?)))
            .collect();
        let entries = stored
            .iter()
            .map(|e| EntryRow {
                id: e.id,
                kind: e.kind.clone(),
                tool_name: e.tool_name.clone().or_else(|| {
                    e.tool_use_id
                        .as_deref()
                        .and_then(|id| tool_names.get(id))
                        .map(|n| n.to_string())
                }),
                expandable: e.summary_chars > PREVIEW_CHARS
                    || e.kind == "tool_use"
                    || e.preview.ends_with('…'),
                preview: e.preview.clone(),
                is_error: e.is_error,
                ts: e.ts.clone(),
            })
            .collect();
        let source_path = queries::main_source_path(conn, &task.session_id)?;
        let tokens = queries::token_totals_for_task(conn, &task_id)?;
        Ok(TaskDetail {
            tokens,
            task,
            parent,
            children,
            entries,
            source_path,
        })
    })
}

#[tauri::command]
pub fn get_entry_text(state: State<'_, AppState>, entry_id: i64) -> AppResult<String> {
    state.with_db(|conn| {
        let raw = queries::entry_raw_json(conn, entry_id)?
            .ok_or_else(|| AppError::Other("That entry is no longer in the archive.".into()))?;
        Ok(parser::full_text(&raw))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stored(id: &str, parent: Option<&str>, last: &str, seq: i64) -> StoredTask {
        StoredTask {
            id: id.into(),
            session_id: "s".into(),
            parent_task_id: parent.map(String::from),
            agent_id: parent.map(|_| format!("a-{id}")),
            title: format!("Task {id}"),
            trigger: "prompt".into(),
            status: "running".into(),
            status_reason: "Writing a response.".into(),
            started_at: None,
            ended_at: None,
            last_activity_at: Some(last.into()),
            entry_count: 1,
            seq,
            activity_ms: 1_000,
            session_title: None,
            project_cwd: Some("/Users/alex/code/demo-app".into()),
            project_dir: "-Users-alex-code-demo-app".into(),
            source_missing: false,
        }
    }

    #[test]
    fn orders_newest_first_with_children_under_parents() {
        let rows = order_and_convert(
            vec![
                stored("old", None, "2026-11-01T10:00:00Z", 0),
                stored("kid2", Some("old"), "2026-11-01T10:09:00Z", 3),
                stored("new", None, "2026-11-01T11:00:00Z", 1),
                stored("kid1", Some("old"), "2026-11-01T10:05:00Z", 2),
                stored("orphan", Some("gone"), "2026-11-01T09:00:00Z", 4),
            ],
            2_000,
            60_000,
        );
        let order: Vec<(&str, u8)> = rows.iter().map(|r| (r.id.as_str(), r.depth)).collect();
        assert_eq!(
            order,
            vec![
                ("new", 0),
                ("old", 0),
                ("kid1", 1),
                ("kid2", 1),
                ("orphan", 1)
            ]
        );
        assert_eq!(rows[1].child_count, 2);
        assert_eq!(rows[0].project.label, "demo-app");
    }

    #[test]
    fn staleness_is_applied_at_read_time() {
        let rows = order_and_convert(
            vec![stored("t", None, "x", 0)],
            1_000 + 20 * 60_000,
            15 * 60_000,
        );
        assert_eq!(rows[0].status, "stale");
        assert!(rows[0].status_reason.contains("Writing a response."));
    }

    #[test]
    fn filters_by_status_project_and_title() {
        let rows = order_and_convert(vec![stored("t", None, "x", 0)], 2_000, 60_000);
        let r = &rows[0];
        let f = |statuses: &[&str], project: Option<&str>| TaskFilter {
            statuses: statuses.iter().map(|s| s.to_string()).collect(),
            project: project.map(String::from),
            query: None,
        };
        assert!(matches(r, &f(&[], None), None));
        assert!(matches(r, &f(&["running", "waiting"], None), None));
        assert!(!matches(r, &f(&["done"], None), None));
        assert!(matches(r, &f(&[], Some("/Users/alex/code/demo-app")), None));
        assert!(!matches(r, &f(&[], Some("/elsewhere")), None));
        assert!(matches(r, &f(&[], None), Some("task t")));
        assert!(!matches(r, &f(&[], None), Some("nothing")));
    }

    #[test]
    fn project_falls_back_to_folder_name() {
        let p = Project::from_parts(None, "-Users-alex-code-my-app");
        assert_eq!(
            (p.label.as_str(), p.is_real_path),
            ("-Users-alex-code-my-app", false)
        );
    }
}

/// Most results shown for one search; refine the query for more.
const SEARCH_LIMIT: i64 = 200;

#[tauri::command]
pub fn search_entries(
    state: State<'_, AppState>,
    query: String,
) -> AppResult<Vec<queries::SearchHit>> {
    state.with_db(|conn| queries::search_entries(conn, &query, SEARCH_LIMIT))
}
