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

// 参数构建与探测已拆分到独立模块，调用方直接引用 `args::` / `probe::`；
// 本模块只保留进程生命周期管理（start/cancel）与进度循环。
use super::args::build_ffmpeg_args;
use super::probe::probe_file;
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
    fn cancelled(job_id: String, file_name: String, duration_seconds: f64) -> Self {
        Self {
            job_id,
            file_name,
            success: false,
            output_path: None,
            output_size_bytes: None,
            duration_seconds,
            cancelled: true,
            error: Some("用户已取消".into()),
        }
    }
}

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

/// Kill all running encoding child processes (invoked on application exit).
pub fn kill_all_processes() {
    if let Some(map) = PROCESSES.get() {
        if let Ok(mut map) = map.lock() {
            for (_job_id, mut proc) in map.drain() {
                proc.cancel.store(true, Ordering::Relaxed);
                let _ = proc.child.kill();
            }
        }
    }
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
    let file_name = std::path::Path::new(&input_path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let Some(ffmpeg_path) = ffmpeg::get_ffmpeg_path() else {
        log::error!(target: "zffmpeg_lib::encoder", "encode ffmpeg not found job {job_id} file {file_name}");
        return Err(AppError::FfmpegNotFound);
    };

    // Start the clock before probing so both cancellation paths (pre-spawn and
    // post-registration) can report an accurate duration.
    let start_time = std::time::Instant::now();

    // Cancellation may arrive before the process is spawned — e.g. the job was
    // cancelled while waiting for a blocking worker thread (the ffmpeg child is
    // only registered in PROCESSES after spawn, so cancel_process can't find it
    // yet). Honour the flag here so the encode never starts at all.
    if cancel.load(Ordering::Relaxed) {
        let elapsed = start_time.elapsed().as_secs_f64();
        log::warn!(target: "zffmpeg_lib::encoder", "encode cancelled job {job_id} file {file_name} stage pre-spawn elapsed {elapsed:.1}s");
        let _ = app_handle.emit(
            "encode://complete",
            EncodeResult::cancelled(job_id, file_name, elapsed),
        );
        return Ok(());
    }

    let input_size = std::fs::metadata(&input_path).map(|m| m.len()).unwrap_or(0);
    let args = build_ffmpeg_args(&config, &input_path, &output_path);
    let cmd_preview = format!(
        "ffmpeg {}",
        args.iter()
            .map(|a| if a.contains(' ') || a.is_empty() {
                format!("\"{}\"", a)
            } else {
                a.to_string()
            })
            .collect::<Vec<_>>()
            .join(" ")
    );

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
    log::info!(
        target: "zffmpeg_lib::encoder",
        "encode started job {job_id} file {file_name} in_size={input_size}B dur={:.1}s cmd: {cmd_preview}",
        total_duration.unwrap_or(0.0)
    );

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
        let _ = std::fs::remove_file(&output_path);
        let elapsed = start_time.elapsed().as_secs_f64();
        log::warn!(target: "zffmpeg_lib::encoder", "encode cancelled job {job_id} file {file_name} stage post-spawn elapsed {elapsed:.1}s");
        let _ = app_handle.emit(
            "encode://complete",
            EncodeResult::cancelled(job_id, file_name, elapsed),
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
    let mut last_milestone: u32 = 0;
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

            let pct_u32 = percentage.floor() as u32;
            if pct_u32 >= last_milestone + 25 && pct_u32 < 100 {
                last_milestone = (pct_u32 / 25) * 25;
                log::info!(
                    target: "zffmpeg_lib::encoder",
                    "encode progress job {job_id} file {file_name} pct={last_milestone}% fps={:.1} speed={:.2}x",
                    progress.fps,
                    progress.speed
                );
            }

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
        let _ = std::fs::remove_file(&output_path);
        let elapsed_s = elapsed.as_secs_f64();
        log::warn!(target: "zffmpeg_lib::encoder", "encode cancelled job {job_id} file {file_name} stage running elapsed {elapsed_s:.1}s");
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
                EncodeResult {
                    job_id,
                    file_name,
                    success: true,
                    output_path: Some(output_path),
                    output_size_bytes: Some(output_size),
                    duration_seconds: elapsed.as_secs_f64(),
                    cancelled: false,
                    error: None,
                },
            );
            Ok(())
        }
        _ => {
            let exit_code = status.as_ref().and_then(|s| s.code()).unwrap_or(-1);

            // Clean up partial output file on error
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
                EncodeResult {
                    job_id,
                    file_name,
                    success: false,
                    output_path: None,
                    output_size_bytes: None,
                    duration_seconds: elapsed.as_secs_f64(),
                    cancelled: false,
                    error: Some(error.clone()),
                },
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
