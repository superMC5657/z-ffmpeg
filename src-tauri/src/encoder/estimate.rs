use serde_json::Value;

use crate::commands::encode::FileInfo;
use crate::encoder::codec::{EncodeConfig, RateControl, VideoCodec};
use crate::encoder::engine::{fallback_audio_bps, find_main_video_stream};

/// 预估压缩后的输出体积（字节）。
///
/// 编码开始前（Pending 状态）无法拿到实际写出大小，只能基于目标码率 × 时长
/// 推算：体积 ≈ (视频码率 + 音频码率) × 时长 / 8。
///
/// - ABR：视频码率直接取配置值，音频按配置值（Copy 则取源音频流码率）。
///   ABR 为固定码率，体积与分辨率/帧率无关（只影响画质），不应用缩放。
/// - CRF / CQP：没有固定码率，用「输入平均码率 × 经验系数」外推——经验规律
///   CRF 每 +6 输出码率约减半，以 CRF 18 为基准乘 0.8，再乘编码器压缩因子
///   （H.265 / AV1 / VP9 同质量下体积更小）；输出分辨率/帧率变化时按像素面积
///   比 × 帧率比缩放（分辨率降低、帧率降低 → 体积变小）。结果仅供参考。
/// - 探测信息不足（无时长 / 无码率）时返回 `None`，前端不展示。
pub fn estimate_output_bytes(config: &EncodeConfig, probe: &Value) -> Option<u64> {
    let format = probe.get("format")?;
    let duration = format
        .get("duration")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok())?;
    if duration <= 0.0 {
        return None;
    }

    // 输入平均码率（kbps）：优先 ffprobe 的 format.bit_rate，缺失时由
    // 文件大小 / 时长推算。
    let input_kbps = format
        .get("bit_rate")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok())
        .map(|bps| bps / 1000.0)
        .or_else(|| {
            let size = format
                .get("size")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<f64>().ok())?;
            Some(size * 8.0 / 1000.0 / duration)
        })?;
    if input_kbps <= 0.0 {
        return None;
    }

    // 输入视频流分辨率 / 帧率（输出缩放用；仅 CRF/CQP 生效）。
    // 跳过内嵌封面（attached_pic）流；streams 缺失时退化为不缩放，不影响预估本身。
    let video_stream = probe.get("streams").and_then(find_main_video_stream);
    let in_width = video_stream.and_then(|s| s.get("width")).and_then(|v| v.as_u64()).map(|w| w as u32);
    let in_height = video_stream.and_then(|s| s.get("height")).and_then(|v| v.as_u64()).map(|h| h as u32);
    let in_fps = video_stream.and_then(|s| s.get("r_frame_rate")).and_then(|v| v.as_str()).and_then(parse_fps_str);

    estimate_bytes(
        config,
        duration,
        input_kbps,
        audio_stream_kbps(probe),
        output_scale_factor(config, in_width, in_height, in_fps),
    )
}

/// 基于前端已探测的 `FileInfo` 预估输出体积（字节）。
///
/// 编码页实时预览用：不依赖 probe JSON、无 I/O 的纯算术计算，参数变化时
/// 可反复调用。与 `estimate_output_bytes` 共用同一推算核心，保证与队列
/// Pending 阶段的预估一致。
pub fn estimate_output_bytes_from_info(config: &EncodeConfig, info: &FileInfo) -> Option<u64> {
    let duration = info.duration?;
    if duration <= 0.0 {
        return None;
    }

    // 输入平均码率（kbps）：优先容器总码率，缺失时由文件大小 / 时长推算
    let input_kbps = info
        .bitrate
        .map(|bps| bps as f64 / 1000.0)
        .or_else(|| {
            if info.file_size == 0 {
                return None;
            }
            Some(info.file_size as f64 * 8.0 / 1000.0 / duration)
        })?;
    if input_kbps <= 0.0 {
        return None;
    }

    let source_audio_kbps = info.audio_bitrate.map(|bps| bps as f64 / 1000.0);
    let scale = output_scale_factor(config, info.width, info.height, info.frame_rate);
    estimate_bytes(config, duration, input_kbps, source_audio_kbps, scale)
}

