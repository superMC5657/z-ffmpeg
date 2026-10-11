//! 体积模型拥有者：codec × device 因子 + 物理锚点 + guard 截断。
//!
//! 与 `super::flags` 的分工（C3 按 seam 拆分）：
//!
//! - 本模块唯一拥有体积侧：`crf_baseline` / `source_codec_factor` /
//!   `codec_factor` / `hardware_accel_factor` / `physical_anchor_crf_kbps` /
//!   `estimate_video_kbps`，以及 guard 截断中属于体积的部分
//!   （`effective_maxrate_kbps`）。
//!
//! - guard 解析原语（`should_apply` / `has_custom` / `guard_maxrate_kbps` /
//!   `parse_custom`）唯一拥有者为 `super::flags`，本模块按需导入，不重述 1.25x。

use crate::encoder::codec::{EncodeConfig, HwAccelConfig, HwAccelDevice, RateControl, VideoCodec};
use crate::encoder::flags::{
    guard_maxrate_kbps, parse_custom_maxrate_kbps, should_apply_auto_maxrate_guard,
};

/// 获取当前转码任务的有效 -maxrate 限制（Auto Maxrate Guard 或用户自定义 -maxrate），用于封顶截断。
/// 与搬运前逐行一致：自定义值可解析则优先；不可解析/缺值时穿透到自动 guard。
/// 分歧锁死（搬运前既有，不统一）：CLI 侧以 has_custom 为准、体积侧以 parse 为准，不可解析时前者空、后者回退 guard。
pub fn effective_maxrate_kbps(config: &EncodeConfig, input_kbps: f64) -> Option<f64> {
    // 1. 用户显式指定的 -maxrate
    if let Some(v) = parse_custom_maxrate_kbps(&config.video_settings.additional_params) {
        return Some(v);
    }

    // 2. Auto Maxrate Guard：硬件加速恒定质量转码时自动封顶在 1.25x 输入码率（底线 500k）
    if should_apply_auto_maxrate_guard(config) && input_kbps > 0.0 {
        return Some(guard_maxrate_kbps(input_kbps));
    }

    None
}

// ============================================================
// 体积预估因子（原 estimate.rs，原样搬运）
// ============================================================

/// 各编码器与硬件环境下的画质参考基准点（对应视觉平衡参考值）：
/// - H.264 CPU 基准为 18.0；
/// - H.265 CPU 基准为 20.0，硬件为 22.0；
/// - AV1 CPU 基准为 24.0，硬件为 26.0；
/// - VP9 CPU 基准为 24.0。
pub fn crf_baseline(codec: &VideoCodec, hw: Option<&HwAccelConfig>) -> f64 {
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
pub fn source_codec_factor(codec_name: Option<&str>) -> f64 {
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
        _ => 1.0, // 默认 H264 或标准格式
    }
}

