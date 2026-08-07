use std::io::{BufRead, BufReader};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use tauri::{AppHandle, Emitter};
use crate::commands::encode::FileInfo;
use crate::encoder::codec::{EncodeConfig, VideoCodec};
use crate::encoder::progress::EncodeProgress;
use crate::error::{AppError, AppResult};
use crate::ffmpeg;

/// A running ffmpeg process, registered so it can be forcibly killed.
struct ActiveProcess {
    cancel: Arc<AtomicBool>,
    child: std::process::Child,
}

/// Global registry: job_id -> running ffmpeg process
static PROCESSES: OnceLock<Mutex<HashMap<String, ActiveProcess>>> = OnceLock::new();

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

/// Parse `out_time=HH:MM:SS.micro` into seconds
fn out_time_to_seconds(s: &str) -> Option<f64> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() == 3 {
        let h: f64 = parts[0].parse().ok()?;
        let m: f64 = parts[1].parse().ok()?;
        let sec: f64 = parts[2].parse().ok()?;
        Some(h * 3600.0 + m * 60.0 + sec)
    } else {
        None
    }
}

/// Extract the numeric part from `1600.0kbits/s`
fn parse_bitrate_kbps(s: &str) -> f64 {
    s.chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect::<String>()
        .parse()
        .unwrap_or(0.0)
}

/// Compute percentage from a completed `-progress` key/value block
fn compute_percentage(kv: &HashMap<String, String>, total_duration: Option<f64>) -> f64 {
    match (
        kv.get("out_time").and_then(|s| out_time_to_seconds(s)),
        total_duration,
    ) {
        (Some(t), Some(d)) if d > 0.0 => (t / d * 100.0).min(99.9),
        _ => 0.0,
    }
}

/// Build the ffmpeg command arguments from config
pub fn build_ffmpeg_args(
    config: &EncodeConfig,
    input_path: &str,
    output_path: &str,
) -> Vec<String> {
    let mut args: Vec<String> = vec![];

    // Input
    args.push("-y".into()); // Overwrite output
    args.push("-i".into());
    args.push(input_path.into());

    // Video encoder
    let encoder = config.video_codec.encoder_name(config.hw_accel.as_ref());
    if config.video_codec != VideoCodec::Copy {
        args.push("-c:v".into());
        args.push(encoder.into());

        // Encoder preset (value depends on the encoder — see encoder_preset_args)
        args.extend(encoder_preset_args(config));

        // Rate control
        args.extend(config.video_settings.rate_control.to_args());

        // Profile
        if let Some(ref profile) = config.video_settings.profile {
            args.push("-profile:v".into());
            args.push(profile.clone());
        }

        // Pixel format
        if let Some(ref pix_fmt) = config.video_settings.pixel_format {
            args.push("-pix_fmt".into());
            args.push(pix_fmt.clone());
        }

        // Resolution scaling
        if let Some(ref res) = config.video_settings.resolution {
            args.push("-vf".into());
            args.push(format!("scale={}:{}", res.width, res.height));
        }

        // Frame rate
        if let Some(fps) = config.video_settings.frame_rate {
            args.push("-r".into());
            args.push(fps.to_string());
        }
    } else {
        args.push("-c:v".into());
        args.push("copy".into());
    }

    // Audio settings
    args.extend(config.audio_settings.to_args());

    // Additional params
    args.extend(config.video_settings.additional_params.clone());

    // Output
    args.push(output_path.into());

    args
}

