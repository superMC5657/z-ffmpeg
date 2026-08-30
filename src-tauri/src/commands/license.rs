//! 授权相关 Tauri 命令：状态查询 / 激活 / 注销激活。
//! HTTP 调用统一放阻塞线程（10s 超时），避免卡住 async runtime。

use tauri::State;

use crate::error::{AppError, AppResult};
use crate::license::LicenseStatus;
use crate::AppState;

/// 读取当前授权状态（Free / Pro + 到期时间 + 离线宽限期标记）
#[tauri::command]
pub async fn get_license_status(state: State<'_, AppState>) -> AppResult<LicenseStatus> {
    Ok(state.license.status())
}

/// 激活：code + email 绑定当前设备。重复激活幂等（覆盖本地令牌）。
#[tauri::command]
pub async fn activate_license(
    state: State<'_, AppState>,
    code: String,
    email: String,
) -> AppResult<LicenseStatus> {
    let manager = state.license.clone();
    let result = tauri::async_runtime::spawn_blocking(move || manager.activate(&code, &email))
        .await
        .map_err(|e| AppError::Internal(format!("激活任务异常终止: {e}")))?
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(result)
}

/// 注销激活：解除设备绑定、释放一个名额，删除本地令牌并停用专业功能。
#[tauri::command]
pub async fn deactivate_license(state: State<'_, AppState>) -> AppResult<LicenseStatus> {
    let manager = state.license.clone();
    let result = tauri::async_runtime::spawn_blocking(move || manager.deactivate())
        .await
        .map_err(|e| AppError::Internal(format!("注销任务异常终止: {e}")))?
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(result)
}
