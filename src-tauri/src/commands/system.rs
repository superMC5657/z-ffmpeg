use serde::{Deserialize, Serialize};
use tauri::State;
use crate::encoder::hw_accel::{self, HwAccelInfo};
use crate::error::AppResult;
use crate::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    pub hw_accels: Vec<HwAccelInfo>,
    pub ffmpeg_version: Option<String>,
    pub ffmpeg_path: Option<String>,
    pub cpu_name: String,
    pub cpu_cores: u32,
    pub total_memory_gb: f64,
    pub platform: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FfmpegStatusInfo {
    pub status: String,
    pub version: Option<String>,
    pub path: Option<String>,
    pub download_progress: Option<f64>,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn detect_hw_accel(
    state: State<'_, AppState>,
) -> AppResult<SystemInfo> {
    let sys = sysinfo::System::new_all();
    let cpu = sys.cpus();
    let cpu_name = cpu
        .first()
        .map(|c| c.brand().to_string())
        .unwrap_or_else(|| "Unknown CPU".into());
    let cpu_cores = cpu.len() as u32;
    let total_memory_gb = sys.total_memory() as f64 / (1024.0 * 1024.0 * 1024.0);

    let ffmpeg_status = state.ffmpeg_status.lock().clone();
    let platform = std::env::consts::OS.to_string();
    let detect_cpu = cpu_name.clone();
    let detect_platform = platform.clone();

    // Detect hardware accelerators via ffmpeg + platform/CPU constraints.
    // `detect_all` 内部会跑阻塞的 ffmpeg -encoders 子进程,
    // 放到 blocking 线程池执行,避免卡住 async runtime。
    let hw_accels = tauri::async_runtime::spawn_blocking(move || {
        hw_accel::detect_all(&detect_cpu, &detect_platform)
    })
    .await
    .map_err(|e| {
        crate::error::AppError::Internal(format!("硬件加速检测任务异常终止: {e}"))
    })?;

    Ok(SystemInfo {
        hw_accels,
        ffmpeg_version: ffmpeg_status.version.clone(),
        ffmpeg_path: ffmpeg_status.ffmpeg_path.clone(),
        cpu_name,
        cpu_cores,
        total_memory_gb: (total_memory_gb * 10.0).round() / 10.0,
        platform,
    })
}

#[tauri::command]
pub async fn get_system_info(
    state: State<'_, AppState>,
) -> AppResult<SystemInfo> {
    detect_hw_accel(state).await
}

#[tauri::command]
pub async fn check_ffmpeg_status(
    state: State<'_, AppState>,
) -> AppResult<FfmpegStatusInfo> {
    let status = state.ffmpeg_status.lock();

    let status_str = if status.available {
        "installed"
    } else {
        "not-installed"
    };

    Ok(FfmpegStatusInfo {
        status: status_str.into(),
        version: status.version.clone(),
        path: status.ffmpeg_path.clone(),
        download_progress: None,
        error: None,
    })
}

#[tauri::command]
pub async fn download_ffmpeg(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> AppResult<FfmpegStatusInfo> {
    use tauri::Emitter;

    match crate::ffmpeg::downloader::download_ffmpeg(&app, &state.ffmpeg_status).await {
        Ok(status) => {
            let info = FfmpegStatusInfo {
                status: "installed".into(),
                version: status.version.clone(),
                path: status.ffmpeg_path.clone(),
                download_progress: Some(100.0),
                error: None,
            };
            let _ = app.emit("ffmpeg://ready", &info);
            Ok(info)
        }
        Err(e) => {
            // 失败时也广播状态:Sidebar 靠 download-progress 事件进入了
            // "downloading" 状态,若没有失败事件,footer 会一直卡在
            // "正在下载..."直到重启。命令本身的 Err 照常返回给调用方。
            let info = FfmpegStatusInfo {
                status: "error".into(),
                version: None,
                path: None,
                download_progress: None,
                error: Some(e.to_string()),
            };
            let _ = app.emit("ffmpeg://error", &info);
            Err(e)
        }
    }
}