/// Map the x264-style named preset to the value accepted by the actual encoder.
///
/// Software encoders:
/// - libx264 / libx265: accept the names directly (`-preset medium`).
/// - libsvtav1 (AV1): only accepts `-preset <0-13>` (0 = slowest/best, 13 = fastest).
/// - libvpx-vp9 (VP9): has NO `-preset` option — it uses `-cpu-used <0-8>`
///   (0 = slowest/best, 8 = fastest).
///
/// Hardware encoders each have their own preset vocabulary:
/// - NVENC: `-preset p1`(fastest)..`p7`(best quality); legacy names still work.
/// - QSV:   `-preset veryfast..veryslow` accepted as-is.
/// - AMF:   `-quality speed|balanced|quality` (`-preset` is a synonym).
/// - VAAPI: no `-preset` — uses `-compression_level` (1 = best quality .. 7 = fastest).
/// - VideoToolbox: modern FFmpeg (5.0+) removed `-preset` entirely, so the
///   option is omitted.
fn encoder_preset_args(config: &EncodeConfig) -> Vec<String> {
    let name = &config.video_settings.encoder_preset;

    match config.hw_accel.as_ref() {
        Some(crate::encoder::codec::HwAccelConfig {
            device: crate::encoder::codec::HwAccelDevice::NVENC,
            ..
        }) => {
            // NVENC: p1 = fastest, p7 = best quality.
            let p = match name.as_str() {
                "ultrafast" | "superfast" => "p1",
                "veryfast" | "faster" => "p2",
                "fast" => "p3",
                "medium" => "p4",
                "slow" => "p5",
                "slower" => "p6",
                "veryslow" => "p7",
                other => other, // p1..p7 / legacy values pass through
            };
            vec!["-preset".into(), p.into()]
        }
        Some(crate::encoder::codec::HwAccelConfig {
            device: crate::encoder::codec::HwAccelDevice::QSV,
            ..
        }) => {
            // QSV accepts the veryfast..veryslow names directly.
            vec!["-preset".into(), name.clone()]
        }
        Some(crate::encoder::codec::HwAccelConfig {
            device: crate::encoder::codec::HwAccelDevice::AMF,
            ..
        }) => {
            // AMF uses -quality (or the synonym -preset): speed / balanced / quality.
            let q = match name.as_str() {
                "ultrafast" | "superfast" | "veryfast" | "faster" | "fast" => "speed",
                "medium" => "balanced",
                "slow" | "slower" | "veryslow" => "quality",
                other => other, // speed / balanced / quality / high_quality pass through
            };
            vec!["-quality".into(), q.into()]
        }
        Some(crate::encoder::codec::HwAccelConfig {
            device: crate::encoder::codec::HwAccelDevice::VAAPI,
            ..
        }) => {
            // VAAPI: -compression_level, 1 = slowest/best quality, 7 = fastest.
            let lvl = match name.as_str() {
                "ultrafast" => "7",
                "superfast" | "veryfast" => "6",
                "faster" | "fast" => "5",
                "medium" => "4",
                "slow" => "3",
                "slower" => "2",
                "veryslow" => "1",
                other => other, // numeric levels pass through
            };
            vec!["-compression_level".into(), lvl.into()]
        }
        Some(crate::encoder::codec::HwAccelConfig {
            device: crate::encoder::codec::HwAccelDevice::VideoToolbox,
            ..
        }) => {
            // VideoToolbox dropped -preset in FFmpeg 5.0; omit it entirely.
            vec![]
        }
        None => {
            let av1_map = |n: &str| -> i32 {
                match n {
                    "ultrafast" => 13,
                    "superfast" => 11,
                    "veryfast" => 9,
                    "faster" => 8,
                    "fast" => 7,
                    "medium" => 6,
                    "slow" => 4,
                    "slower" => 3,
                    "veryslow" => 1,
                    _ => 8, // SVT-AV1 default
                }
            };
            let vp9_map = |n: &str| -> i32 {
                match n {
                    "ultrafast" => 8,
                    "superfast" => 7,
                    "veryfast" => 6,
                    "faster" => 5,
                    "fast" => 4,
                    "medium" => 3,
                    "slow" => 2,
                    "slower" => 1,
                    "veryslow" => 0,
                    _ => 1, // libvpx-vp9 default
                }
            };

            match config.video_codec {
                VideoCodec::AV1 => vec!["-preset".into(), av1_map(name).to_string()],
                VideoCodec::VP9 => vec!["-cpu-used".into(), vp9_map(name).to_string()],
                _ => vec!["-preset".into(), name.clone()],
            }
        }
    }
}

