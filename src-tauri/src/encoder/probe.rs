//! ffprobe 探测：调用 ffprobe 获取媒体信息 JSON、解析为 `FileInfo`，
//! 以及流选择 / 音频码率回退等共享启发式（供预估模块复用）。

use crate::commands::encode::FileInfo;
use crate::error::{AppError, AppResult};
use crate::ffmpeg;

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

/// 从 streams 数组选出主视频流：跳过内嵌封面（`disposition.attached_pic=1`）。
/// 封面流常被 ffprobe 列在首位，直接取第一个视频流会拿到封面尺寸/帧率，
/// 导致输出缩放按封面面积计算（可能 clamp 到 4.0 放大预估）。全部为封面时
/// 回退取第一个视频流。
pub(crate) fn find_main_video_stream(
    streams: &serde_json::Value,
) -> Option<&serde_json::Value> {
    let arr = streams.as_array()?;
    let is_video = |s: &&serde_json::Value| {
        s.get("codec_type").and_then(|t| t.as_str()) == Some("video")
    };
    let is_cover = |s: &&serde_json::Value| {
        s.get("disposition")
            .and_then(|d| d.get("attached_pic"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
            > 0
    };
    arr.iter()
        .find(|s| is_video(s) && !is_cover(s))
        .or_else(|| arr.iter().find(|s| is_video(s)))
}

/// 音频码率启发式回退（bps）：音频流缺 `bit_rate` 时，用容器总码率减去视频流
/// 码率近似，避免把整个容器（含视频）当成音频（否则 Copy 音频预估虚高 3–50 倍）。
/// `container_bps` 由 size/duration 推算；`video_bps` 为视频流 `bit_rate`（缺失则
/// None，此时按音频约占容器 10% 粗估）。
pub(crate) fn fallback_audio_bps(container_bps: f64, video_bps: Option<f64>) -> f64 {
    match video_bps {
        // 下限保护：剩余码率不低于容器总码率的 1%，防止 video≈container 时算出 0
        Some(v) if v > 0.0 => (container_bps - v).max(container_bps * 0.01),
        _ => container_bps * 0.1,
    }
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

    // Find main video stream (skips embedded cover/attached_pic streams)
    let video_stream = find_main_video_stream(streams);

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
        audio_bitrate: audio_stream
            .and_then(|s| s.get("bit_rate"))
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<u64>().ok())
            .or_else(|| {
                // 仅当存在音频流时按「容器总码率 − 视频流码率」近似（视频-only 文件
                // 无音频流，不应误标）；直接用容器总码率会把视频也当成音频，Copy 预估虚高
                if audio_stream.is_none() {
                    return None;
                }
                let dur = duration?;
                if dur <= 0.0 {
                    return None; // 防止 size*8/0 → inf → u64::MAX 污染预估
                }
                let size = format
                    .get("size")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<f64>().ok())?;
                let container_bps = size * 8.0 / dur;
                let video_bps = video_stream
                    .and_then(|s| s.get("bit_rate"))
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<f64>().ok());
                Some(fallback_audio_bps(container_bps, video_bps) as u64)
            }),
        pixel_format: video_stream
            .and_then(|s| s.get("pix_fmt"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn parse_probe_result_extracts_audio_bitrate() {
        let json = serde_json::json!({
            "format": { "size": "104857600", "duration": "100.0", "bit_rate": "8000000" },
            "streams": [
                { "codec_type": "video", "codec_name": "h264", "width": 1920, "height": 1080,
                  "pix_fmt": "yuv420p", "r_frame_rate": "30/1" },
                { "codec_type": "audio", "codec_name": "aac", "bit_rate": "192000" },
            ],
        });
        let info = parse_probe_result(&json, r"C:\in\a.mp4").unwrap();
        assert_eq!(info.audio_bitrate, Some(192_000));
        assert_eq!(info.bitrate, Some(8_000_000));
    }

    #[test]
    fn parse_probe_result_audio_bitrate_falls_back_to_container_rate() {
        // 音频流不写 bit_rate：用「容器总码率 − 视频流码率」近似，不再把整个容器当音频
        // 容器 = 104857600*8/100 ≈ 8388608 bps，视频 = 8000000 bps → 音频 ≈ 388608 bps
        let json = serde_json::json!({
            "format": { "size": "104857600", "duration": "100.0" },
            "streams": [
                { "codec_type": "video", "codec_name": "h264", "bit_rate": "8000000" },
                { "codec_type": "audio", "codec_name": "aac" },
            ],
        });
        let info = parse_probe_result(&json, r"C:\in\a.mp4").unwrap();
        assert_eq!(info.audio_bitrate, Some(388_608));
    }

    #[test]
    fn parse_probe_result_audio_fallback_without_video_bitrate() {
        // 视频流也无 bit_rate：退化为容器总码率的 10%（8388608 × 0.1 ≈ 838861）
        let json = serde_json::json!({
            "format": { "size": "104857600", "duration": "100.0" },
            "streams": [
                { "codec_type": "video", "codec_name": "h264" },
                { "codec_type": "audio", "codec_name": "aac" },
            ],
        });
        let info = parse_probe_result(&json, r"C:\in\a.mp4").unwrap();
        assert_eq!(info.audio_bitrate, Some(838_860));
    }

    #[test]
    fn parse_probe_result_skips_attached_pic_cover() {
        // 带内嵌封面（attached_pic=1，列在首位）的 MKV：主视频宽高/编码不应取封面
        let json = serde_json::json!({
            "format": { "size": "104857600", "duration": "100.0", "bit_rate": "8000000" },
            "streams": [
                { "codec_type": "video", "codec_name": "mjpeg", "width": 400, "height": 300,
                  "disposition": { "attached_pic": 1 } },
                { "codec_type": "video", "codec_name": "h264", "width": 1920, "height": 1080,
                  "pix_fmt": "yuv420p", "r_frame_rate": "30/1", "bit_rate": "7000000" },
                { "codec_type": "audio", "codec_name": "aac", "bit_rate": "192000" },
            ],
        });
        let info = parse_probe_result(&json, r"C:\in\a.mkv").unwrap();
        assert_eq!(info.video_codec.as_deref(), Some("h264"));
        assert_eq!((info.width, info.height), (Some(1920), Some(1080)));
        assert_eq!(info.frame_rate, Some(30.0));
    }

    #[test]
    fn parse_probe_result_video_only_has_no_audio_bitrate() {
        // 无音频流：不应把容器总码率误标为音频码率
        let json = serde_json::json!({
            "format": { "size": "104857600", "duration": "100.0", "bit_rate": "8000000" },
            "streams": [
                { "codec_type": "video", "codec_name": "h264" },
            ],
        });
        let info = parse_probe_result(&json, r"C:\in\a.mp4").unwrap();
        assert_eq!(info.audio_bitrate, None);
        assert_eq!(info.audio_codec, None);
    }

    #[test]
    fn fallback_audio_bps_edges() {
        // 正常：容器 − 视频
        assert_eq!(fallback_audio_bps(8_388_608.0, Some(8_000_000.0)), 388_608.0);
        // 视频码率 ≥ 容器：clamp 到容器总码率的 1%，不出现 0/负
        assert_eq!(fallback_audio_bps(8_000_000.0, Some(8_000_000.0)), 80_000.0);
        assert_eq!(fallback_audio_bps(8_000_000.0, Some(9_000_000.0)), 80_000.0);
        // 视频码率缺失：容器总码率的 10%
        assert_eq!(fallback_audio_bps(8_388_608.0, None), 838_860.8);
    }

    #[test]
    fn audio_fallback_guard_zero_duration() {
        // 时长 0：容器码率回退必须返回 None，防止 size*8/0 → inf → u64::MAX 污染
        let json = serde_json::json!({
            "format": { "size": "104857600", "duration": "0" },
            "streams": [
                { "codec_type": "video", "codec_name": "h264", "bit_rate": "8000000" },
                { "codec_type": "audio", "codec_name": "aac" },
            ],
        });
        let info = parse_probe_result(&json, r"C:\in\a.mp4").unwrap();
        assert_eq!(info.audio_bitrate, None);
        assert_eq!(info.duration, Some(0.0));
    }
}
