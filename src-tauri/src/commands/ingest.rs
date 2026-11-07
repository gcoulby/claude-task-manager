use tauri::State;

use crate::error::{AppError, AppResult};
use crate::state::{AppState, IngestState};

#[tauri::command]
pub fn ingest_state(state: State<'_, AppState>) -> AppResult<IngestState> {
    if let Some(e) = &state.db_error {
        return Ok(IngestState::Failed { message: e.clone() });
    }
    state
        .ingest
        .lock()
        .map(|g| g.clone())
        .map_err(|_| AppError::Other("The ingest state lock was poisoned.".into()))
}