/// Build output path from input path + config (shared by queue and command preview)
pub fn derive_output_path(input: &str, config: &EncodeConfig, output_dir: Option<&str>) -> String {
    let path = std::path::Path::new(input);
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();

    let parent = match output_dir {
        Some(dir) if !dir.trim().is_empty() => std::path::Path::new(dir).to_path_buf(),
        _ => path.parent().unwrap_or(std::path::Path::new(".")).to_path_buf(),
    };

    let ext = match config.container_format {
        crate::encoder::codec::ContainerFormat::MP4 => "mp4",
        crate::encoder::codec::ContainerFormat::MKV => "mkv",
        crate::encoder::codec::ContainerFormat::WebM => "webm",
        crate::encoder::codec::ContainerFormat::MOV => "mov",
    };

    parent.join(format!("{}_encoded.{}", stem, ext))
        .to_string_lossy()
        .to_string()
}

/// Derive unique output paths for a batch of inputs.
///
/// `derive_output_path` always maps an input to `{stem}_encoded.{ext}`, so two
/// inputs sharing a basename (from different folders, or the same file added
/// twice) would collide and — with the `-y` flag — silently overwrite the first
/// result. Later duplicates get a numeric suffix (`_2`, `_3`, ...) inserted
/// before the extension.
pub fn derive_output_paths_unique(
    inputs: &[String],
    config: &EncodeConfig,
    output_dir: Option<&str>,
) -> Vec<String> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    inputs
        .iter()
        .map(|f| {
            let base = derive_output_path(f, config, output_dir);
            let count = seen.entry(base.clone()).or_insert(0);
            *count += 1;
            if *count == 1 {
                base
            } else {
                let p = std::path::Path::new(&base);
                let stem = p.file_stem().unwrap_or_default().to_string_lossy();
                let ext = p.extension().unwrap_or_default().to_string_lossy();
                let parent = p.parent().unwrap_or(std::path::Path::new("."));
                let name = if ext.is_empty() {
                    format!("{}_{}", stem, *count)
                } else {
                    format!("{}_{}.{}", stem, *count, ext)
                };
                parent.join(name).to_string_lossy().to_string()
            }
        })
        .collect()
}

/// Build a display-ready ffmpeg command line (`ffmpeg <args...>`) from a config.
/// Paths containing spaces or quotes are quoted so the command can be copied
/// and pasted into a terminal directly.
pub fn build_ffmpeg_command_line(
    config: &EncodeConfig,
    input_path: &str,
    output_path: &str,
) -> String {
    let mut parts = vec!["ffmpeg".to_string()];
    for arg in build_ffmpeg_args(config, input_path, output_path) {
        if arg.contains(' ') || arg.contains('"') {
            parts.push(format!("\"{}\"", arg.replace('"', "\\\"")));
        } else {
            parts.push(arg);
        }
    }
    parts.join(" ")
}

/// ffprobe args shared by both the sync and async probing paths.
fn ffprobe_args(input_path: &str) -> Vec<String> {
    vec![
        "-v".into(),
        "quiet".into(),
        "-print_format".into(),
        "json".into(),
        "-show_format".into(),
        "-show_streams".into(),
        input_path.into(),
    ]
}

/// Parse ffprobe's JSON stdout into a serde value.
fn parse_ffprobe_stdout(stdout: &[u8]) -> AppResult<serde_json::Value> {
    let stdout = String::from_utf8_lossy(stdout);
    serde_json::from_str(&stdout)
        .map_err(|e| AppError::Ffmpeg(format!("Failed to parse ffprobe output: {}", e)))
}

/// Run ffprobe to get file information (blocking — call from spawn_blocking contexts).
pub fn probe_file(input_path: &str) -> AppResult<serde_json::Value> {
    let ffprobe = ffmpeg::get_ffprobe_path()
        .or_else(|| ffmpeg::get_ffmpeg_path())
        .ok_or(AppError::FfmpegNotFound)?;

    let output = ffmpeg::hidden_command(ffprobe)
        .args(ffprobe_args(input_path))
        .output()
        .map_err(|e| AppError::Ffmpeg(format!("ffprobe failed: {}", e)))?;

    parse_ffprobe_stdout(&output.stdout)
}

