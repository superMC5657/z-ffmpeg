//! 埋点辅助命令：UI 侧行为计数 + 埋点开关读写。
//! 会话聚合上报本体在 `analytics::report`（退出时触发），这里只负责计数与开关。

use tauri::State;

use crate::analytics;
use crate::error::AppResult;
use crate::queue::settings::SETTINGS_KEY_ANALYTICS_ENABLED;
use crate::AppState;

/// 记录一个纯 UI 侧行为事件（页面导航、主题切换等后端看不到的行为）。
/// 只累加内存计数器，随会话退出一次性上报。
#[tauri::command]
pub fn track_event(name: String) -> AppResult<()> {
    // 名称做基本收敛：去空白、限长，避免恶意/异常输入撑大负载
    let name = name.trim();
    if name.is_empty() || name.len() > 64 {
        return Ok(());
    }
    analytics::record_event(name);
    Ok(())
}

/// 读取埋点开关（默认开启）
#[tauri::command]
pub async fn get_analytics_enabled(state: State<'_, AppState>) -> AppResult<bool> {
    let queue = state.queue_manager.as_ref().ok_or_else(|| {
        crate::error::AppError::Internal("Queue not initialized".into())
    })?;
    Ok(queue.get_setting_usize(SETTINGS_KEY_ANALYTICS_ENABLED, 1) != 0)
}

/// 保存埋点开关（退出时上报据此决定是否发送）
#[tauri::command]
pub async fn set_analytics_enabled(state: State<'_, AppState>, enabled: bool) -> AppResult<bool> {
    let queue = state.queue_manager.as_ref().ok_or_else(|| {
        crate::error::AppError::Internal("Queue not initialized".into())
    })?;
    queue.set_setting_usize(SETTINGS_KEY_ANALYTICS_ENABLED, if enabled { 1 } else { 0 });
    log::info!("analytics enabled set to {enabled}");
    Ok(enabled)
}
