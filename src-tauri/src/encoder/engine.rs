//! 编码引擎：ffmpeg 子进程的启动、进度解析循环、取消与结束处理。
//! 参数构建见 `args`，探测见 `probe`，进度结构见 `progress`。

use std::io::{BufRead, BufReader};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use tauri::{AppHandle, Emitter};
use crate::encoder::codec::EncodeConfig;
use crate::encoder::progress::EncodeProgress;
use crate::error::{AppError, AppResult};
use crate::ffmpeg;

// 参数/探测/进度解析已拆分到独立模块；这里统一再导出，
// 既有调用方（commands、estimate 等）的 `engine::xxx` 路径无需修改。
pub use super::args::{
    build_ffmpeg_args, build_ffmpeg_command_line, derive_output_paths_unique,
};
pub use super::probe::{parse_probe_result, probe_file, probe_file_async};
pub(crate) use super::probe::{fallback_audio_bps, find_main_video_stream};
use super::progress::{compute_percentage, parse_bitrate_kbps};

/// A running ffmpeg process, registered so it can be forcibly killed.
struct ActiveProcess {
    cancel: Arc<AtomicBool>,
    child: std::process::Child,
}

/// Global registry: job_id -> running ffmpeg process
static PROCESSES: OnceLock<Mutex<HashMap<String, ActiveProcess>>> = OnceLock::new();

/// 编码失败时附加到错误信息中的 stderr 尾部行数（ffmpeg 把真正的报错原因
/// 写在 stderr 末尾，完整输出太长，尾部几十行足够定位问题）。
const STDERR_TAIL_LINES: usize = 50;

/// Request cancellation of a running encode: set the cancel flag and kill
/// the ffmpeg process immediately (works even when ffmpeg is not emitting output).
pub fn cancel_process(job_id: &str) -> bool {
    if let Some(map) = PROCESSES.get() {
        if let Ok(mut map) = map.lock() {
            if let Some(proc) = map.get_mut(job_id) {
                proc.cancel.store(true, Ordering::Relaxed);
                let _ = proc.child.kill();
                return true;
            }
        }
    }
    false
}