/// 目标编码器相对 H.264 的压缩因子（同质量下体积更小）
pub fn codec_factor(codec: &VideoCodec) -> f64 {
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
pub fn hardware_accel_factor(
    hw_accel: Option<&HwAccelConfig>,
    fps: Option<f64>,
) -> f64 {
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

/// 源媒体流信息打包（体积模型输入的源文件事实），避免函数参数过多触发 clippy 告警
pub struct SourceMediaInfo<'a> {
    pub source_codec: Option<&'a str>,
    pub source_audio_kbps: Option<f64>,
    pub in_width: Option<u32>,
    pub in_height: Option<u32>,
    pub in_fps: Option<f64>,
    pub scale: f64,
}

/// 基于分辨率与帧率的物理像素通量基线锚点（Bits Per Pixel, BPP 模型）。
/// 标准 1080p 30fps 在 H.264 CRF 23 下典型码率约 3200 kbps (bpp ≈ 0.0514)。
pub fn physical_anchor_crf_kbps(
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

/// 预估视频码率（期望值、平缓下限、动态上限）
pub fn estimate_video_kbps(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoder::codec::{AudioSettings, ContainerFormat, VideoSettings};
    use crate::encoder::flags::encoder_name;

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
                codec: crate::encoder::codec::AudioCodec::Aac,
                bitrate_kbps: 128,
                channels: 2,
                sample_rate: 44100,
            },
            container_format: ContainerFormat::MP4,
            hw_accel: None,
        }
    }

    fn hw_config(device: HwAccelDevice) -> EncodeConfig {
        let mut c = base_config();
        c.hw_accel = Some(HwAccelConfig { device, device_index: None });
        c
    }

    fn sample_media(scale: f64) -> SourceMediaInfo<'static> {
        SourceMediaInfo {
            source_codec: Some("h264"),
            source_audio_kbps: Some(192.0),
            in_width: Some(1920),
            in_height: Some(1080),
            in_fps: Some(30.0),
            scale,
        }
    }

    // ---- 矩阵：体积侧 guard 截断（effective_maxrate，唯一拥有者） ----

    #[test]
    fn matrix_effective_maxrate_clamp_snapshot() {
        // 有效 guard：NVENC CRF 4000 → 5000；自定义 3000k 优先；CPU → None
        assert_eq!(effective_maxrate_kbps(&hw_config(HwAccelDevice::NVENC), 4000.0), Some(5000.0));
        let mut custom = hw_config(HwAccelDevice::NVENC);
        custom.video_settings.additional_params = vec!["-maxrate".into(), "3000k".into()];
        assert_eq!(effective_maxrate_kbps(&custom, 4000.0), Some(3000.0));
        assert_eq!(effective_maxrate_kbps(&base_config(), 4000.0), None);
    }

    // ---- 矩阵：体积因子（hw 膨胀 + baseline + 编解码因子） ----

    #[test]
    fn matrix_estimate_factors_snapshot() {
        assert_eq!(hardware_accel_factor(None, Some(30.0)), 1.0);
        assert_eq!(hardware_accel_factor(Some(&HwAccelConfig { device: HwAccelDevice::NVENC, device_index: None }), Some(30.0)), 1.30);
        assert_eq!(hardware_accel_factor(Some(&HwAccelConfig { device: HwAccelDevice::QSV, device_index: None }), Some(30.0)), 1.20);
        assert_eq!(hardware_accel_factor(Some(&HwAccelConfig { device: HwAccelDevice::AMF, device_index: None }), Some(30.0)), 1.35);
        assert_eq!(hardware_accel_factor(Some(&HwAccelConfig { device: HwAccelDevice::VAAPI, device_index: None }), Some(30.0)), 1.25);
        // 高帧率 +0.15，低帧率 -0.05（带 1.0 下限）
        assert_eq!(hardware_accel_factor(Some(&HwAccelConfig { device: HwAccelDevice::NVENC, device_index: None }), Some(60.0)), 1.45);
        assert_eq!(hardware_accel_factor(Some(&HwAccelConfig { device: HwAccelDevice::NVENC, device_index: None }), Some(24.0)), 1.25);
        // baseline 快照
        assert_eq!(crf_baseline(&VideoCodec::H264, None), 18.0);
        assert_eq!(crf_baseline(&VideoCodec::H265, None), 20.0);
        assert_eq!(crf_baseline(&VideoCodec::H265, Some(&HwAccelConfig { device: HwAccelDevice::NVENC, device_index: None })), 22.0);
        assert_eq!(crf_baseline(&VideoCodec::AV1, None), 24.0);
        assert_eq!(crf_baseline(&VideoCodec::AV1, Some(&HwAccelConfig { device: HwAccelDevice::NVENC, device_index: None })), 26.0);
        // 编解码因子快照
        assert_eq!(codec_factor(&VideoCodec::H264), 1.0);
        assert_eq!(codec_factor(&VideoCodec::H265), 0.65);
        assert_eq!(codec_factor(&VideoCodec::AV1), 0.5);
        assert_eq!(source_codec_factor(Some("h264")), 1.0);
        assert_eq!(source_codec_factor(Some("hevc")), 0.65);
        assert_eq!(source_codec_factor(Some("av1")), 0.50);
        assert_eq!(source_codec_factor(None), 1.0);
    }

    // ---- 矩阵：联合（codec × device × rate-control → flags + guard + clamp） ----
    // flags 顺序由 flags 侧快照锁死，此处只锁体积截断，并复用 flags 接口断言输出顺序。

    #[test]
    fn matrix_joint_flags_guard_clamp_snapshot() {
        // NVENC H264 CRF23 编码器健全性（顺序精确 vec 由 flags 侧 joint 快照唯一拥有，此处不重复）。
        let cfg = hw_config(HwAccelDevice::NVENC);
        let encoder = encoder_name(&cfg.video_codec, cfg.hw_accel.as_ref());
        assert_eq!(encoder, "h264_nvenc");
        // 体积侧：同 NVENC CRF10 高画质在 8000k 输入下被 guard 截断到 10000k
        let mut crf10 = hw_config(HwAccelDevice::NVENC);
        crf10.video_settings.rate_control = RateControl::Crf { value: 10 };
        let (exp, _min, max) = estimate_video_kbps(&crf10, 8000.0, &sample_media(1.0));
        assert!(exp <= 10000.0 + 1e-6, "exp {exp} should be clamped to guard 10000");
        assert!(max <= 10000.0 + 1e-6, "max {max} should be clamped to guard 10000");
        // CPU H264 CRF23 无 guard：estimate 无截断
        let cpu = base_config();
        assert_eq!(effective_maxrate_kbps(&cpu, 8000.0), None);
    }
}
