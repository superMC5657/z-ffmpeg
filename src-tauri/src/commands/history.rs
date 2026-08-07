use serde::{Deserialize, Serialize};
use crate::error::AppResult;
use crate::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: String,
    pub input_path: String,
    pub output_path: String,
    pub file_name: String,
    pub status: String,
    pub vmaf_score: Option<f64>,
    pub vmaf_detail: Option<String>,
    pub output_size: Option<u64>,
    pub input_size: Option<u64>,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn get_history(
    state: tauri::State<'_, AppState>,
) -> AppResult<Vec<HistoryEntry>> {
    // History is read straight from the database so it survives app restarts
    // (the in-memory queue only restores active jobs on startup).
    let queue = match state.queue_manager.as_ref() {
        Some(q) => q,
        None => return Ok(vec![]),
    };

    let history: Vec<HistoryEntry> = queue
        .history()
        .into_iter()
        .map(|j| HistoryEntry {
            id: j.id,
            input_path: j.input_path,
            output_path: j.output_path,
            file_name: j.file_name,
            status: j.status,
            vmaf_score: j.vmaf_score,
            vmaf_detail: j.vmaf_detail,
            output_size: j.output_size,
            input_size: j.input_size,
            created_at: j.created_at,
            completed_at: j.completed_at,
            error: j.error,
        })
        .collect();

    Ok(history)
}

/// Delete specific history entries by id.
#[tauri::command]
pub async fn delete_history(
    state: tauri::State<'_, AppState>,
    ids: Vec<String>,
) -> AppResult<()> {
    let queue = match state.queue_manager.as_ref() {
        Some(q) => q,
        None => return Ok(()),
    };
    queue.delete_history(&ids);
    log::info!("Deleted {} history entries", ids.len());
    Ok(())
}

/// Clear ALL history entries (Completed / Failed / Cancelled).
#[tauri::command]
pub async fn clear_history(
    state: tauri::State<'_, AppState>,
) -> AppResult<()> {
    let queue = match state.queue_manager.as_ref() {
        Some(q) => q,
        None => return Ok(()),
    };
    queue.clear_history();
    log::info!("Cleared all history entries");
    Ok(())
}