/// 输出缩放因子：分辨率按像素面积比、帧率按比例缩放（仅 CRF/CQP 生效）。
/// 输入缺分辨率/帧率或未设置输出时保持 1.0；clamp 防止极端参数把预估推到离谱范围。
fn output_scale_factor(
    config: &EncodeConfig,
    in_width: Option<u32>,
    in_height: Option<u32>,
    in_fps: Option<f64>,
) -> f64 {
    let mut factor = 1.0;
    if let Some(res) = &config.video_settings.resolution {
        // 任一维度 ≤ 0 视为未设置分辨率（保持原始），避免 0/面积 → clamp 到极小值
        if res.width > 0 && res.height > 0 {
            if let (Some(w), Some(h)) = (in_width, in_height) {
                let in_area = (w as f64) * (h as f64);
                if in_area > 0.0 {
                    let out_area = (res.width as f64) * (res.height as f64);
                    factor *= (out_area / in_area).clamp(0.05, 4.0);
                }
            }
        }
    }
    if let Some(out_fps) = config.video_settings.frame_rate {
        if let Some(in_fps) = in_fps {
            if in_fps > 0.0 && out_fps > 0.0 {
                factor *= (out_fps / in_fps).clamp(0.1, 4.0);
            }
        }
    }
    factor
}

/// 解析 ffprobe 的 r_frame_rate（"30000/1001" 或 "30"）为 fps
fn parse_fps_str(s: &str) -> Option<f64> {
    let parts: Vec<&str> = s.split('/').collect();
    if parts.len() == 2 {
        let num = parts[0].parse::<f64>().ok()?;
        let den = parts[1].parse::<f64>().ok()?;
        (den > 0.0).then(|| num / den)
    } else {
        s.parse::<f64>().ok().filter(|v| *v > 0.0)
    }
}

/// 公共预估核心：目标码率（或 CRF/CQP 外推）× 时长 → 字节。
/// `source_audio_kbps` 为源音频流码率（音频 Copy 时用），探测不到时为 `None`；
/// `scale` 为分辨率/帧率缩放因子（仅 CRF/CQP 应用，ABR/Copy 忽略）。
fn estimate_bytes(
    config: &EncodeConfig,
    duration: f64,
    input_kbps: f64,
    source_audio_kbps: Option<f64>,
    scale: f64,
) -> Option<u64> {
    let video_kbps = match &config.video_settings.rate_control {
        RateControl::Abr { bitrate_kbps, .. } => {
            // 视频流直接复制时 ABR 码率不生效，体积基本不变，退化为输入平均码率
            if matches!(config.video_codec, VideoCodec::Copy) {
                return input_kbps_to_bytes(input_kbps, duration);
            }
            // ABR 固定码率：分辨率/帧率只影响画质，不改变体积，不应用 scale
            *bitrate_kbps as f64
        }
        RateControl::Crf { value } => {
            // 视频流直接复制时体积基本不变，退化为输入平均码率
            if matches!(config.video_codec, VideoCodec::Copy) {
                return input_kbps_to_bytes(input_kbps, duration);
            }
            let ratio = 2f64.powf((18.0 - *value as f64) / 6.0) * 0.8;
            (input_kbps * codec_factor(&config.video_codec) * ratio * scale).max(1.0)
        }
        RateControl::Cqp { value } => {
            if matches!(config.video_codec, VideoCodec::Copy) {
                return input_kbps_to_bytes(input_kbps, duration);
            }
            let ratio = 2f64.powf((18.0 - *value as f64) / 6.0) * 0.8;
            (input_kbps * codec_factor(&config.video_codec) * ratio * scale).max(1.0)
        }
    };

    let audio_kbps = match config.audio_settings.codec.as_str() {
        "None" => 0.0,
        "Copy" => source_audio_kbps.unwrap_or(0.0),
        _ => config.audio_settings.bitrate_kbps as f64,
    };

    input_kbps_to_bytes(video_kbps + audio_kbps, duration)
}

