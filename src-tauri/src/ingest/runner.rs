//! Runs ingest off the main thread and reports progress as Tauri events.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager};

use super::scanner;
use crate::db;
use crate::error::AppResult;
use crate::state::{AppState, IngestState};

pub const EVENT_INGEST: &str = "ingest://state";

/// Progress events are throttled so a large first scan does not flood the UI.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

pub fn projects_root(state: &AppState) -> AppResult<PathBuf> {
    let custom = state.with_db(|c| db::queries::get_setting(c, "projects_root"))?;
    match custom.filter(|p| !p.trim().is_empty()) {
        Some(p) => Ok(PathBuf::from(p)),
        None => scanner::default_projects_root(),
    }
}

pub fn spawn_initial_scan(app: AppHandle) {
    tauri::async_runtime::spawn_blocking(move || {
        let result = run_scan(&app);
        if let Err(e) = result {
            set_state(
                &app,
                IngestState::Failed {
                    message: e.to_string(),
                },
            );
        }
    });
}

fn run_scan(app: &AppHandle) -> AppResult<()> {
    let state = app.state::<AppState>();
    let root = projects_root(&state)?;
    // A separate connection so UI reads are never blocked behind ingest.
    let mut conn = db::open_existing(&state.db_path)?;
    let mut last_emit = Instant::now() - PROGRESS_INTERVAL;
    let summary = scanner::scan(&mut conn, &root, |progress| {
        let done = progress.files_done == progress.files_total;
        if done || last_emit.elapsed() >= PROGRESS_INTERVAL {
            last_emit = Instant::now();
            set_state(
                app,
                IngestState::Scanning {
                    progress: progress.clone(),
                },
            );
        }
    })?;
    set_state(app, IngestState::Ready { summary });
    Ok(())
}

fn set_state(app: &AppHandle, next: IngestState) {
    let state = app.state::<AppState>();
    if let Ok(mut guard) = state.ingest.lock() {
        *guard = next.clone();
    }
    // Emitting only fails once the app is shutting down, when nobody is listening.
    let _ = app.emit(EVENT_INGEST, next);
}
