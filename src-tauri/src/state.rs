//! Shared application state held by Tauri.

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::mpsc::Sender;

use rusqlite::Connection;
use serde::Serialize;

use crate::error::{AppError, AppResult};
use crate::ingest::runner::IngestMsg;
use crate::ingest::scanner::{ScanProgress, ScanSummary};
use crate::usage::{PlanUsage, UsageMsg};

pub struct AppState {
    pub db_path: PathBuf,
    /// None when the archive could not be opened; `db_error` says why.
    pub db: Mutex<Option<Connection>>,
    pub db_error: Option<String>,
    pub ingest: Mutex<IngestState>,
    /// Wakes the ingest thread, for example to rescan after a settings change.
    pub ingest_tx: Sender<IngestMsg>,
    /// The latest `/usage` result, if one has been fetched.
    pub plan_usage: Mutex<Option<PlanUsage>>,
    pub usage_tx: Sender<UsageMsg>,
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
// rename_all only renames the variants; rename_all_fields is what turns
// root_exists into rootExists for the UI.
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "phase"
)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ingest_state_serialises_with_camel_case_fields() {
        let ready = IngestState::Ready {
            summary: ScanSummary::default(),
            root: "/r".into(),
            root_exists: true,
        };
        let json = serde_json::to_value(&ready).unwrap();
        assert_eq!(json["phase"], "ready");
        assert_eq!(json["rootExists"], true);
        assert_eq!(json["summary"]["filesRead"], 0);
    }
}
