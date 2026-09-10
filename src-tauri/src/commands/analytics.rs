//! 埋点辅助命令：UI 侧行为计数。
//! 会话聚合上报本体在 `analytics::report`（退出时触发），这里只负责计数。

use crate::analytics;
use crate::error::AppResult;

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

