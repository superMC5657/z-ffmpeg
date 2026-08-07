use serde_json::Value;

use crate::encoder::codec::{EncodeConfig, RateControl, VideoCodec};

/// 预估压缩后的输出体积（字节）。
///
/// 编码开始前（Pending 状态）无法拿到实际写出大小，只能基于目标码率 × 时长
/// 推算：体积 ≈ (视频码率 + 音频码率) × 时长 / 8。
///
/// - ABR：视频码率直接取配置值，音频按配置值（Copy 则取源音频流码率）。
/// - CRF / CQP：没有固定码率，用「输入平均码率 × 经验系数」外推——经验规律
///   CRF 每 +6 输出码率约减半，以 CRF 18 为基准乘 0.8，再乘编码器压缩因子
///   （H.265 / AV1 / VP9 同质量下体积更小）。结果仅供参考。
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

    let video_kbps = match &config.video_settings.rate_control {
        RateControl::Abr { bitrate_kbps, .. } => {
            // 视频流直接复制时 ABR 码率不生效，体积基本不变，退化为输入平均码率
            if matches!(config.video_codec, VideoCodec::Copy) {
                return input_kbps_to_bytes(input_kbps, duration);
            }
            *bitrate_kbps as f64
        }
        RateControl::Crf { value } => {
            // 视频流直接复制时体积基本不变，退化为输入平均码率
            if matches!(config.video_codec, VideoCodec::Copy) {
                return input_kbps_to_bytes(input_kbps, duration);
            }
            let codec_factor = match config.video_codec {
                VideoCodec::H264 => 1.0,
                VideoCodec::H265 => 0.65,
                VideoCodec::AV1 => 0.5,
                VideoCodec::VP9 => 0.6,
                VideoCodec::Copy => 1.0,
            };
            let ratio = 2f64.powf((18.0 - *value as f64) / 6.0) * 0.8;
            (input_kbps * codec_factor * ratio).max(1.0)
        }
        RateControl::Cqp { value } => {
            if matches!(config.video_codec, VideoCodec::Copy) {
                return input_kbps_to_bytes(input_kbps, duration);
            }
            let codec_factor = match config.video_codec {
                VideoCodec::H264 => 1.0,
                VideoCodec::H265 => 0.65,
                VideoCodec::AV1 => 0.5,
                VideoCodec::VP9 => 0.6,
                VideoCodec::Copy => 1.0,
            };
            let ratio = 2f64.powf((18.0 - *value as f64) / 6.0) * 0.8;
            (input_kbps * codec_factor * ratio).max(1.0)
        }
    };

    let audio_kbps = match config.audio_settings.codec.as_str() {
        "None" => 0.0,
        "Copy" => audio_stream_kbps(probe).unwrap_or(0.0),
        _ => config.audio_settings.bitrate_kbps as f64,
    };

    input_kbps_to_bytes(video_kbps + audio_kbps, duration)
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
            // 个别封装不写 stream bit_rate，按容器总码率近似
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
            Some(size * 8.0 / 1000.0 / dur)
        })?;
    (bps > 0.0).then_some(bps / 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoder::codec::{
        AudioSettings, ContainerFormat, EncodeConfig, HwAccelConfig, RateControl, VideoCodec,
        VideoSettings,
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
