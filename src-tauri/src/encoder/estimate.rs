use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::commands::encode::FileInfo;
use crate::encoder::codec::{AudioCodec, EncodeConfig, RateControl, VideoCodec};
use crate::encoder::probe::{fallback_audio_bps, find_main_video_stream};

/// 包含预估期望值（预期中位数）、乐观值（平缓场景下限）与悲观值（剧烈动态/高噪点场景上限）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EstimatedSize {
    /// 预估期望输出体积（字节）
    pub expected: u64,
    /// 平缓/低动态画面下限预估（字节）
    pub min: u64,
    /// 激烈/高动态/噪点画面上限预估（字节）
    pub max: u64,
}

/// 预估压缩后的输出体积（字节），返回单一体积值（expected）。
///
/// 供队列 Pending 状态持久化及旧有接口兼容调用。
pub fn estimate_output_bytes(config: &EncodeConfig, probe: &Value) -> Option<u64> {
    estimate_output_size(config, probe).map(|e| e.expected)
}

/// 预估压缩后的输出体积区间（字节）。
///
/// 编码开始前（Pending 状态）无法拿到实际写出大小，结合多维度模型推算：
/// - ABR：目标码率直接确定，min == expected == max。
/// - CRF / CQP：
///   1. 源视频编码效率与目标编码效率的比值转换（消除 AV1->AV1 等同代编码二次折减偏差）；
///   2. 物理像素通量基线锚点（BPP 模型，防止源片极端冗余或极端欠压导致预测跑偏）；
///   3. 硬件编码芯片（NVENC/QSV/AMF）高帧率动态膨胀补偿；
///   4. Auto Maxrate Guard 与用户自定义 -maxrate 硬截断；
///   5. 生成平缓场景（下限）与高动态/噪点场景（上限）的合理区间。
pub fn estimate_output_size(config: &EncodeConfig, probe: &Value) -> Option<EstimatedSize> {
    let format = probe.get("format")?;
    let duration = format
        .get("duration")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok())?;
    if duration <= 0.0 {
        return None;
    }

    // 输入平均码率（kbps）：优先 ffprobe 的 format.bit_rate，缺失时由文件大小 / 时长推算
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

    // 输入视频流信息（跳过内嵌封面）
    let video_stream = probe.get("streams").and_then(find_main_video_stream);
    let in_width = video_stream.and_then(|s| s.get("width")).and_then(|v| v.as_u64()).map(|w| w as u32);
    let in_height = video_stream.and_then(|s| s.get("height")).and_then(|v| v.as_u64()).map(|h| h as u32);
    let in_fps = video_stream.and_then(|s| s.get("r_frame_rate")).and_then(|v| v.as_str()).and_then(parse_fps_str);
    let source_codec = video_stream.and_then(|s| s.get("codec_name")).and_then(|v| v.as_str());

    let scale = output_scale_factor(config, in_width, in_height, in_fps);
    let media = SourceMediaInfo {
        source_codec,
        source_audio_kbps: audio_stream_kbps(probe),
        in_width,
        in_height,
        in_fps,
        scale,
    };

    estimate_bytes(config, duration, input_kbps, &media)
}

/// 基于前端已探测的 `FileInfo` 预估输出体积（字节单值）。
#[allow(dead_code)]
pub fn estimate_output_bytes_from_info(config: &EncodeConfig, info: &FileInfo) -> Option<u64> {
    estimate_output_size_from_info(config, info).map(|e| e.expected)
}

/// 基于前端已探测的 `FileInfo` 预估输出体积区间。
pub fn estimate_output_size_from_info(config: &EncodeConfig, info: &FileInfo) -> Option<EstimatedSize> {
    let duration = info.duration?;
    if duration <= 0.0 {
        return None;
    }

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
    let media = SourceMediaInfo {
        source_codec: info.video_codec.as_deref(),
        source_audio_kbps,
        in_width: info.width,
        in_height: info.height,
        in_fps: info.frame_rate,
        scale,
    };
    estimate_bytes(config, duration, input_kbps, &media)
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

/// 源媒体流信息打包，避免函数参数过多触发 clippy 告警
struct SourceMediaInfo<'a> {
    source_codec: Option<&'a str>,
    source_audio_kbps: Option<f64>,
    in_width: Option<u32>,
    in_height: Option<u32>,
    in_fps: Option<f64>,
    scale: f64,
}

