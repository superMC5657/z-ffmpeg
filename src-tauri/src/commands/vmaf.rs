use tauri::State;

use crate::encoder::vmaf::{
    self, VmafResult, DEFAULT_SEGMENTS, DEFAULT_SEGMENT_SECONDS,
};
use crate::error::AppResult;
use crate::queue::settings::SETTINGS_KEY_VMAF_SEGMENTS;

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
    // Pro 门控：VMAF 质量对比
    state.license.ensure_pro("VMAF 质量对比")?;
    crate::analytics::bump(&crate::analytics::COUNTERS.vmaf_runs, 1);

    let queue = state
        .queue_manager
        .as_ref()
        .ok_or_else(|| crate::error::AppError::Internal("Queue not initialized".into()))?;

    let (input_path, output_path) = match queue.get_job_paths(&job_id) {
        Some(paths) => paths,
        None => {
            // job_id 为内部 UUID，可进日志；不记输入输出路径
            let err = crate::error::AppError::InvalidConfig(format!("任务不存在: {job_id}"));
            let msg = err.to_string();
            let top = msg.lines().next().unwrap_or("unknown").to_string();
            log::warn!("vmaf compute failed job {job_id} reason {top}");
            return Err(err);
        }
    };

    if !std::path::Path::new(&input_path).exists() || !std::path::Path::new(&output_path).exists() {
        log::warn!("vmaf compute failed job {job_id} reason missing files");
        return Err(crate::error::AppError::InvalidConfig(
            "原始文件或输出文件不存在，无法计算 VMAF".into(),
        ));
    }

    // 0 = 全量，1..=32 = 采样段数
    let segments = segments.clamp(0, MAX_VMAF_SEGMENTS);

    let input = input_path.clone();
    let output = output_path.clone();
    // 每次计算使用唯一工作目录：job 重算/并发计算互不干扰
    let work_id = format!("{job_id}_{}", uuid::Uuid::new_v4());
    let result = match tokio::task::spawn_blocking(move || {
        vmaf::compute_vmaf_sampled(
            &input,
            &output,
            segments,
            DEFAULT_SEGMENT_SECONDS,
            &work_id,
        )
    })
    .await
    .map_err(|e| crate::error::AppError::Internal(e.to_string()))
    {
        Ok(inner) => match inner {
            Ok(result) => result,
            Err(e) => {
                let msg = e.to_string();
                let top = msg.lines().next().unwrap_or("unknown").to_string();
                log::warn!("vmaf compute failed job {job_id} reason {top}");
                return Err(e);
            }
        },
        Err(e) => {
            let msg = e.to_string();
            let top = msg.lines().next().unwrap_or("unknown").to_string();
            log::warn!("vmaf compute failed job {job_id} reason {top}");
            return Err(e);
        }
    };

    // 持久化平均分 + 各段明细（含模式标记，供前端区分全量/采样展示）
    let detail = serde_json::json!({
        "mode": if segments == 0 { "full" } else { "sampled" },
        "scores": result.segment_scores,
    });
    queue.set_vmaf_score(&job_id, result.average_score, Some(detail.to_string()));

    // 刷新队列展示
    super::queue::emit_queue(&app_handle, queue);

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
    Ok(value)
}
