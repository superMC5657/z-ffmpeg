use crate::error::AppResult;
use crate::queue::job::JobSnapshot;
use crate::AppState;
use serde::Serialize;

/// 分页后的历史结果：entries 为当前页，total 为筛选后的总条数
/// （供前端计算页数），与前端 `HistoryPageResult` 对齐。
/// 条目直接复用 `JobSnapshot`（前端历史页只读其子集字段，多余字段被忽略）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPageResult {
    pub entries: Vec<JobSnapshot>,
    pub total: usize,
}

/// 读取编码历史。所有参数可选：
/// - `limit`/`offset`：分页（limit 为 None 时全量返回）；
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
    // 历史记录直接从数据库读取，因此在应用重启后依然保留
    //（内存中的队列在启动时仅恢复活动任务）。
    let queue = match state.queue_manager.as_ref() {
        Some(q) => q,
        None => {
            return Ok(HistoryPageResult {
                entries: vec![],
                total: 0,
            })
        }
    };

    let (entries, total) = queue.history_filtered(
        limit,
        offset.unwrap_or(0),
        status.as_deref(),
        search.as_deref(),
    );

    Ok(HistoryPageResult { entries, total })
}

/// 按 ID 删除指定的历史记录条目。
#[tauri::command]
pub async fn delete_history(state: tauri::State<'_, AppState>, ids: Vec<String>) -> AppResult<()> {
    let queue = match state.queue_manager.as_ref() {
        Some(q) => q,
        None => return Ok(()),
    };
    queue.delete_history(&ids);
    Ok(())
}

/// 清空所有历史记录条目（已完成 / 失败 / 已取消）。
#[tauri::command]
pub async fn clear_history(state: tauri::State<'_, AppState>) -> AppResult<()> {
    let queue = match state.queue_manager.as_ref() {
        Some(q) => q,
        None => return Ok(()),
    };
    queue.clear_history();
    Ok(())
}
