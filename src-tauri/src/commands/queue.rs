use tauri::{Emitter, State};
use crate::encoder::codec::EncodeConfig;
use crate::encoder::estimate;
use crate::encoder::engine;
use crate::queue::QueueStatus;
use crate::error::AppResult;

#[tauri::command]
pub async fn add_to_queue(
    app_handle: tauri::AppHandle,
    state: State<'_, crate::AppState>,
    files: Vec<String>,
    config: EncodeConfig,
    output_dir: Option<String>,
) -> AppResult<Vec<String>> {
    log::info!("add_to_queue: {} files", files.len());

    // Pro 门控：硬件加速 / 高级参数透传（后端强制，前端绕不过）
    crate::commands::ensure_config_allowed(&state.license, &config)?;

    // 埋点：入队规模、编码器分布、硬件加速使用
    crate::analytics::bump(&crate::analytics::COUNTERS.files_added, files.len() as u64);
    crate::analytics::bump(&crate::analytics::COUNTERS.jobs_added, files.len() as u64);
    crate::analytics::record_codec(&codec_key(&config), files.len() as u64);
    if config.hw_accel.is_some() {
        crate::analytics::bump(&crate::analytics::COUNTERS.hw_accel_jobs, files.len() as u64);
    }

    let queue = state.queue_manager.as_ref()
        .ok_or_else(|| crate::error::AppError::Internal("Queue not initialized".into()))?;

    // Build (input, output) pairs. Output paths are deduplicated within the
    // batch so two inputs with the same basename don't silently overwrite
    // each other (ffmpeg runs with `-y`).
    let outputs = engine::derive_output_paths_unique(&files, &config, output_dir.as_deref());
    let pairs: Vec<(String, String)> = files.iter().cloned().zip(outputs).collect();

    // 入队前探测每个输入文件，预估压缩后的输出体积（Pending 状态即可展示）。
    // 探测失败只影响预估，不阻塞入队。批量文件并行探测，避免串行等待；
    // 每个任务携带自身索引，保证估算值与输入文件一一对应（join_next 不保序）。
    let mut probes = tokio::task::JoinSet::new();
    for (idx, input) in files.iter().enumerate() {
        let input = input.clone();
        let config = config.clone();
        probes.spawn(async move {
            let est = match engine::probe_file_async(&input).await {
                Ok(json) => estimate::estimate_output_bytes(&config, &json),
                Err(_) => None,
            };
            (idx, est)
        });
    }
    let mut estimates: Vec<Option<u64>> = vec![None; files.len()];
    while let Some(res) = probes.join_next().await {
        if let Ok((idx, est)) = res {
            estimates[idx] = est;
        }
    }

    let ids = queue.add_jobs_estimated(pairs, estimates, config);

    // Emit updated queue state
    let status = queue.get_status();
    let _ = app_handle.emit("queue://updated", &status);

    Ok(ids)
}

/// Start processing the queue (user-triggered, not automatic on add)
#[tauri::command]
pub async fn start_queue(
    app_handle: tauri::AppHandle,
    state: State<'_, crate::AppState>,
) -> AppResult<()> {
    if let Some(queue) = state.queue_manager.as_ref() {
        log::info!("start_queue: user triggered processing");
        queue.process_queue(app_handle);
    }
    Ok(())
}

#[tauri::command]
pub async fn remove_from_queue(
    app_handle: tauri::AppHandle,
    state: State<'_, crate::AppState>,
    job_ids: Vec<String>,
) -> AppResult<()> {
    if let Some(queue) = state.queue_manager.as_ref() {
        queue.remove_jobs(&job_ids);
        let status = queue.get_status();
        let _ = app_handle.emit("queue://updated", &status);
    }
    Ok(())
}

/// Cancel a queue job: kills the running ffmpeg process (if any) and marks the job cancelled
#[tauri::command]
pub async fn cancel_job(
    app_handle: tauri::AppHandle,
    state: State<'_, crate::AppState>,
    job_id: String,
) -> AppResult<()> {
    if let Some(queue) = state.queue_manager.as_ref() {
        queue.cancel_job(&job_id);
        let status = queue.get_status();
        let _ = app_handle.emit("queue://updated", &status);
    }
    Ok(())
}

