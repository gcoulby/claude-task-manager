use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::queries::{self, TokenTotals};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::usage::{PlanUsage, UsageMsg};

#[tauri::command]
pub fn get_plan_usage(state: State<'_, AppState>) -> AppResult<Option<PlanUsage>> {
    state
        .plan_usage
        .lock()
        .map(|g| g.clone())
        .map_err(|_| AppError::Other("The usage lock was poisoned.".into()))
}

#[tauri::command]
pub fn refresh_plan_usage(state: State<'_, AppState>) -> AppResult<()> {
    state
        .usage_tx
        .send(UsageMsg::Refresh)
        .map_err(|_| AppError::Other("The usage checker is not running.".into()))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenRange {
    pub key: String,
    /// ISO timestamp; the UI works out local midnight and so on. None for all time.
    pub since: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenRangeTotals {
    pub key: String,
    pub totals: TokenTotals,
}

#[tauri::command]
pub fn token_usage(
    state: State<'_, AppState>,
    ranges: Vec<TokenRange>,
) -> AppResult<Vec<TokenRangeTotals>> {
    state.with_db(|conn| {
        ranges
            .into_iter()
            .map(|r| {
                Ok(TokenRangeTotals {
                    totals: queries::token_totals_since(conn, r.since.as_deref())?,
                    key: r.key,
                })
            })
            .collect()
    })
}
