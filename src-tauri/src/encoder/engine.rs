//! 编码引擎：ffmpeg 子进程的启动、进度解析循环、取消与结束处理。
//! 参数构建见 `args`，探测见 `probe`，进度结构见 `progress`。

use std::io::{BufRead, BufReader};
use std::process::Stdio;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use tauri::{AppHandle, Emitter};
use crate::encoder::codec::EncodeConfig;
use crate::encoder::file_name_from_path;
use crate::error::{AppError, AppResult};
use crate::ffmpeg::facade;

// 参数构建与探测已拆分到独立模块，调用方直接引用 `args::` / `probe::`；
// 进程监管（注册表/取消状态机/进度块拼装）归 `supervisor`，本模块只做事件→emit 薄映射。
use super::args::{build_ffmpeg_args_with_bitrate, format_command_line};
use super::probe::probe_file;
use super::supervisor::{self, BlockPoller, CancelPoint, Supervision, TerminalOutcome};

/// 编码失败时附加到错误信息中的 stderr 尾部行数（ffmpeg 把真正的报错原因
/// 写在 stderr 末尾，完整输出太长，尾部几十行足够定位问题）。
const STDERR_TAIL_LINES: usize = 50;

/// `encode://complete` 事件的载荷，字段对齐前端 `src/types/index.ts` 的 EncodeResult。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EncodeResult {
    pub job_id: String,
    pub file_name: String,
    pub success: bool,
    pub output_path: Option<String>,
    pub output_size_bytes: Option<u64>,
    pub duration_seconds: f64,
    pub cancelled: bool,
    pub error: Option<String>,
}

impl EncodeResult {
    /// 取消路径的统一载荷：三处取消分支（spawn 前 / spawn 中 / 运行中）共用。
    /// 字段语义唯一拥有者为 `supervisor::TerminalOutcome`，此处薄映射保持 wire 形状不变。
    fn cancelled(job_id: String, file_name: String, duration_seconds: f64) -> Self {
        TerminalOutcome::cancelled(job_id, file_name, duration_seconds).into()
    }
}

impl From<TerminalOutcome> for EncodeResult {
    fn from(t: TerminalOutcome) -> Self {
        Self {
            job_id: t.job_id,
            file_name: t.file_name,
            success: t.success,
            output_path: t.output_path,
            output_size_bytes: t.output_size_bytes,
            duration_seconds: t.duration_seconds,
            cancelled: t.cancelled,
            error: t.error,
        }
    }
}

/// 请求取消正在运行的编码任务：设置取消标记并立即终止
/// ffmpeg 进程（即使 ffmpeg 没有输出时也能立即生效）。
/// 薄委托：语义归 supervisor 注册表。
pub fn cancel_process(job_id: &str) -> bool {
    supervisor::request_cancel(job_id)
}

/// 终止所有正在运行的编码子进程（在应用退出时调用）。
/// 薄委托：语义归 supervisor 注册表。
pub fn kill_all_processes() {
    supervisor::kill_all()
}

