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

/// 分页后的历史结果：entries 为当前页，total 为筛选后的总条数
/// （供前端计算页数），与前端 `HistoryPageResult` 对齐。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPageResult {
    pub entries: Vec<HistoryEntry>,
    pub total: usize,
}

/// 读取编码历史。所有参数可选：
/// - `limit`/`offset`：分页（limit 缺省 = 不分页，全量返回，兼容旧调用）；
/// - `status`：按状态过滤（Completed / Failed / Cancelled）；
/// - `search`：按文件路径模糊搜索。
#[tauri::command]
pub async fn get_history(
    state: tauri::State<'_, AppState>,
    limit: Option<usize>,
    offset: Option<usize>,
    status: Option<String>,
    search: Option<String>,
) -> AppResult<HistoryPageResult> {
    // History is read straight from the database so it survives app restarts
    // (the in-memory queue only restores active jobs on startup).
    let queue = match state.queue_manager.as_ref() {
        Some(q) => q,
        None => return Ok(HistoryPageResult { entries: vec![], total: 0 }),
    };

    let (snapshots, total) = queue.history_filtered(
        limit,
        offset.unwrap_or(0),
        status.as_deref(),
        search.as_deref(),
    );

    let entries: Vec<HistoryEntry> = snapshots
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

    Ok(HistoryPageResult { entries, total })
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
