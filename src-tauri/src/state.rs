//! Shared application state held by Tauri.

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::mpsc::Sender;

use rusqlite::Connection;
use serde::Serialize;

use crate::error::{AppError, AppResult};
use crate::ingest::runner::IngestMsg;
use crate::ingest::scanner::{ScanProgress, ScanSummary};

pub struct AppState {
    pub db_path: PathBuf,
    /// None when the archive could not be opened; `db_error` says why.
    pub db: Mutex<Option<Connection>>,
    pub db_error: Option<String>,
    pub ingest: Mutex<IngestState>,
    /// Wakes the ingest thread, for example to rescan after a settings change.
    pub ingest_tx: Sender<IngestMsg>,
}

impl AppState {
    /// Runs `f` with the UI's connection, or returns why there is none.
    pub fn with_db<T>(&self, f: impl FnOnce(&Connection) -> AppResult<T>) -> AppResult<T> {
        let guard = self
            .db
            .lock()
            .map_err(|_| AppError::Other("The database lock was poisoned.".into()))?;
        match guard.as_ref() {
            Some(conn) => f(conn),
            None => Err(AppError::Other(
                self.db_error
                    .clone()
                    .unwrap_or_else(|| "The archive is not open.".into()),
            )),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase", tag = "phase")]
pub enum IngestState {
    #[default]
    Idle,
    Scanning {
        progress: ScanProgress,
    },
    Ready {
        summary: ScanSummary,
        root: String,
        root_exists: bool,
    },
    Failed {
        message: String,
    },
}