/// 启动编码并通过 Tauri 事件上报进度。
/// 可将 `cancel` 标记设为 true 以请求取消编码。
pub fn start_encode(
    app_handle: AppHandle,
    job_id: String,
    config: EncodeConfig,
    input_path: String,
    output_path: String,
    cancel: Arc<AtomicBool>,
) -> AppResult<()> {
    let file_name = file_name_from_path(&input_path);

    let Some(ffmpeg_path) = facade::get_ffmpeg_path() else {
        log::error!(target: "zffmpeg_lib::encoder", "encode ffmpeg not found job {job_id} file {file_name}");
        return Err(AppError::FfmpegNotFound);
    };

    // 在探测前启动计时器，使两条取消路径（创建前和
    // 注册后）都能准确报告耗时。监管状态机保证三处分支恰好认领一次终态。
    let start_time = std::time::Instant::now();
    let mut supervision = Supervision::new(cancel.clone());

    // 取消请求可能在进程启动前到达——例如任务在等待阻塞工作线程时被取消
    // （ffmpeg 子进程仅在 spawn 后才注册，`request_cancel`
    // 尚无法 kill 它）。在此响应标记，使编码根本无需启动。
    if supervision.claim_cancellation(CancelPoint::PreSpawn).is_some() {
        let elapsed = supervision.elapsed_secs();
        log::warn!(target: "zffmpeg_lib::encoder", "encode cancelled job {job_id} file {file_name} stage {} elapsed {elapsed:.1}s", CancelPoint::PreSpawn.as_str());
        let _ = app_handle.emit(
            "encode://complete",
            EncodeResult::cancelled(job_id, file_name, elapsed),
        );
        return Ok(());
    }

    let input_size = std::fs::metadata(&input_path).map(|m| m.len()).unwrap_or(0);

    // 首先进行探测以获取总时长和码率，用于进度计算和自动最大码率保护
    let probe_json = probe_file(&input_path).ok();
    let total_duration = probe_json.as_ref().and_then(|json| {
        json.get("format")
            .and_then(|f| f.get("duration"))
            .and_then(|d| d.as_str())
            .and_then(|s| s.parse::<f64>().ok())
    });

    let input_bitrate_kbps = probe_json.as_ref().and_then(|json| {
        let format = json.get("format")?;
        format.get("bit_rate")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<f64>().ok())
            .map(|bps| (bps / 1000.0).round() as u32)
            .or_else(|| {
                let dur = total_duration?;
                if dur > 0.0 && input_size > 0 {
                    Some(((input_size as f64 * 8.0) / (dur * 1000.0)).round() as u32)
                } else {
                    None
                }
            })
    });

    let args = build_ffmpeg_args_with_bitrate(&config, &input_path, &output_path, input_bitrate_kbps);
    let cmd_preview = format_command_line(&args);

    // 确保在 ffmpeg 写入前输出目标目录已存在
    if let Some(parent) = std::path::Path::new(&output_path).parent() {
        if !parent.as_os_str().is_empty() {
            let _ = std::fs::create_dir_all(parent);
        }
    }

    // 启动 ffmpeg（在 Windows 上隐藏控制台窗口）。
    // `-progress pipe:1` 将机器可读的 key=value 报告写入 stdout——
    // 当 ffmpeg 使用管道 stdio 启动时，这是可靠的进度来源
    // （其在 stderr 上的易读统计信息仅在终端下输出）。
    let mut child = facade::hidden_command(ffmpeg_path)
        .args(args)
        .arg("-progress")
        .arg("pipe:1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AppError::Ffmpeg(format!("Failed to spawn ffmpeg: {}", e)))?;

    // 先取出管道，以便将子进程移入注册表。
    let stdout = child.stdout.take().expect("failed to get stdout");
    let stderr = child.stderr.take().expect("failed to get stderr");

    // 注册进程以便可以通过 job_id 强制终止（注册表归 supervisor）。
    supervisor::attach_child(&job_id, supervision.cancel_flag(), child);
    log::info!(
        target: "zffmpeg_lib::encoder",
        "encode started job {job_id} file {file_name} in_size={input_size}B dur={:.1}s cmd: {cmd_preview}",
        total_duration.unwrap_or(0.0)
    );

    // 子进程注册后重新检查取消状态：用户可能在 probe/spawn 期间取消了任务，
    // 此时子进程尚未注册，因此 `cancel_process` 无法终止任何进程。
    // 若此时已取消则立即终止——否则编码会在后台持续运行直到完成，
    // 而 UI 上任务状态却显示为“已取消”。终态认领归状态机（恰好一次）。
    if supervision.claim_cancellation(CancelPoint::PostSpawn).is_some() {
        // kill + wait 收进 supervisor（锁内只取走 handle，阻塞的 kill/wait
        // 在锁外——绝不能阻塞其他 cancel 请求，与搬运前一致）。
        supervisor::kill_and_reap(&job_id);
        let _ = std::fs::remove_file(&output_path);
        let elapsed = supervision.elapsed_secs();
        log::warn!(target: "zffmpeg_lib::encoder", "encode cancelled job {job_id} file {file_name} stage {} elapsed {elapsed:.1}s", CancelPoint::PostSpawn.as_str());
        let _ = app_handle.emit(
            "encode://complete",
            EncodeResult::cancelled(job_id, file_name, elapsed),
        );
        return Ok(());
    }

    // 持续读取 stderr 以免 ffmpeg 因管道写满而阻塞；保留最后
    // STDERR_TAIL_LINES 行用于失败诊断（ffmpeg 会将真正的错误原因报告在 stderr 末尾）。
    let stderr_thread = std::thread::spawn(move || {
        let mut tail: std::collections::VecDeque<String> = std::collections::VecDeque::new();
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if tail.len() >= STDERR_TAIL_LINES {
                tail.pop_front();
            }
            tail.push_back(line);
        }
        tail
    });

    // 解析来自 stdout 的 `-progress` 报告。每段报告是一组
    // 以 `progress=continue|end` 结尾的 `key=value` 行。
    // 块轮询与事件拼装归 supervisor（纯逻辑可单测）；此处只做 emit 薄映射。
    let mut poller = BlockPoller::new(BufReader::new(stdout));
    let mut last_milestone: u32 = 0;
    loop {
        match poller.poll_block(&supervision) {
            supervisor::PollOutcome::Cancelled | supervisor::PollOutcome::Eof => break,
            supervisor::PollOutcome::Block { is_end } => {
                // 一段报告块结束：发送进度事件并重置
                let elapsed = start_time.elapsed();
                let progress = supervisor::assemble_progress_event(
                    &job_id,
                    &file_name,
                    poller.kv(),
                    total_duration,
                    elapsed.as_secs(),
                );
                let _ = app_handle.emit("encode://progress", &progress);

                if let Some(milestone) = supervisor::next_milestone(last_milestone, progress.percentage) {
                    last_milestone = milestone;
                    log::info!(
                        target: "zffmpeg_lib::encoder",
                        "encode progress job {job_id} file {file_name} pct={last_milestone}% fps={:.1} speed={:.2}x",
                        progress.fps,
                        progress.speed
                    );
                }

                if is_end {
                    break;
                }
                poller.clear_block();
            }
        }
    }

    // 回收子进程（从注册表中移除，防止 cancel_process 重复终止）
    let status = supervisor::reap_child(&job_id);
    let stderr_tail: Vec<String> = stderr_thread
        .join()
        .unwrap_or_default()
        .into_iter()
        .collect();

    let elapsed = start_time.elapsed();

    if supervision.claim_cancellation(CancelPoint::Running).is_some() {
        let _ = std::fs::remove_file(&output_path);
        let elapsed_s = elapsed.as_secs_f64();
        log::warn!(target: "zffmpeg_lib::encoder", "encode cancelled job {job_id} file {file_name} stage {} elapsed {elapsed_s:.1}s", CancelPoint::Running.as_str());
        let _ = app_handle.emit(
            "encode://complete",
            EncodeResult::cancelled(job_id, file_name, elapsed_s),
        );
        return Ok(());
    }

    match status {
        Some(status) if status.success() => {
            let output_size = std::fs::metadata(&output_path)
                .map(|m| m.len())
                .unwrap_or(0);
            let elapsed_s = elapsed.as_secs_f64();
            let ratio_str = if input_size > 0 {
                let pct = (output_size as f64 / input_size as f64) * 100.0;
                let diff_pct = ((output_size as f64 - input_size as f64) / input_size as f64) * 100.0;
                format!("{:.1}% (delta {:+.1}%)", pct, diff_pct)
            } else {
                "N/A".to_string()
            };
            let speed_str = total_duration
                .filter(|d| *d > 0.0 && elapsed_s > 0.0)
                .map(|d| format!(" avg_speed={:.2}x", d / elapsed_s))
                .unwrap_or_default();
            log::info!(
                target: "zffmpeg_lib::encoder",
                "encode completed job {job_id} file {file_name} in={input_size}B out={output_size}B ratio={ratio_str} elapsed={elapsed_s:.1}s{speed_str}"
            );
            let _ = app_handle.emit(
                "encode://complete",
                EncodeResult::from(TerminalOutcome::completed(
                    job_id,
                    file_name,
                    output_path,
                    output_size,
                    elapsed.as_secs_f64(),
                )),
            );
            Ok(())
        }
        _ => {
            let exit_code = status.as_ref().and_then(|s| s.code()).unwrap_or(-1);

            // 发生错误时清理残留的未完成输出文件
            let _ = std::fs::remove_file(&output_path);

            // 附加 stderr 尾部，让用户能在 UI 直接看到 ffmpeg 的报错原因
            let mut error = format!("FFmpeg 以退出码 {} 退出", exit_code);
            if !stderr_tail.is_empty() {
                error.push_str(&format!(
                    "\n\nFFmpeg 输出（最后 {} 行）：\n{}",
                    stderr_tail.len(),
                    stderr_tail.join("\n")
                ));
            }

            let elapsed_s = elapsed.as_secs_f64();
            // 日志只记 stderr 尾部（stderr_tail 已截断为 STDERR_TAIL_LINES 行），不 dump 全文
            let tail = if stderr_tail.is_empty() {
                String::new()
            } else {
                format!(" tail {}", stderr_tail.join(" | "))
            };
            log::error!(target: "zffmpeg_lib::encoder", "encode failed job {job_id} file {file_name} exit {exit_code} elapsed {elapsed_s:.1}s{tail}");

            let _ = app_handle.emit(
                "encode://complete",
                EncodeResult::from(TerminalOutcome::failed(
                    job_id,
                    file_name,
                    elapsed.as_secs_f64(),
                    error.clone(),
                )),
            );

            Err(AppError::EncodingFailed(error))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::EncodeResult;

    #[test]
    fn encode_result_serializes_camel_case_keys() {
        let value = serde_json::to_value(EncodeResult {
            job_id: "j1".into(),
            file_name: "a.mp4".into(),
            success: true,
            output_path: Some("out.mp4".into()),
            output_size_bytes: Some(1024),
            duration_seconds: 1.5,
            cancelled: false,
            error: None,
        })
        .unwrap();

        assert_eq!(value["jobId"], "j1");
        assert_eq!(value["fileName"], "a.mp4");
        assert_eq!(value["outputPath"], "out.mp4");
        assert_eq!(value["outputSizeBytes"], 1024);
        assert_eq!(value["durationSeconds"], 1.5);
        assert_eq!(value["cancelled"], false);
        assert!(value["error"].is_null());
    }

    #[test]
    fn cancelled_constructor_marks_cancelled() {
        let r = EncodeResult::cancelled("j1".into(), "a.mp4".into(), 2.0);

        assert!(r.cancelled);
        assert!(!r.success);
        assert_eq!(r.output_path, None);
        assert_eq!(r.output_size_bytes, None);
        assert_eq!(r.error.as_deref(), Some("用户已取消"));
    }
}