/// Run ffprobe asynchronously — never blocks the async runtime thread.
pub async fn probe_file_async(input_path: &str) -> AppResult<serde_json::Value> {
    let ffprobe = ffmpeg::get_ffprobe_path()
        .or_else(|| ffmpeg::get_ffmpeg_path())
        .ok_or(AppError::FfmpegNotFound)?;

    let mut cmd = tokio::process::Command::new(ffprobe);
    // Windows: keep the console window hidden, same as the sync `hidden_command`.
    #[cfg(windows)]
    {
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let output = cmd
        .args(ffprobe_args(input_path))
        .output()
        .await
        .map_err(|e| AppError::Ffmpeg(format!("ffprobe failed: {}", e)))?;

    parse_ffprobe_stdout(&output.stdout)
}

/// Parse ffprobe JSON into structured file info
pub fn parse_probe_result(json: &serde_json::Value, path: &str) -> AppResult<FileInfo> {
    let format = json.get("format").ok_or_else(|| AppError::Ffmpeg("No format info".into()))?;
    let streams = json.get("streams").ok_or_else(|| AppError::Ffmpeg("No streams".into()))?;

    let file_name = std::path::Path::new(path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let file_size = format
        .get("size")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    let duration = format
        .get("duration")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok());

    // Find video stream
    let video_stream = streams
        .as_array()
        .and_then(|arr| arr.iter().find(|s| s.get("codec_type").and_then(|t| t.as_str()) == Some("video")));

    let audio_stream = streams
        .as_array()
        .and_then(|arr| arr.iter().find(|s| s.get("codec_type").and_then(|t| t.as_str()) == Some("audio")));

    Ok(FileInfo {
        path: path.to_string(),
        file_name,
        file_size,
        duration,
        video_codec: video_stream
            .and_then(|s| s.get("codec_name"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        audio_codec: audio_stream
            .and_then(|s| s.get("codec_name"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        width: video_stream
            .and_then(|s| s.get("width"))
            .and_then(|v| v.as_u64())
            .map(|w| w as u32),
        height: video_stream
            .and_then(|s| s.get("height"))
            .and_then(|v| v.as_u64())
            .map(|h| h as u32),
        frame_rate: video_stream
            .and_then(|s| s.get("r_frame_rate"))
            .and_then(|v| v.as_str())
            .and_then(|s| {
                let parts: Vec<&str> = s.split('/').collect();
                if parts.len() == 2 {
                    let num = parts[0].parse::<f64>().ok()?;
                    let den = parts[1].parse::<f64>().ok()?;
                    Some(num / den)
                } else {
                    s.parse::<f64>().ok()
                }
            }),
        bitrate: format
            .get("bit_rate")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<u64>().ok()),
        pixel_format: video_stream
            .and_then(|s| s.get("pix_fmt"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    })
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
                "error": "Cancelled by user"
            }),
        );
        return Ok(());
    }

    // Drain stderr so ffmpeg never blocks on a full pipe; keep the lines
    // available for future diagnostics.
    let stderr_thread = std::thread::spawn(move || {
        use std::io::BufRead;
        for line in BufReader::new(stderr).lines() {
            let _ = line;
        }
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
    let _ = stderr_thread.join();

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
                    "error": null,
                }),
            );
        }
        _ => {
            let exit_code = status.as_ref().and_then(|s| s.code()).unwrap_or(-1);
            log::error!("Encoding failed: {} (exit code: {})", job_id, exit_code);

            let _ = app_handle.emit(
                "encode://complete",
                serde_json::json!({
                    "jobId": job_id,
                    "fileName": file_name,
                    "success": false,
                    "outputPath": null,
                    "outputSizeBytes": null,
                    "durationSeconds": elapsed.as_secs_f64(),
                    "error": format!("FFmpeg exited with code {}", exit_code),
                }),
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoder::codec::{
        EncodeConfig, HwAccelConfig, HwAccelDevice, RateControl, Resolution,
    };

    fn sample_config() -> EncodeConfig {
        serde_json::from_str(
            r#"{"videoCodec":"H264","videoSettings":{"rateControl":{"type":"CRF","value":23},"encoderPreset":"medium","resolution":null,"frameRate":null,"pixelFormat":null,"profile":null,"additionalParams":[]},"audioSettings":{"codec":"AAC","bitrateKbps":192,"channels":2,"sampleRate":48000},"containerFormat":"MP4","hwAccel":null}"#,
        ).unwrap()
    }

    #[test]
    fn derive_output_path_uses_input_dir_and_container_ext() {
        let config = sample_config();
        assert_eq!(
            derive_output_path(r"C:\in\movie.mkv", &config, None),
            r"C:\in\movie_encoded.mp4"
        );
        assert_eq!(
            derive_output_path(r"C:\in\movie.mkv", &config, Some(r"D:\out")),
            r"D:\out\movie_encoded.mp4"
        );
    }

    #[test]
    fn derive_output_paths_unique_avoids_collisions() {
        let config = sample_config();
        // Without an output dir, inputs in different folders get distinct paths
        let outputs = derive_output_paths_unique(
            &[
                r"C:\a\movie.mp4".into(),
                r"C:\b\movie.mp4".into(),
            ],
            &config,
            None,
        );
        assert_eq!(outputs[0], r"C:\a\movie_encoded.mp4");
        assert_eq!(outputs[1], r"C:\b\movie_encoded.mp4");

        // Custom output dir: same basename from different folders collides and
        // later entries get a numeric suffix
        let outputs = derive_output_paths_unique(
            &[
                r"C:\a\movie.mp4".into(),
                r"C:\b\movie.mp4".into(),
                r"C:\c\movie.mp4".into(),
            ],
            &config,
            Some(r"D:\out"),
        );
        assert_eq!(outputs[0], r"D:\out\movie_encoded.mp4");
        assert_eq!(outputs[1], r"D:\out\movie_encoded_2.mp4");
        assert_eq!(outputs[2], r"D:\out\movie_encoded_3.mp4");

        // The same file added twice collides even without an output dir
        let outputs = derive_output_paths_unique(
            &[r"C:\a\movie.mp4".into(), r"C:\a\movie.mp4".into()],
            &config,
            None,
        );
        assert_eq!(outputs[0], r"C:\a\movie_encoded.mp4");
        assert_eq!(outputs[1], r"C:\a\movie_encoded_2.mp4");
    }

    // ---- build_ffmpeg_args ----

    fn config_with_hw(hw: Option<HwAccelConfig>) -> EncodeConfig {
        let mut c = sample_config();
        c.hw_accel = hw;
        c
    }

    fn assert_args_contain(args: &[String], expected: &[&str]) {
        for pair in expected.chunks(2) {
            let (k, v) = (pair[0], pair[1]);
            let pos = args
                .iter()
                .position(|a| a == k)
                .unwrap_or_else(|| panic!("arg {k} not found in {args:?}"));
            assert_eq!(args[pos + 1], v, "value for {k} in {args:?}");
        }
    }

    #[test]
    fn build_args_software_h264_crf() {
        let args = build_ffmpeg_args(&sample_config(), r"C:\in\movie.mkv", r"C:\out\movie.mp4");
        assert_eq!(
            args,
            vec![
                "-y", "-i", r"C:\in\movie.mkv", "-c:v", "libx264", "-preset", "medium",
                "-crf", "23", "-c:a", "aac", "-b:a", "192k", "-ac", "2", "-ar", "48000",
                r"C:\out\movie.mp4",
            ]
        );
    }

    #[test]
    fn build_args_nvenc_maps_preset_to_p_scale() {
        let config = config_with_hw(Some(HwAccelConfig {
            device: HwAccelDevice::NVENC,
            device_index: None,
        }));
        let args = build_ffmpeg_args(&config, "in.mp4", "out.mp4");
        assert_args_contain(&args, &["-c:v", "h264_nvenc", "-preset", "p4", "-crf", "23"]);
    }

    #[test]
    fn build_args_qsv_keeps_preset_name() {
        let config = config_with_hw(Some(HwAccelConfig {
            device: HwAccelDevice::QSV,
            device_index: None,
        }));
        let args = build_ffmpeg_args(&config, "in.mp4", "out.mp4");
        assert_args_contain(&args, &["-c:v", "h264_qsv", "-preset", "medium"]);
    }

    #[test]
    fn build_args_amf_maps_preset_to_quality() {
        let config = config_with_hw(Some(HwAccelConfig {
            device: HwAccelDevice::AMF,
            device_index: None,
        }));
        let args = build_ffmpeg_args(&config, "in.mp4", "out.mp4");
        assert_args_contain(&args, &["-c:v", "h264_amf", "-quality", "balanced"]);
    }

    #[test]
    fn build_args_av1_software_maps_preset_to_number() {
        let mut config = sample_config();
        config.video_codec = VideoCodec::AV1;
        let args = build_ffmpeg_args(&config, "in.mp4", "out.mp4");
        // medium → 6
        assert_args_contain(&args, &["-c:v", "libsvtav1", "-preset", "6"]);
    }

    #[test]
    fn build_args_vp9_software_uses_cpu_used() {
        let mut config = sample_config();
        config.video_codec = VideoCodec::VP9;
        let args = build_ffmpeg_args(&config, "in.mp4", "out.mp4");
        // medium → 3
        assert_args_contain(&args, &["-c:v", "libvpx-vp9", "-cpu-used", "3"]);
    }

    #[test]
    fn build_args_copy_skips_rate_control() {
        let mut config = sample_config();
        config.video_codec = VideoCodec::Copy;
        let args = build_ffmpeg_args(&config, "in.mp4", "out.mp4");
        assert_args_contain(&args, &["-c:v", "copy"]);
        // Copy 分支不得附加 -crf/-preset
        assert!(!args.iter().any(|a| a == "-crf" || a == "-preset"), "{args:?}");
    }

    #[test]
    fn build_args_resolution_fps_profile_pixfmt_extra() {
        let mut config = sample_config();
        config.video_settings.resolution = Some(Resolution { width: 1920, height: 1080 });
        config.video_settings.frame_rate = Some(30.0);
        config.video_settings.profile = Some("high".into());
        config.video_settings.pixel_format = Some("yuv420p".into());
        config.video_settings.additional_params = vec!["-movflags".into(), "+faststart".into()];

        let args = build_ffmpeg_args(&config, "in.mp4", "out.mp4");
        assert_args_contain(
            &args,
            &[
                "-profile:v", "high",
                "-pix_fmt", "yuv420p",
                "-vf", "scale=1920:1080",
                "-r", "30",
                "-movflags", "+faststart",
            ],
        );
    }

    #[test]
    fn build_args_audio_copy_and_none() {
        let mut copy = sample_config();
        copy.audio_settings.codec = "Copy".into();
        let args = build_ffmpeg_args(&copy, "in.mp4", "out.mp4");
        assert_args_contain(&args, &["-c:a", "copy"]);

        let mut none = sample_config();
        none.audio_settings.codec = "None".into();
        let args = build_ffmpeg_args(&none, "in.mp4", "out.mp4");
        assert!(args.contains(&"-an".to_string()), "{args:?}");
        assert!(!args.contains(&"-c:a".to_string()), "{args:?}");
    }

    #[test]
    fn build_args_abr_rate_control_includes_maxrate() {
        let mut config = sample_config();
        config.video_settings.rate_control = RateControl::Abr {
            bitrate_kbps: 4000,
            max_bitrate_kbps: Some(6000),
        };
        let args = build_ffmpeg_args(&config, "in.mp4", "out.mp4");
        assert_args_contain(
            &args,
            &["-b:v", "4000k", "-maxrate", "6000k", "-bufsize", "12000k"],
        );
    }

    #[test]
    fn ffprobe_args_shape() {
        assert_eq!(
            ffprobe_args(r"C:\in\movie.mkv"),
            vec![
                "-v", "quiet", "-print_format", "json", "-show_format", "-show_streams",
                r"C:\in\movie.mkv",
            ]
        );
    }

}
