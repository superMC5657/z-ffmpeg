use serde::{Deserialize, Serialize};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use crate::encoder::engine;
use crate::error::{AppError, AppResult};

// Re-export types for convenience
pub use crate::encoder::codec::{
    EncodeConfig,
};

/// File info returned by probe_file
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
    pub pixel_format: Option<String>,
}

// ============================================================
// Commands
// ============================================================

#[tauri::command]
pub async fn probe_file(file_path: String) -> AppResult<FileInfo> {
    let path = std::path::Path::new(&file_path);
    if !path.exists() {
        return Err(AppError::InvalidConfig(format!(
            "File not found: {}",
            file_path
        )));
    }

    // Use ffprobe to get detailed info (async — doesn't block the runtime)
    let json = engine::probe_file_async(&file_path).await?;
    let info = engine::parse_probe_result(&json, &file_path)?;
    Ok(info)
}

#[tauri::command]
pub async fn start_encode(
    app_handle: tauri::AppHandle,
    _state: tauri::State<'_, crate::AppState>,
    config: EncodeConfig,
    input_path: String,
    output_path: String,
    job_id: String,
) -> AppResult<()> {
    log::info!("start_encode: {} -> {} (job: {})", input_path, output_path, job_id);

    let cancel = Arc::new(AtomicBool::new(false));

    let app_handle_clone = app_handle.clone();
    let config_clone = config.clone();
    let input_path_clone = input_path.clone();
    let output_path_clone = output_path.clone();
    let job_id_clone = job_id.clone();
    let cancel_clone = cancel.clone();

    // Run encoding in a blocking thread
    tokio::task::spawn_blocking(move || {
        match engine::start_encode(
            app_handle_clone,
            job_id_clone,
            config_clone,
            input_path_clone,
            output_path_clone,
            cancel_clone,
        ) {
            Ok(()) => {}
            Err(e) => {
                log::error!("Encoding error: {:?}", e);
            }
        }
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(())
}

#[tauri::command]
pub async fn cancel_encode(
    _state: tauri::State<'_, crate::AppState>,
    job_id: String,
) -> AppResult<()> {
    log::info!("cancel_encode: {}", job_id);

    if engine::cancel_process(&job_id) {
        log::info!("Cancellation signal sent to ffmpeg process {}", job_id);
    } else {
        log::warn!("No active ffmpeg process found for job {}", job_id);
    }

    Ok(())
}

/// Build display-ready ffmpeg command lines from a codec config — one per
/// input file. The output path for each is derived the same way as the queue
/// (input dir + `_encoded.ext`), honoring the configured output directory.
/// Colliding output paths within the batch get a numeric suffix so the
/// preview never shows two commands writing the same file.
#[tauri::command]
pub async fn build_ffmpeg_commands(
    config: EncodeConfig,
    files: Vec<String>,
    output_dir: Option<String>,
) -> AppResult<Vec<String>> {
    let outputs = engine::derive_output_paths_unique(&files, &config, output_dir.as_deref());
    let cmds: Vec<String> = files
        .iter()
        .zip(outputs.iter())
        .map(|(f, out)| engine::build_ffmpeg_command_line(&config, f, out))
        .collect();
    log::info!("Built {} ffmpeg command preview(s)", cmds.len());
    Ok(cmds)
}

/// Write a text file (e.g. a saved ffmpeg command) to the given path.
#[tauri::command]
pub async fn save_command_to_file(content: String, path: String) -> AppResult<()> {
    std::fs::write(&path, content)
        .map_err(|e| AppError::Io(e))?;
    log::info!("Saved command to file: {}", path);
    Ok(())
}
