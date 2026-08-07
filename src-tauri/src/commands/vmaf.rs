use tauri::{Emitter, State};

use crate::encoder::vmaf::{
    self, VmafResult, DEFAULT_SEGMENTS, DEFAULT_SEGMENT_SECONDS,
};
use crate::error::AppResult;

/// VMAF 段数设置的 settings 表 key 与取值范围
const SETTINGS_KEY_VMAF_SEGMENTS: &str = "vmaf_segments";
const MAX_VMAF_SEGMENTS: usize = 32;

/// 计算已完成编码任务的 VMAF 质量得分。
///
/// `segments == 0`：全量对比（整片一次打分，耗时随视频时长线性增长）；
/// 否则：均匀采样 `segments` 段 × 5 秒，取平均分。
/// 得分与各段明细持久化到 DB（随任务保留），完成后 emit `queue://updated`
/// 让队列页刷新展示。
#[tauri::command]
pub async fn compute_vmaf(
    app_handle: tauri::AppHandle,
    state: State<'_, crate::AppState>,
    job_id: String,
    segments: usize,
) -> AppResult<VmafResult> {
    let queue = state
        .queue_manager
        .as_ref()
        .ok_or_else(|| crate::error::AppError::Internal("Queue not initialized".into()))?;

    let (input_path, output_path) = queue
        .get_job_paths(&job_id)
        .ok_or_else(|| crate::error::AppError::InvalidConfig(format!("任务不存在: {job_id}")))?;

    if !std::path::Path::new(&input_path).exists() || !std::path::Path::new(&output_path).exists() {
        return Err(crate::error::AppError::InvalidConfig(
            "原始文件或输出文件不存在，无法计算 VMAF".into(),
        ));
    }

    // 0 = 全量，1..=32 = 采样段数
    let raw_segments = segments;
    let segments = segments.clamp(0, MAX_VMAF_SEGMENTS);
    if raw_segments != segments {
        log::warn!("compute_vmaf: segments={} 越界，已收敛为 {}", raw_segments, segments);
    }

    log::info!(
        "compute_vmaf: {} segments={} (ref={}, dist={})",
        job_id,
        segments,
        input_path,
        output_path
    );

    let input = input_path.clone();
    let output = output_path.clone();
    // 每次计算使用唯一工作目录：job 重算/并发计算互不干扰
    let work_id = format!("{job_id}_{}", uuid::Uuid::new_v4());
    let result = tokio::task::spawn_blocking(move || {
        vmaf::compute_vmaf_sampled(
            &input,
            &output,
            segments,
            DEFAULT_SEGMENT_SECONDS,
            &work_id,
        )
    })
    .await
    .map_err(|e| crate::error::AppError::Internal(e.to_string()))??;

    log::info!("compute_vmaf: {} average={}", job_id, result.average_score);

    // 持久化平均分 + 各段明细（含模式标记，供前端区分全量/采样展示）
    let detail = serde_json::json!({
        "mode": if segments == 0 { "full" } else { "sampled" },
        "scores": result.segment_scores,
    });
    queue.set_vmaf_score(&job_id, result.average_score, Some(detail.to_string()));

    // 刷新队列展示
    let status = queue.get_status();
    let _ = app_handle.emit("queue://updated", &status);

    Ok(result)
}

/// 读取 VMAF 段数设置（0 = 全量对比，N = N 段 × 5 秒均匀采样）。
#[tauri::command]
pub async fn get_vmaf_segments(
    state: State<'_, crate::AppState>,
) -> AppResult<usize> {
    let queue = state
        .queue_manager
        .as_ref()
        .ok_or_else(|| crate::error::AppError::Internal("Queue not initialized".into()))?;
    Ok(queue.get_setting_usize(SETTINGS_KEY_VMAF_SEGMENTS, DEFAULT_SEGMENTS))
}

/// 保存 VMAF 段数设置（0 = 全量对比，N = N 段 × 5 秒均匀采样），返回保存后的值。
#[tauri::command]
pub async fn set_vmaf_segments(
    state: State<'_, crate::AppState>,
    value: usize,
) -> AppResult<usize> {
    let queue = state
        .queue_manager
        .as_ref()
        .ok_or_else(|| crate::error::AppError::Internal("Queue not initialized".into()))?;
    let value = value.clamp(0, MAX_VMAF_SEGMENTS);
    queue.set_setting_usize(SETTINGS_KEY_VMAF_SEGMENTS, value);
    log::info!("set_vmaf_segments: {}", value);
    Ok(value)
}