/// 公共预估核心：目标码率（或 CRF/CQP 外推与锚点融合）× 时长 → 字节区间。
fn estimate_bytes(
    config: &EncodeConfig,
    duration: f64,
    input_kbps: f64,
    media: &SourceMediaInfo<'_>,
) -> Option<EstimatedSize> {
    // 视频流直接复制时码率设置不生效，体积基本不变（三种码控共用同一出口）
    if matches!(config.video_codec, VideoCodec::Copy) {
        let b = input_kbps_to_bytes(input_kbps, duration)?;
        return Some(EstimatedSize { expected: b, min: b, max: b });
    }

    let (exp_v, min_v, max_v) = estimate_video_kbps(config, input_kbps, media);

    let audio_kbps = match config.audio_settings.codec {
        AudioCodec::None => 0.0,
        AudioCodec::Copy => media.source_audio_kbps.unwrap_or(0.0),
        _ => config.audio_settings.bitrate_kbps as f64,
    };

    let expected = input_kbps_to_bytes(exp_v + audio_kbps, duration)?;
    let min = input_kbps_to_bytes(min_v + audio_kbps, duration).unwrap_or(expected);
    let max = input_kbps_to_bytes(max_v + audio_kbps, duration).unwrap_or(expected);

    Some(EstimatedSize {
        expected,
        min: min.min(expected),
        max: max.max(expected),
    })
}

/// 各编码器与硬件环境下的画质参考基准点（对应视觉平衡参考值）：
/// - H.264 CPU 基准为 18.0；
/// - H.265 CPU 基准为 20.0，硬件为 22.0；
/// - AV1 CPU 基准为 24.0，硬件为 26.0；
/// - VP9 CPU 基准为 24.0。
fn crf_baseline(codec: &VideoCodec, hw: Option<&crate::encoder::codec::HwAccelConfig>) -> f64 {
    match codec {
        VideoCodec::H264 => 18.0,
        VideoCodec::H265 => {
            if hw.is_some() { 22.0 } else { 20.0 }
        }
        VideoCodec::AV1 => {
            if hw.is_some() { 26.0 } else { 24.0 }
        }
        VideoCodec::VP9 => 24.0,
        VideoCodec::Copy => 18.0,
    }
}

/// 源文件视频编码器基准压缩因子（相对 H.264 视同 1.0 的相对能效）。
fn source_codec_factor(codec_name: Option<&str>) -> f64 {
    let name = match codec_name {
        Some(s) => s.trim().to_lowercase(),
        None => return 1.0,
    };
    match name.as_str() {
        "hevc" | "h265" | "hev1" | "hvc1" => 0.65,
        "av1" | "av01" => 0.50,
        "vp9" => 0.60,
        "vp8" => 1.10,
        "mpeg4" | "msmpeg4" | "xvid" => 1.30,
        "mpeg2video" => 1.80,
        "prores" | "dnxhd" | "dnxhr" | "mjpeg" | "rawvideo" => 3.5,
        _ => 1.0, // default h264 or standard
    }
}

/// 目标编码器相对 H.264 的压缩因子（同质量下体积更小）
fn codec_factor(codec: &VideoCodec) -> f64 {
    match codec {
        VideoCodec::H264 => 1.0,
        VideoCodec::H265 => 0.65,
        VideoCodec::AV1 => 0.5,
        VideoCodec::VP9 => 0.6,
        VideoCodec::Copy => 1.0,
    }
}

/// 硬件编码器动态膨胀系数：
/// 显卡芯片追求吞吐与低时延，帧间预测深度不及 CPU 细致，在同等质量下容易输出更高码率；
/// 区分硬件设备类型并对高帧率（>=50fps）适当补足动态系数。
fn hardware_accel_factor(
    hw_accel: Option<&crate::encoder::codec::HwAccelConfig>,
    fps: Option<f64>,
) -> f64 {
    use crate::encoder::codec::HwAccelDevice;
    let Some(hw) = hw_accel else {
        return 1.0;
    };
    let base = match hw.device {
        HwAccelDevice::NVENC => 1.30,
        HwAccelDevice::QSV => 1.20,
        HwAccelDevice::AMF => 1.35,
        HwAccelDevice::VAAPI | HwAccelDevice::VideoToolbox => 1.25,
    };
    if let Some(f) = fps {
        if f >= 50.0 {
            return base + 0.15;
        } else if f <= 25.0 {
            return (base - 0.05).max(1.0);
        }
    }
    base
}

