use std::collections::BTreeMap;

use serde::Serialize;
use tauri::State;

use super::tasks::Project;
use crate::db::queries::{self, ArchiveCounts};
use crate::error::AppResult;
use crate::ingest::scanner;
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveInfo {
    pub counts: ArchiveCounts,
    pub db_path: String,
    pub db_bytes: u64,
    /// How long Claude Code keeps its own transcripts.
    pub retention_days: i64,
    pub retention_is_default: bool,
}

#[tauri::command]
pub fn archive_info(state: State<'_, AppState>) -> AppResult<ArchiveInfo> {
    let counts = state.with_db(queries::archive_counts)?;
    let (retention_days, retention_is_default) = scanner::claude_retention_days();
    // The write-ahead log holds recent writes, so count it towards the size.
    let size = |p: &std::path::Path| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    let mut wal = state.db_path.clone().into_os_string();
    wal.push("-wal");
    Ok(ArchiveInfo {
        counts,
        db_path: state.db_path.to_string_lossy().into_owned(),
        db_bytes: size(&state.db_path) + size(std::path::Path::new(&wal)),
        retention_days,
        retention_is_default,
    })
}