/// Start encoding with progress reporting via Tauri events.
/// The `cancel` flag can be set to true to request cancellation.
pub fn start_encode(
    app_handle: AppHandle,
    job_id: String,
    config: EncodeConfig,
    input_path: String,
    output_path: String,
    cancel: Arc<AtomicBool>,
) -> AppResult<()> {
    let ffmpeg_path = ffmpeg::get_ffmpeg_path()
        .or_else(|| ffmpeg::get_ffprobe_path())
        .ok_or(AppError::FfmpegNotFound)?;

    let file_name = std::path::Path::new(&input_path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    // Start the clock before probing so both cancellation paths (pre-spawn and
    // post-registration) can report an accurate duration.
    let start_time = std::time::Instant::now();

    // Cancellation may arrive before the process is spawned — e.g. the job was
    // cancelled while waiting for a blocking worker thread (the ffmpeg child is
    // only registered in PROCESSES after spawn, so cancel_process can't find it
    // yet). Honour the flag here so the encode never starts at all.
    if cancel.load(Ordering::Relaxed) {
        log::info!("Encoding cancelled before start: {}", job_id);
        let _ = app_handle.emit(
            "encode://complete",
            serde_json::json!({
                "jobId": job_id,
                "fileName": file_name,
                "success": false,
                "outputPath": null,
                "outputSizeBytes": null,
                "durationSeconds": start_time.elapsed().as_secs_f64(),
                "cancelled": true,
                "error": "Cancelled by user"
            }),
        );
        return Ok(());
    }

    let args = build_ffmpeg_args(&config, &input_path, &output_path);
    log::info!("Running ffmpeg: {} {}", ffmpeg_path.display(), args.join(" "));

    // First, probe to get total duration for percentage calculation
    let total_duration = match probe_file(&input_path) {
        Ok(json) => {
            json.get("format")
                .and_then(|f| f.get("duration"))
                .and_then(|d| d.as_str())
                .and_then(|s| s.parse::<f64>().ok())
        }
        Err(_) => None,
    };

    // Spawn ffmpeg (hidden console on Windows).
    // `-progress pipe:1` writes machine-readable key=value reports to stdout —
    // this is the reliable progress source when ffmpeg is spawned with piped
    // stdio (its human-readable stats on stderr are only emitted to a terminal).
    let mut child = ffmpeg::hidden_command(ffmpeg_path)
        .args(args)
        .arg("-progress")
        .arg("pipe:1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AppError::Ffmpeg(format!("Failed to spawn ffmpeg: {}", e)))?;

    // Take the pipes first so the child can be moved into the registry.
    let stdout = child.stdout.take().expect("failed to get stdout");
    let stderr = child.stderr.take().expect("failed to get stderr");

    // Register the process so it can be forcibly killed by job id.
    PROCESSES
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .insert(job_id.clone(), ActiveProcess {
            cancel: cancel.clone(),
            child,
        });

    // Re-check cancellation after the child is registered: the user may have
    // cancelled during the probe/spawn window, when no child was registered
    // yet so `cancel_process` could not kill anything. Kill it now if so —
    // otherwise the encode would run to completion in the background while
    // the UI shows the job as Cancelled.
    if cancel.load(Ordering::Relaxed) {
        // Take the process out of the registry first, then drop the lock before
        // wait() — a kill that blocks must not stall other cancel_process calls.
        let mut proc_to_kill = None;
        if let Some(map) = PROCESSES.get() {
            if let Ok(mut map) = map.lock() {
                proc_to_kill = map.remove(&job_id);
            }
        }
        if let Some(mut proc) = proc_to_kill {
            let _ = proc.child.kill();
            let _ = proc.child.wait();
        }
        log::info!("Encoding cancelled during probe/spawn: {}", job_id);
        let _ = app_handle.emit(
            "encode://complete",
            serde_json::json!({
                "jobId": job_id,
                "fileName": file_name,
                "success": false,
                "outputPath": null,
                "outputSizeBytes": null,
                "durationSeconds": start_time.elapsed().as_secs_f64(),
                "cancelled": true,
                "error": "Cancelled by user"
            }),
        );
        return Ok(());
    }

    // Drain stderr so ffmpeg never blocks on a full pipe; keep the last
    // STDERR_TAIL_LINES lines for failure diagnostics (ffmpeg reports the
    // actual error reason at the end of stderr).
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

    // Parse `-progress` reports from stdout. Each report is a block of
    // `key=value` lines terminated by `progress=continue|end`.
    let mut kv: HashMap<String, String> = HashMap::new();
    for line in BufReader::new(stdout).lines() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }

        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim().to_string();
        let value = value.trim().to_string();

        if key == "progress" {
            // End of one report block: emit progress and reset
            let percentage = compute_percentage(&kv, total_duration);
            let elapsed = start_time.elapsed();
            let total_size_kb = kv
                .get("total_size")
                .and_then(|s| s.parse::<u64>().ok())
                .map(|b| b / 1024)
                .unwrap_or(0);
            let progress = EncodeProgress {
                job_id: job_id.clone(),
                file_name: file_name.clone(),
                frame: kv.get("frame").and_then(|s| s.parse().ok()).unwrap_or(0),
                fps: kv.get("fps").and_then(|s| s.parse().ok()).unwrap_or(0.0),
                bitrate: kv.get("bitrate").map(|s| parse_bitrate_kbps(s)).unwrap_or(0.0),
                total_size_kb,
                // 线性外推预估最终体积；进度为 0（编码刚开始）时没有可靠估算
                estimated_size_kb: (total_size_kb > 0 && percentage > 0.0)
                    .then(|| (total_size_kb as f64 * 100.0 / percentage) as u64),
                elapsed: format!("{:02}:{:02}:{:02}",
                    elapsed.as_secs() / 3600,
                    (elapsed.as_secs() % 3600) / 60,
                    elapsed.as_secs() % 60,
                ),
                percentage,
                speed: kv
                    .get("speed")
                    .and_then(|s| s.trim_end_matches('x').parse().ok())
                    .unwrap_or(0.0),
                stage: "encoding".into(),
                time: kv.get("out_time").cloned().unwrap_or_default(),
            };
            let _ = app_handle.emit("encode://progress", &progress);

            if value == "end" {
                break;
            }
            kv.clear();
        } else {
            kv.insert(key, value);
        }
    }

    // Reap the process (removed from the registry so cancel_process can't kill it twice)
    let status = match PROCESSES.get() {
        Some(map) => map
            .lock()
            .unwrap()
            .remove(&job_id)
            .and_then(|mut proc| proc.child.wait().ok()),
        None => None,
    };
    let stderr_tail: Vec<String> = stderr_thread
        .join()
        .unwrap_or_default()
        .into_iter()
        .collect();

    let elapsed = start_time.elapsed();

    if cancel.load(Ordering::Relaxed) {
        log::info!("Encoding cancelled: {}", job_id);
        let _ = app_handle.emit(
            "encode://complete",
            serde_json::json!({
                "jobId": job_id,
                "fileName": file_name,
                "success": false,
                "outputPath": null,
                "outputSizeBytes": null,
                "durationSeconds": elapsed.as_secs_f64(),
                "cancelled": true,
                "error": "Cancelled by user"
            }),
        );
        return Ok(());
    }

    match status {
        Some(status) if status.success() => {
            let output_size = std::fs::metadata(&output_path)
                .map(|m| m.len())
                .unwrap_or(0);

            log::info!("Encoding completed: {} in {:?}", job_id, elapsed);

            let _ = app_handle.emit(
                "encode://complete",
                serde_json::json!({
                    "jobId": job_id,
                    "fileName": file_name,
                    "success": true,
                    "outputPath": output_path,
                    "outputSizeBytes": output_size,
                    "durationSeconds": elapsed.as_secs_f64(),
                    "cancelled": false,
                    "error": null,
                }),
            );
        }
        _ => {
            let exit_code = status.as_ref().and_then(|s| s.code()).unwrap_or(-1);
            log::error!("Encoding failed: {} (exit code: {})", job_id, exit_code);

            // 附加 stderr 尾部，让用户能在 UI 直接看到 ffmpeg 的报错原因
            let mut error = format!("FFmpeg exited with code {}", exit_code);
            if !stderr_tail.is_empty() {
                error.push_str(&format!(
                    "\n\nFFmpeg 输出（最后 {} 行）：\n{}",
                    stderr_tail.len(),
                    stderr_tail.join("\n")
                ));
            }

            let _ = app_handle.emit(
                "encode://complete",
                serde_json::json!({
                    "jobId": job_id,
                    "fileName": file_name,
                    "success": false,
                    "outputPath": null,
                    "outputSizeBytes": null,
                    "durationSeconds": elapsed.as_secs_f64(),
                    "cancelled": false,
                    "error": error,
                }),
            );
        }
    }

    Ok(())
}
