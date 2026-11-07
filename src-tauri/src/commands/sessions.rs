use std::collections::BTreeMap;

use serde::Serialize;
use tauri::State;

use super::tasks::Project;
use crate::error::AppResult;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub project: Project,
    pub session_count: i64,
    pub last_activity_at: Option<String>,
}

/// Projects that have at least one session, most recently active first.
#[tauri::command]
pub fn list_projects(state: State<'_, AppState>) -> AppResult<Vec<ProjectSummary>> {
    state.with_db(|conn| {
        let mut stmt =
            conn.prepare("SELECT project_cwd, project_dir, last_activity_at FROM sessions")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, Option<String>>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })?;
        let mut by_key: BTreeMap<String, ProjectSummary> = BTreeMap::new();
        for row in rows {
            let (cwd, dir, last) = row?;
            let project = Project::from_parts(cwd.as_deref(), &dir);
            let entry = by_key
                .entry(project.key.clone())
                .or_insert_with(|| ProjectSummary {
                    project,
                    session_count: 0,
                    last_activity_at: None,
                });
            entry.session_count += 1;
            if last > entry.last_activity_at {
                entry.last_activity_at = last;
            }
        }
        let mut out: Vec<ProjectSummary> = by_key.into_values().collect();
        out.sort_by(|a, b| b.last_activity_at.cmp(&a.last_activity_at));
        Ok(out)
    })
}