/// 基于分辨率与帧率的物理像素通量基线锚点（Bits Per Pixel, BPP 模型）。
/// 标准 1080p 30fps 在 H.264 CRF 23 下典型码率约 3200 kbps (bpp ≈ 0.0514)。
fn physical_anchor_crf_kbps(
    config: &EncodeConfig,
    in_width: Option<u32>,
    in_height: Option<u32>,
    in_fps: Option<f64>,
    hw_factor: f64,
    crf_val: f64,
) -> f64 {
    let width = config.video_settings.resolution.as_ref()
        .filter(|r| r.width > 0 && r.height > 0)
        .map(|r| r.width)
        .or(in_width)
        .unwrap_or(1920);
    let height = config.video_settings.resolution.as_ref()
        .filter(|r| r.width > 0 && r.height > 0)
        .map(|r| r.height)
        .or(in_height)
        .unwrap_or(1080);
    let fps = config.video_settings.frame_rate
        .filter(|f| *f > 0.0)
        .or(in_fps)
        .unwrap_or(30.0);

    let pps = (width as f64) * (height as f64) * fps;
    let base_h264_kbps = pps * 0.05144 / 1000.0;

    let baseline = crf_baseline(&config.video_codec, config.hw_accel.as_ref());
    let quality_ratio = 2f64.powf((baseline - crf_val) / 6.0) * 0.8;
    (base_h264_kbps * codec_factor(&config.video_codec) * hw_factor * quality_ratio).max(1.0)
}

/// 获取当前转码任务的有效 -maxrate 限制（Auto Maxrate Guard 或用户自定义 -maxrate），用于封顶截断。
fn effective_maxrate_kbps(config: &EncodeConfig, input_kbps: f64) -> Option<f64> {
    // 1. 用户显式指定的 -maxrate
    let params = &config.video_settings.additional_params;
    if let Some(pos) = params.iter().position(|a| a == "-maxrate") {
        if let Some(val_str) = params.get(pos + 1) {
            let s = val_str.trim().to_lowercase();
            let parsed = if let Some(m) = s.strip_suffix('m') {
                m.parse::<f64>().ok().map(|n| n * 1000.0)
            } else if let Some(k) = s.strip_suffix('k') {
                k.parse::<f64>().ok()
            } else {
                s.parse::<f64>().ok().map(|n| n / 1000.0)
            };
            if let Some(v) = parsed {
                return Some(v);
            }
        }
    }

    // 2. Auto Maxrate Guard：硬件加速恒定质量转码时自动封顶在 1.25x 输入码率（底线 500k）
    if config.hw_accel.is_some()
        && !matches!(config.video_settings.rate_control, RateControl::Abr { .. })
        && input_kbps > 0.0
    {
        let guard = (input_kbps * 1.25).round().max(500.0);
        return Some(guard);
    }

    None
}

