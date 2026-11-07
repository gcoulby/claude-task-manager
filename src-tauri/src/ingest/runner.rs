//! The ingest thread: a full scan, then incremental updates from the file
//! watcher, all off the main thread on a connection of its own.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use super::scanner::{self, ScanSummary};
use super::watcher::{self, WatchEvent};
use crate::db;
use crate::error::AppResult;
use crate::state::{AppState, IngestState};

pub const EVENT_INGEST: &str = "ingest://state";
pub const EVENT_ARCHIVE_CHANGED: &str = "archive://changed";

/// Progress events are throttled so a large first scan does not flood the UI.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/// How often to look for a projects folder that does not exist yet.
const MISSING_ROOT_RETRY: Duration = Duration::from_secs(30);

pub enum IngestMsg {
    Watch(WatchEvent),
    /// The projects folder setting changed: scan again and re-watch.
    Rescan,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ArchiveChanged {
    session_ids: Vec<String>,
}

pub fn projects_root(state: &AppState) -> AppResult<PathBuf> {
    let custom = state.with_db(|c| db::queries::get_setting(c, "projects_root"))?;
    match custom.filter(|p| !p.trim().is_empty()) {
        Some(p) => Ok(PathBuf::from(p)),
        None => scanner::default_projects_root(),
    }
}

/// Starts the ingest thread. `tx` must be the sender paired with `rx`; the
/// watcher uses a clone of it.
pub fn spawn(app: AppHandle, tx: Sender<IngestMsg>, rx: Receiver<IngestMsg>) {
    let spawned = std::thread::Builder::new()
        .name("ingest".into())
        .spawn(move || {
            if let Err(e) = run(&app, tx, rx) {
                set_state(
                    &app,
                    IngestState::Failed {
                        message: e.to_string(),
                    },
                );
            }
        });
    if let Err(e) = spawned {
        eprintln!("could not start the ingest thread: {e}");
    }
}

fn run(app: &AppHandle, tx: Sender<IngestMsg>, rx: Receiver<IngestMsg>) -> AppResult<()> {
    let state = app.state::<AppState>();
    let mut conn = db::open_existing(&state.db_path)?;

    // Each pass of this loop is one "generation": a full scan of the current
    // root and a watcher on it, until a rescan is requested.
    loop {
        let root = projects_root(&state)?;
        let summary = full_scan(app, &mut conn, &root)?;
        let root_exists = root.is_dir();
        let _watcher = if root_exists {
            match watcher::watch(&root, watcher::to_channel(tx.clone(), IngestMsg::Watch)) {
                Ok(w) => Some(w),
                Err(e) => {
                    set_state(
                        app,
                        IngestState::Failed {
                            message: format!(
                                "Archived {} files, but cannot watch {} for changes: {e}",
                                summary.files,
                                root.display()
                            ),
                        },
                    );
                    None
                }
            }
        } else {
            None
        };

        let rescan = listen(app, &mut conn, &root, &rx, root_exists);
        if !rescan {
            return Ok(());
        }
    }
}

/// Handles watcher batches until a rescan is needed (true) or the app is
/// closing (false).
fn listen(
    app: &AppHandle,
    conn: &mut rusqlite::Connection,
    root: &Path,
    rx: &Receiver<IngestMsg>,
    root_exists: bool,
) -> bool {
    loop {
        let msg = if root_exists {
            match rx.recv() {
                Ok(m) => m,
                Err(_) => return false,
            }
        } else {
            match rx.recv_timeout(MISSING_ROOT_RETRY) {
                Ok(m) => m,
                Err(RecvTimeoutError::Timeout) if root.is_dir() => return true,
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => return false,
            }
        };
        match msg {
            IngestMsg::Rescan => return true,
            IngestMsg::Watch(WatchEvent::Changed(paths)) => {
                match scanner::ingest_changed(conn, root, &paths) {
                    Ok(ids) if !ids.is_empty() => {
                        let _ =
                            app.emit(EVENT_ARCHIVE_CHANGED, ArchiveChanged { session_ids: ids });
                    }
                    Ok(_) => {}
                    // A failed batch is not fatal; the next change retries it.
                    Err(e) => eprintln!("ingest of changed files failed: {e}"),
                }
            }
            // Usually a dropped event on a busy folder. A rescan catches up.
            IngestMsg::Watch(WatchEvent::Error(_)) => return true,
        }
    }
}

fn full_scan(
    app: &AppHandle,
    conn: &mut rusqlite::Connection,
    root: &Path,
) -> AppResult<ScanSummary> {
    let mut last_emit = Instant::now() - PROGRESS_INTERVAL;
    let summary = scanner::scan(conn, root, |progress| {
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
    set_state(
        app,
        IngestState::Ready {
            summary: summary.clone(),
            root: root.to_string_lossy().into_owned(),
            root_exists: root.is_dir(),
        },
    );
    if summary.sessions_rebuilt > 0 {
        let _ = app.emit(
            EVENT_ARCHIVE_CHANGED,
            ArchiveChanged {
                session_ids: Vec::new(),
            },
        );
    }
    Ok(summary)
}

fn set_state(app: &AppHandle, next: IngestState) {
    let state = app.state::<AppState>();
    if let Ok(mut guard) = state.ingest.lock() {
        *guard = next.clone();
    }
    // Emitting only fails once the app is shutting down, when nobody is listening.
    let _ = app.emit(EVENT_INGEST, next);
}