/// 编码器相对 H.264 的压缩因子（同质量下体积更小）
fn codec_factor(codec: &VideoCodec) -> f64 {
    match codec {
        VideoCodec::H264 => 1.0,
        VideoCodec::H265 => 0.65,
        VideoCodec::AV1 => 0.5,
        VideoCodec::VP9 => 0.6,
        VideoCodec::Copy => 1.0,
    }
}

/// kbps × 秒 → 字节
fn input_kbps_to_bytes(kbps: f64, duration: f64) -> Option<u64> {
    if kbps <= 0.0 {
        return None;
    }
    Some((kbps * 1000.0 / 8.0 * duration) as u64)
}

/// 源文件中音频流的码率（kbps），供音频 Copy 时估算用。
fn audio_stream_kbps(probe: &Value) -> Option<f64> {
    let streams = probe.get("streams")?.as_array()?;
    let audio = streams.iter().find(|s| {
        s.get("codec_type").and_then(|v| v.as_str()) == Some("audio")
    })?;
    let bps = audio
        .get("bit_rate")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok())
        .or_else(|| {
            // 个别封装不写 stream bit_rate：用「容器总码率 − 视频流码率」近似，
            // 与 engine.rs `fallback_audio_bps` 一致，避免把整个容器当成音频
            let dur = probe
                .get("format")?
                .get("duration")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<f64>().ok())?;
            let size = probe
                .get("format")?
                .get("size")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<f64>().ok())?;
            let container_bps = size * 8.0 / dur;
            let video_bps = probe
                .get("streams")
                .and_then(find_main_video_stream)
                .and_then(|s| s.get("bit_rate"))
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<f64>().ok());
            Some(fallback_audio_bps(container_bps, video_bps))
        })?;
    (bps > 0.0).then_some(bps / 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoder::codec::{
        AudioSettings, ContainerFormat, EncodeConfig, HwAccelConfig, RateControl, Resolution,
        VideoCodec, VideoSettings,
    };
    use serde_json::json;

    fn base_config() -> EncodeConfig {
        EncodeConfig {
            video_codec: VideoCodec::H264,
            video_settings: VideoSettings {
                rate_control: RateControl::Crf { value: 23 },
                encoder_preset: "medium".into(),
                resolution: None,
                frame_rate: None,
                pixel_format: None,
                profile: None,
                additional_params: vec![],
            },
            audio_settings: AudioSettings {
                codec: "AAC".into(),
                bitrate_kbps: 128,
                channels: 2,
                sample_rate: 44100,
            },
            container_format: ContainerFormat::MP4,
            hw_accel: None::<HwAccelConfig>,
        }
    }

    fn probe_json(duration: f64, bit_rate: u64, size: u64) -> Value {
        json!({
            "format": {
                "duration": duration.to_string(),
                "bit_rate": bit_rate.to_string(),
                "size": size.to_string(),
            },
            "streams": [
                { "codec_type": "video", "bit_rate": "4000000" },
                { "codec_type": "audio", "bit_rate": "192000" },
            ],
        })
    }

    #[test]
    fn abr_uses_configured_bitrate() {
        let mut cfg = base_config();
        cfg.video_settings.rate_control = RateControl::Abr {
            bitrate_kbps: 2000,
            max_bitrate_kbps: None,
        };
        // (2000 + 128) kbps × 100s → bytes
        let expected = (2128.0 * 1000.0 / 8.0 * 100.0) as u64;
        assert_eq!(
            estimate_output_bytes(&cfg, &probe_json(100.0, 8_000_000, 100_000_000)),
            Some(expected)
        );
    }

    #[test]
    fn abr_with_copy_video_falls_back_to_input_bitrate() {
        let mut cfg = base_config();
        cfg.video_codec = VideoCodec::Copy;
        cfg.video_settings.rate_control = RateControl::Abr {
            bitrate_kbps: 2000, // Copy 下不生效
            max_bitrate_kbps: None,
        };
        // 输入 8000kbps + 音频 128kbps ≈ 8128kbps × 100s
        let est = estimate_output_bytes(&cfg, &probe_json(100.0, 8_000_000, 100_000_000));
        assert!(est.is_some());
        let kbps_implied = est.unwrap() as f64 / 100.0 * 8.0 / 1000.0;
        // 应接近输入总码率（8000 + 128），而不是 2128（ABR 配置值）
        assert!((8000.0..8300.0).contains(&kbps_implied), "implied kbps: {}", kbps_implied);
    }

    #[test]
    fn crf_scales_with_input_bitrate() {
        let cfg = base_config(); // H264 CRF 23
        let est = estimate_output_bytes(&cfg, &probe_json(100.0, 8_000_000, 100_000_000));
        // 输入 8000kbps × 0.8 × 2^((18-23)/6) ≈ 3592kbps + 128kbps 音频
        assert!(est.is_some());
        let bytes = est.unwrap();
        let kbps_implied = bytes as f64 / 100.0 * 8.0 / 1000.0;
        assert!((3000.0..4200.0).contains(&kbps_implied), "implied kbps: {}", kbps_implied);
    }

    #[test]
    fn missing_duration_returns_none() {
        let cfg = base_config();
        assert_eq!(estimate_output_bytes(&cfg, &json!({"format": {}})), None);
    }

    /// 无 streams 的 probe（极端情况）应退化为不缩放，仍能正常预估
    #[test]
    fn probe_without_streams_still_estimates() {
        let cfg = base_config();
        let json = json!({
            "format": { "duration": "100.0", "bit_rate": "8000000", "size": "100000000" },
        });
        let est = estimate_output_bytes(&cfg, &json);
        assert!(est.is_some());
        // 与带空 streams 的结果一致（无缩放）
        let with_empty = estimate_output_bytes(&cfg, &json!({
            "format": { "duration": "100.0", "bit_rate": "8000000", "size": "100000000" },
            "streams": [],
        }));
        assert_eq!(est, with_empty);
    }

    /// 带内嵌封面（attached_pic，列在首位）的 probe：缩放必须取主视频尺寸而非封面
    #[test]
    fn probe_skips_cover_stream_for_scale() {
        let mut cfg = base_config();
        cfg.video_settings.resolution = Some(Resolution { width: 1280, height: 720 });
        // 封面 400×300 在首位（若误用 → out/in = 0.576/0.12 会 clamp 到 4.0）
        let with_cover = json!({
            "format": { "duration": "100.0", "bit_rate": "8000000", "size": "100000000" },
            "streams": [
                { "codec_type": "video", "width": 400, "height": 300,
                  "disposition": { "attached_pic": 1 } },
                { "codec_type": "video", "width": 1920, "height": 1080, "r_frame_rate": "30/1" },
                { "codec_type": "audio", "bit_rate": "192000" },
            ],
        });
        let without_cover = json!({
            "format": { "duration": "100.0", "bit_rate": "8000000", "size": "100000000" },
            "streams": [
                { "codec_type": "video", "width": 1920, "height": 1080, "r_frame_rate": "30/1" },
                { "codec_type": "audio", "bit_rate": "192000" },
            ],
        });
        assert_eq!(
            estimate_output_bytes(&cfg, &with_cover),
            estimate_output_bytes(&cfg, &without_cover)
        );
    }

    /// 音频流缺 bit_rate 时，probe 与 FileInfo 两条路径按同一「容器 − 视频」回退，结果一致
    #[test]
    fn audio_fallback_consistent_between_paths() {
        let mut cfg = base_config();
        cfg.video_settings.rate_control = RateControl::Abr {
            bitrate_kbps: 2000,
            max_bitrate_kbps: None,
        };
        cfg.audio_settings.codec = "Copy".into();
        // 容器 104857600B / 100s ≈ 8388608 bps，视频 8000000 bps → 音频回退 ≈ 388608 bps
        let probe = json!({
            "format": { "duration": "100.0", "size": "104857600" },
            "streams": [
                { "codec_type": "video", "bit_rate": "8000000", "width": 1920, "height": 1080 },
                { "codec_type": "audio" },
            ],
        });
        let info = FileInfo {
            path: "C:\\in\\a.mp4".into(),
            file_name: "a.mp4".into(),
            file_size: 104_857_600,
            duration: Some(100.0),
            video_codec: Some("h264".into()),
            audio_codec: Some("aac".into()),
            width: Some(1920),
            height: Some(1080),
            frame_rate: Some(30.0),
            bitrate: None,
            audio_bitrate: Some(388_608), // 与 engine.rs fallback_audio_bps 一致
            pixel_format: Some("yuv420p".into()),
        };
        assert_eq!(
            estimate_output_bytes(&cfg, &probe),
            estimate_output_bytes_from_info(&cfg, &info)
        );
    }

    /// FileInfo 输入与 probe JSON 输入应共用同一推算核心，结果一致
    #[test]
    fn from_info_matches_probe_version() {
        let mut cfg = base_config(); // H264 CRF 23
        // 设置输出分辨率/帧率，验证缩放逻辑在两个入口完全一致
        cfg.video_settings.resolution = Some(Resolution { width: 1280, height: 720 });
        cfg.video_settings.frame_rate = Some(24.0);
        let probe = json!({
            "format": {
                "duration": "100.0",
                "bit_rate": "8000000",
                "size": "100000000",
            },
            "streams": [
                { "codec_type": "video", "bit_rate": "4000000",
                  "width": 1920, "height": 1080, "r_frame_rate": "30/1" },
                { "codec_type": "audio", "bit_rate": "192000" },
            ],
        });
        let info = FileInfo {
            path: "C:\\in\\a.mp4".into(),
            file_name: "a.mp4".into(),
            file_size: 100_000_000,
            duration: Some(100.0),
            video_codec: Some("h264".into()),
            audio_codec: Some("aac".into()),
            width: Some(1920),
            height: Some(1080),
            frame_rate: Some(30.0),
            bitrate: Some(8_000_000),
            audio_bitrate: Some(192_000),
            pixel_format: Some("yuv420p".into()),
        };
        assert_eq!(
            estimate_output_bytes_from_info(&cfg, &info),
            estimate_output_bytes(&cfg, &probe)
        );
    }

    /// CRF 模式下输出分辨率/帧率应缩放预估：面积 0.444 × 帧率 0.8 ≈ 0.356
    #[test]
    fn crf_output_resolution_and_fps_shrink_estimate() {
        let mut cfg = base_config(); // H264 CRF 23，输入 8000kbps / 100s
        let base_est = estimate_output_bytes_from_info(&cfg, &sample_info()).unwrap();
        cfg.video_settings.resolution = Some(Resolution { width: 1280, height: 720 }); // 0.444×
        cfg.video_settings.frame_rate = Some(24.0); // 24/30 = 0.8×
        let shrunk = estimate_output_bytes_from_info(&cfg, &sample_info()).unwrap();
        let ratio = shrunk as f64 / base_est as f64;
        // 视频部分缩放 0.444 × 0.8 ≈ 0.3556；音频 128kbps 不缩放会把整体比例
        // 抬到 ≈0.378（视频 3592kbps → 1277kbps + 128kbps 音频）
        assert!((0.35..0.40).contains(&ratio), "ratio: {ratio}");
    }

    /// ABR 固定码率：分辨率/帧率只影响画质，不改变预估体积
    #[test]
    fn abr_ignores_resolution_and_fps() {
        let mut cfg = base_config();
        cfg.video_settings.rate_control = RateControl::Abr {
            bitrate_kbps: 2000,
            max_bitrate_kbps: None,
        };
        let base_est = estimate_output_bytes_from_info(&cfg, &sample_info()).unwrap();
        cfg.video_settings.resolution = Some(Resolution { width: 1280, height: 720 });
        cfg.video_settings.frame_rate = Some(24.0);
        let same_est = estimate_output_bytes_from_info(&cfg, &sample_info()).unwrap();
        assert_eq!(base_est, same_est);
    }

    /// 输入分辨率缺失时，分辨率缩放因子自动退化为 1（不缩放，仍可预估）
    #[test]
    fn missing_input_resolution_skips_scale() {
        let mut cfg = base_config();
        cfg.video_settings.resolution = Some(Resolution { width: 640, height: 360 });
        let mut info = sample_info();
        info.width = None;
        info.height = None;
        assert!(estimate_output_bytes_from_info(&cfg, &info).is_some());
    }

    /// 分辨率任一维度为 0（如前端输入框被清空）应视为未设置，不缩放、回到原始预估
    #[test]
    fn zero_dimension_resolution_ignored() {
        let mut cfg = base_config();
        let base_est = estimate_output_bytes_from_info(&cfg, &sample_info()).unwrap();
        cfg.video_settings.resolution = Some(Resolution { width: 0, height: 1080 });
        assert_eq!(
            estimate_output_bytes_from_info(&cfg, &sample_info()),
            Some(base_est)
        );
        cfg.video_settings.resolution = Some(Resolution { width: 1920, height: 0 });
        assert_eq!(
            estimate_output_bytes_from_info(&cfg, &sample_info()),
            Some(base_est)
        );
    }

    /// 构造一个带完整视频信息的样例 FileInfo
    fn sample_info() -> FileInfo {
        FileInfo {
            path: "C:\\in\\a.mp4".into(),
            file_name: "a.mp4".into(),
            file_size: 100_000_000,
            duration: Some(100.0),
            video_codec: Some("h264".into()),
            audio_codec: Some("aac".into()),
            width: Some(1920),
            height: Some(1080),
            frame_rate: Some(30.0),
            bitrate: Some(8_000_000),
            audio_bitrate: Some(192_000),
            pixel_format: Some("yuv420p".into()),
        }
    }

    #[test]
    fn from_info_abr_and_audio_copy() {
        let mut cfg = base_config();
        cfg.video_settings.rate_control = RateControl::Abr {
            bitrate_kbps: 2000,
            max_bitrate_kbps: None,
        };
        cfg.audio_settings.codec = "Copy".into();
        let info = FileInfo {
            path: "C:\\in\\a.mp4".into(),
            file_name: "a.mp4".into(),
            file_size: 100_000_000,
            duration: Some(100.0),
            video_codec: Some("h264".into()),
            audio_codec: Some("aac".into()),
            width: Some(1920),
            height: Some(1080),
            frame_rate: Some(30.0),
            bitrate: Some(8_000_000),
            audio_bitrate: Some(192_000),
            pixel_format: Some("yuv420p".into()),
        };
        // 视频 2000kbps + 音频 Copy 192kbps × 100s
        let expected = (2192.0 * 1000.0 / 8.0 * 100.0) as u64;
        assert_eq!(
            estimate_output_bytes_from_info(&cfg, &info),
            Some(expected)
        );
    }

    #[test]
    fn from_info_missing_data_returns_none() {
        let cfg = base_config();
        let mut info = FileInfo {
            path: "C:\\in\\a.mp4".into(),
            file_name: "a.mp4".into(),
            file_size: 0,
            duration: None,
            video_codec: None,
            audio_codec: None,
            width: None,
            height: None,
            frame_rate: None,
            bitrate: None,
            audio_bitrate: None,
            pixel_format: None,
        };
        assert_eq!(estimate_output_bytes_from_info(&cfg, &info), None);
        // 无容器码率但有文件大小 → 由 size/duration 推算
        info.duration = Some(100.0);
        info.file_size = 100_000_000;
        assert!(estimate_output_bytes_from_info(&cfg, &info).is_some());
        // 无码率也无大小 → 无法推算
        info.file_size = 0;
        assert_eq!(estimate_output_bytes_from_info(&cfg, &info), None);
    }

    #[test]
    fn no_audio_reduces_estimate() {
        let mut cfg = base_config();
        cfg.audio_settings.codec = "None".into();
        let est = estimate_output_bytes(&cfg, &probe_json(100.0, 8_000_000, 100_000_000));
        assert!(est.is_some());
    }

    #[test]
    fn bitrate_fallback_uses_size_and_duration() {
        let cfg = base_config();
        // 无 format.bit_rate：由 size(100MB) / 100s 推得输入 8000kbps
        let json = json!({
            "format": { "duration": "100.0", "size": "104857600" },
            "streams": [],
        });
        assert!(estimate_output_bytes(&cfg, &json).is_some());
    }
}