/// 预估视频码率（期望值、平缓下限、动态上限）
fn estimate_video_kbps(
    config: &EncodeConfig,
    input_kbps: f64,
    media: &SourceMediaInfo<'_>,
) -> (f64, f64, f64) {
    let crf_val = match &config.video_settings.rate_control {
        RateControl::Abr { bitrate_kbps, .. } => {
            let b = *bitrate_kbps as f64;
            return (b, b, b);
        }
        RateControl::Crf { value } => *value as f64,
        RateControl::Cqp { value } => *value as f64,
    };

    let hw_factor = hardware_accel_factor(config.hw_accel.as_ref(), media.in_fps);
    let src_factor = source_codec_factor(media.source_codec);
    let target_factor = codec_factor(&config.video_codec);
    // 相对源编码格式的换算比（例如 H.264->AV1 为 0.5/1.0 = 0.5；AV1->AV1 为 0.5/0.5 = 1.0）
    let relative_factor = (target_factor / src_factor).clamp(0.2, 3.0);

    let baseline = crf_baseline(&config.video_codec, config.hw_accel.as_ref());
    let quality_ratio = 2f64.powf((baseline - crf_val) / 6.0) * 0.8;
    let relative_kbps = (input_kbps * relative_factor * hw_factor * quality_ratio * media.scale).max(1.0);

    // 物理像素通量锚点码率
    let anchor_kbps = physical_anchor_crf_kbps(
        config,
        media.in_width,
        media.in_height,
        media.in_fps,
        hw_factor,
        crf_val,
    );

            // 65% 相对源片推算 + 35% 物理像素通量锚点，兼顾源片特征与物理客观基准
            let mut expected = 0.65 * relative_kbps + 0.35 * anchor_kbps;

            // 动态范围：平缓/静态场景 0.75x ~ 高动态/噪点场景 1.40x
            let mut min = (expected * 0.75).max(1.0);
            let mut max = (expected * 1.40).max(expected);

            // Maxrate 上限截断（Auto Maxrate Guard 或用户指定）
            if let Some(maxrate) = effective_maxrate_kbps(config, input_kbps) {
                expected = expected.min(maxrate);
                max = max.min(maxrate).max(expected);
                min = min.min(expected);
            }

    (expected, min, max)
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
                codec: AudioCodec::Aac,
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
        assert!((2800.0..4200.0).contains(&kbps_implied), "implied kbps: {}", kbps_implied);
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
        cfg.audio_settings.codec = AudioCodec::Copy;
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
        cfg.audio_settings.codec = AudioCodec::Copy;
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
        cfg.audio_settings.codec = AudioCodec::None;
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

    #[test]
    fn hw_accel_factor_increases_crf_estimate() {
        let mut cpu_cfg = base_config();
        cpu_cfg.audio_settings.codec = AudioCodec::None;
        let cpu_est = estimate_output_bytes(&cpu_cfg, &probe_json(100.0, 8_000_000, 100_000_000)).unwrap();

        let mut hw_cfg = cpu_cfg.clone();
        hw_cfg.hw_accel = Some(HwAccelConfig {
            device: crate::encoder::codec::HwAccelDevice::NVENC,
            device_index: None,
        });
        let hw_est = estimate_output_bytes(&hw_cfg, &probe_json(100.0, 8_000_000, 100_000_000)).unwrap();

        // 硬件加速预估体积应显著高于 CPU 预估（NVENC 约 1.25~1.35 倍）
        let ratio = hw_est as f64 / cpu_est as f64;
        assert!((1.20..1.40).contains(&ratio), "hw/cpu ratio: {ratio}");
    }

    /// 源视频编码格式相对能效换算：原片为 AV1 时转 AV1 不应再次按 H264 基准腰斩
    #[test]
    fn source_codec_relative_efficiency() {
        let mut cfg = base_config();
        cfg.video_codec = VideoCodec::AV1;
        cfg.audio_settings.codec = AudioCodec::None;

        let mut info_h264 = sample_info();
        info_h264.video_codec = Some("h264".into());
        let est_from_h264 = estimate_output_bytes_from_info(&cfg, &info_h264).unwrap();

        let mut info_av1 = sample_info();
        info_av1.video_codec = Some("av1".into());
        let est_from_av1 = estimate_output_bytes_from_info(&cfg, &info_av1).unwrap();

        // 原片已是 AV1，再转 AV1 的预估码率应明显高于从低效 H.264 重新转成 AV1 的折算码率
        assert!(est_from_av1 > est_from_h264, "av1->av1 ({est_from_av1}) should be > h264->av1 ({est_from_h264})");
    }

    /// Auto Maxrate Guard 上限硬截断保护
    #[test]
    fn auto_maxrate_guard_clamps_estimate() {
        let mut cfg = base_config();
        cfg.hw_accel = Some(HwAccelConfig {
            device: crate::encoder::codec::HwAccelDevice::NVENC,
            device_index: None,
        });
        // 极高画质 CRF 10（理论码率会飙升）
        cfg.video_settings.rate_control = RateControl::Crf { value: 10 };
        let info = sample_info(); // 8000 kbps 输入，100s
        let est = estimate_output_size_from_info(&cfg, &info).unwrap();

        // Auto Maxrate Guard 限制为 8000 * 1.25 = 10000 kbps (加音频 192k)
        let max_allowed_bytes = ((10000.0 + 192.0) * 1000.0 / 8.0 * 100.0) as u64;
        assert!(est.max <= max_allowed_bytes + 1000, "est.max ({}) should not exceed guard limit ({})", est.max, max_allowed_bytes);
    }

    /// 验证 EstimatedSize 区间：CRF 下 min < expected < max，ABR 下 min == expected == max
    #[test]
    fn estimate_output_size_returns_min_max_range() {
        let crf_cfg = base_config();
        let crf_size = estimate_output_size_from_info(&crf_cfg, &sample_info()).unwrap();
        assert!(crf_size.min < crf_size.expected);
        assert!(crf_size.expected < crf_size.max);

        let mut abr_cfg = base_config();
        abr_cfg.video_settings.rate_control = RateControl::Abr {
            bitrate_kbps: 2000,
            max_bitrate_kbps: None,
        };
        let abr_size = estimate_output_size_from_info(&abr_cfg, &sample_info()).unwrap();
        assert_eq!(abr_size.min, abr_size.expected);
        assert_eq!(abr_size.expected, abr_size.max);
    }
}
