use std::path::Path;

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::db::queries;
use crate::error::{AppError, AppResult};
use crate::ingest::runner::IngestMsg;
use crate::ingest::scanner;
use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub version: String,
}

#[tauri::command]
pub fn app_info(app: AppHandle) -> AppResult<AppInfo> {
    let pkg = app.package_info();
    Ok(AppInfo {
        name: pkg.name.clone(),
        version: pkg.version.to_string(),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// The override, if one is set.
    pub projects_root: Option<String>,
    /// Where Claude Code keeps sessions on this machine.
    pub default_projects_root: Option<String>,
    pub stale_minutes: i64,
    /// Whether to look for a newer version in the background at startup.
    pub update_check_on_startup: bool,
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> AppResult<Settings> {
    state.with_db(|conn| {
        Ok(Settings {
            projects_root: queries::get_setting(conn, "projects_root")?,
            default_projects_root: scanner::default_projects_root()
                .ok()
                .map(|p| p.to_string_lossy().into_owned()),
            stale_minutes: queries::stale_minutes(conn)?,
            update_check_on_startup: queries::get_setting(conn, "update_check_on_startup")?
                .is_none_or(|v| v != "false"),
        })
    })
}

/// Sets or clears the session folder override, then rescans.
#[tauri::command]
pub fn set_projects_root(state: State<'_, AppState>, path: Option<String>) -> AppResult<()> {
    let path = path.map(|p| p.trim().to_string()).filter(|p| !p.is_empty());
    if let Some(p) = &path
        && !Path::new(p).is_dir()
    {
        return Err(AppError::Other(format!("There is no folder at {p}.")));
    }
    state.with_db(|conn| match &path {
        Some(p) => queries::set_setting(conn, "projects_root", p),
        None => queries::delete_setting(conn, "projects_root"),
    })?;
    state
        .ingest_tx
        .send(IngestMsg::Rescan)
        .map_err(|_| AppError::Other("The ingest thread is not running.".into()))
}

#[tauri::command]
pub fn set_stale_minutes(state: State<'_, AppState>, minutes: i64) -> AppResult<()> {
    if !(1..=24 * 60).contains(&minutes) {
        return Err(AppError::Other(
            "Choose between 1 minute and 24 hours.".into(),
        ));
    }
    state.with_db(|conn| queries::set_setting(conn, "stale_minutes", &minutes.to_string()))
}

#[tauri::command]
pub fn set_update_check_on_startup(state: State<'_, AppState>, enabled: bool) -> AppResult<()> {
    state.with_db(|conn| {
        queries::set_setting(
            conn,
            "update_check_on_startup",
            if enabled { "true" } else { "false" },
        )
    })
}