#[tauri::command]
pub async fn get_queue_status(
    state: State<'_, crate::AppState>,
) -> AppResult<QueueStatus> {
    match state.queue_manager.as_ref() {
        Some(queue) => Ok(queue.get_status()),
        None => Ok(QueueStatus {
            jobs: vec![], total: 0, pending: 0, encoding: 0, completed: 0, failed: 0,
            paused: false,
        }),
    }
}

/// 暂停队列自动调度：正在编码的任务继续，剩余 Pending 不再自动开始。
#[tauri::command]
pub async fn pause_queue(
    app_handle: tauri::AppHandle,
    state: State<'_, crate::AppState>,
) -> AppResult<()> {
    if let Some(queue) = state.queue_manager.as_ref() {
        queue.pause_queue();
        let status = queue.get_status();
        let _ = app_handle.emit("queue://updated", &status);
    }
    Ok(())
}

/// 解除队列暂停，并立即拉起调度处理剩余 Pending 任务。
#[tauri::command]
pub async fn resume_queue(
    app_handle: tauri::AppHandle,
    state: State<'_, crate::AppState>,
) -> AppResult<()> {
    if let Some(queue) = state.queue_manager.as_ref() {
        let was = queue.resume_queue();
        if was {
            queue.process_queue(app_handle.clone());
        }
        let status = queue.get_status();
        let _ = app_handle.emit("queue://updated", &status);
    }
    Ok(())
}

#[tauri::command]
pub async fn clear_completed(
    app_handle: tauri::AppHandle,
    state: State<'_, crate::AppState>,
) -> AppResult<()> {
    if let Some(queue) = state.queue_manager.as_ref() {
        queue.clear_completed();
        let status = queue.get_status();
        let _ = app_handle.emit("queue://updated", &status);
    }
    Ok(())
}

/// Re-queue a finished (Failed / Cancelled) job and start processing again.
#[tauri::command]
pub async fn retry_job(
    app_handle: tauri::AppHandle,
    state: State<'_, crate::AppState>,
    job_id: String,
) -> AppResult<bool> {
    let queue = state.queue_manager.as_ref()
        .ok_or_else(|| crate::error::AppError::Internal("Queue not initialized".into()))?;
    let retried = queue.retry_job(&job_id);
    if retried {
        queue.process_queue(app_handle.clone());
    }
    let status = queue.get_status();
    let _ = app_handle.emit("queue://updated", &status);
    Ok(retried)
}

/// Get the current max concurrent encoding jobs limit.
/// 免费版上限为 2（Pro 16）：读取时也收敛，避免历史设置值越界展示。
#[tauri::command]
pub async fn get_max_concurrent(
    state: State<'_, crate::AppState>,
) -> AppResult<usize> {
    let queue = state.queue_manager.as_ref()
        .ok_or_else(|| crate::error::AppError::Internal("Queue not initialized".into()))?;
    let cap = concurrency_cap(&state.license);
    Ok(queue.max_concurrent().min(cap))
}

/// Set the max concurrent encoding jobs limit (clamped to 1..=16, persisted).
/// 免费版进一步 clamp 到 1..=2。
#[tauri::command]
pub async fn set_max_concurrent(
    state: State<'_, crate::AppState>,
    value: usize,
) -> AppResult<usize> {
    let queue = state.queue_manager.as_ref()
        .ok_or_else(|| crate::error::AppError::Internal("Queue not initialized".into()))?;
    let cap = concurrency_cap(&state.license);
    Ok(queue.set_max_concurrent(value.min(cap)))
}

/// 免费版 / Pro 的并发数上限
fn concurrency_cap(license: &crate::license::LicenseManager) -> usize {
    if license.is_pro() {
        16
    } else {
        crate::license::config::FREE_MAX_CONCURRENT
    }
}

/// 编码器埋点 key（小写，对齐上报示例负载的 codecs 结构）
fn codec_key(config: &EncodeConfig) -> String {
    match config.video_codec {
        crate::encoder::codec::VideoCodec::H264 => "h264",
        crate::encoder::codec::VideoCodec::H265 => "h265",
        crate::encoder::codec::VideoCodec::AV1 => "av1",
        crate::encoder::codec::VideoCodec::VP9 => "vp9",
        crate::encoder::codec::VideoCodec::Copy => "copy",
    }
    .to_string()
}
