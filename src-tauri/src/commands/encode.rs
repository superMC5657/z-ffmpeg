use serde::{Deserialize, Serialize};
use crate::encoder::{args, estimate, probe};
use crate::error::{AppError, AppResult};

// 为调用方便重新导出类型
pub use crate::encoder::codec::{
    EncodeConfig,
};

/// 由 probe_file 返回的文件元信息结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub path: String,
    pub file_name: String,
    pub file_size: u64,
    pub duration: Option<f64>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub frame_rate: Option<f64>,
    pub bitrate: Option<u64>,
    /// 源音频流码率（bps）；音频 Copy 时预估输出体积用，缺失时由容器总码率近似
    pub audio_bitrate: Option<u64>,
    pub pixel_format: Option<String>,
}

// ============================================================
// 命令处理器
// ============================================================

#[tauri::command]
pub async fn probe_file(file_path: String) -> AppResult<FileInfo> {
    // 日志只记 basename，不记全路径
    let basename = crate::encoder::file_name_from_path(&file_path);
    let path = std::path::Path::new(&file_path);
    if !path.exists() {
        log::warn!("probe failed file {basename} reason not found");
        return Err(AppError::InvalidConfig(format!(
            "File not found: {}",
            file_path
        )));
    }

    // 使用 ffprobe 异步获取详细信息（不阻塞异步运行时线程）
    let json = match probe::probe_file_async(&file_path).await {
        Ok(json) => json,
        Err(e) => {
            let msg = e.to_string();
            let top = msg.lines().next().unwrap_or("unknown").to_string();
            log::warn!("probe failed file {basename} reason {top}");
            return Err(e);
        }
    };
    let info = match probe::parse_probe_result(&json, &file_path) {
        Ok(info) => info,
        Err(e) => {
            let msg = e.to_string();
            let top = msg.lines().next().unwrap_or("unknown").to_string();
            log::warn!("probe failed file {basename} reason {top}");
            return Err(e);
        }
    };
    log::info!(
        "probe success file {basename} res={}x{} codec={} dur={:.1}s size={}B",
        info.width.unwrap_or(0),
        info.height.unwrap_or(0),
        info.video_codec.as_deref().unwrap_or("unknown"),
        info.duration.unwrap_or(0.0),
        info.file_size
    );
    Ok(info)
}

/// 根据编码配置为每个输入文件构建可展示的 ffmpeg 命令行。
/// 每个文件的输出路径与队列构建方式相同（输入目录 + `_encoded.ext`），
/// 同时遵从用户配置的输出目录。
/// 批处理中发生冲突的输出路径会添加数字后缀，避免预览中出现两条命令写入同一文件。
#[tauri::command]
pub async fn build_ffmpeg_commands(
    state: tauri::State<'_, crate::AppState>,
    config: EncodeConfig,
    files: Vec<String>,
    output_dir: Option<String>,
) -> AppResult<Vec<String>> {
    let active_outputs = state
        .queue_manager
        .as_ref()
        .map(|q| q.get_active_output_paths())
        .unwrap_or_default();
    let outputs = args::derive_output_paths_unique_with_claimed(
        &files,
        &config,
        output_dir.as_deref(),
        &active_outputs,
    );
    let cmds: Vec<String> = files
        .iter()
        .zip(outputs.iter())
        .map(|(f, out)| args::build_ffmpeg_command_line(&config, f, out))
        .collect();
    Ok(cmds)
}

/// 将文本内容（如保存的 ffmpeg 命令）写入指定路径。
/// Pro 功能：把命令保存为 .txt/.bat/.sh 文件（复制到剪贴板保持免费）。
#[tauri::command]
pub async fn save_command_to_file(
    state: tauri::State<'_, crate::AppState>,
    content: String,
    path: String,
) -> AppResult<()> {
    state.license.ensure_pro("命令导出为文件")?;

    std::fs::write(&path, content)
        .map_err(AppError::Io)?;
    crate::analytics::bump(&crate::analytics::COUNTERS.commands_exported, 1);
    Ok(())
}

pub use crate::encoder::estimate::EstimatedSize;

/// 按当前编码参数预估每个输入文件压缩后的输出体积区间（字节），编码页实时预览用。
/// 纯算术计算、无 I/O（文件信息由前端 `probe_file` 探测后传入），参数变化时可
/// 反复调用。探测信息不足（无时长 / 无码率）的对应项返回 `None`。
#[tauri::command]
pub fn estimate_output_sizes(
    config: EncodeConfig,
    files: Vec<FileInfo>,
) -> Vec<Option<EstimatedSize>> {
    files
        .iter()
        .map(|f| estimate::estimate_output_size_from_info(&config, f))
        .collect()
}
