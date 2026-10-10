//! 编码策略矩阵：codec × device × rate-control 全矩阵唯一拥有者。
//!
//! 收敛前策略知识散落在三处且曾多次漂移：
//! - `args.rs` 的 preset 词汇映射 + rate-control 标志映射 + 1.25x 自动 guard；
//! - `codec.rs` 的 `encoder_name` 软件/硬件编码器映射；
//! - `estimate.rs` 的体积预估因子（NVENC 1.3x 等）与同一 1.25x guard 解析。
//!
//! 本模块唯一拥有上述全矩阵，对外接口为“给定用户配置 + 源文件事实，
//! 产出 ffmpeg 视频标志 + guard + 体积模型”，调用方（args / estimate）
//! 一次学会、处处复用，不再各自重述设备 quirks。
//! `codec.rs` 仅保留类型 + 纯工具函数（`ensure_even_dimension` /
//! `parse_fraction_fps`），不收敛类型本身。

use crate::encoder::codec::{EncodeConfig, HwAccelConfig, HwAccelDevice, RateControl, VideoCodec};

/// Auto Maxrate Guard 系数：硬件恒定质量转码时封顶在 1.25x 输入码率。
pub const MAXRATE_GUARD_FACTOR: f64 = 1.25;
/// Auto Maxrate Guard 下限（kbps）：防止低码率输入被压到 0。
pub const MAXRATE_GUARD_FLOOR_KBPS: f64 = 500.0;

// ============================================================
// 编码器映射（原 codec.rs 策略知识，原样搬运）
// ============================================================

/// 映射到 FFmpeg 编码器名称
pub fn encoder_name(codec: &VideoCodec, hw: Option<&HwAccelConfig>) -> &'static str {
    match hw {
        Some(HwAccelConfig { device: HwAccelDevice::NVENC, .. }) => match codec {
            VideoCodec::H264 => "h264_nvenc",
            VideoCodec::H265 => "hevc_nvenc",
            VideoCodec::AV1 => "av1_nvenc",
            _ => software_encoder(codec),
        },
        Some(HwAccelConfig { device: HwAccelDevice::QSV, .. }) => match codec {
            VideoCodec::H264 => "h264_qsv",
            VideoCodec::H265 => "hevc_qsv",
            VideoCodec::AV1 => "av1_qsv",
            _ => software_encoder(codec),
        },
        Some(HwAccelConfig { device: HwAccelDevice::AMF, .. }) => match codec {
            VideoCodec::H264 => "h264_amf",
            VideoCodec::H265 => "hevc_amf",
            VideoCodec::AV1 => "av1_amf",
            _ => software_encoder(codec),
        },
        Some(HwAccelConfig { device: HwAccelDevice::VideoToolbox, .. }) => match codec {
            VideoCodec::H264 => "h264_videotoolbox",
            VideoCodec::H265 => "hevc_videotoolbox",
            VideoCodec::AV1 => "av1_videotoolbox",
            _ => software_encoder(codec),
        },
        Some(HwAccelConfig { device: HwAccelDevice::VAAPI, .. }) => match codec {
            VideoCodec::H264 => "h264_vaapi",
            VideoCodec::H265 => "hevc_vaapi",
            VideoCodec::AV1 => "av1_vaapi",
            _ => software_encoder(codec),
        },
        None => software_encoder(codec),
    }
}

pub fn software_encoder(codec: &VideoCodec) -> &'static str {
    match codec {
        VideoCodec::H264 => "libx264",
        VideoCodec::H265 => "libx265",
        VideoCodec::AV1 => "libsvtav1",
        VideoCodec::VP9 => "libvpx-vp9",
        VideoCodec::Copy => "copy",
    }
}

// ============================================================
// preset 词汇映射（原 args.rs，原样搬运）
// ============================================================

/// 将 x264 风格的命名预设（preset）映射为实际编码器所接受的参数。
///
/// 软件编码器：
/// - libx264 / libx265：直接接受名称（`-preset medium`）。
/// - libsvtav1 (AV1)：仅接受 `-preset <0-13>`（0 最慢/画质最好，13 最快）。
/// - libvpx-vp9 (VP9)：无 `-preset` 选项——使用 `-cpu-used <0-8>`
///   （0 最慢/画质最好，8 最快）。
///
/// 硬件编码器各有自己的预设词汇表：
/// - NVENC：`-preset p1`（最快）..`p7`（最高画质）；传统预设名称依然兼容。
/// - QSV：直接接受 `veryfast..veryslow` 名称。
/// - AMF：`-quality speed|balanced|quality`（`-preset` 为其别名）。
/// - VAAPI：无 `-preset`——使用 `-compression_level`（1 最慢/最高画质 .. 7 最快）。
/// - VideoToolbox：现代 FFmpeg (5.0+) 已完全移除 `-preset`，因此省略该参数。
pub fn encoder_preset_args(config: &EncodeConfig) -> Vec<String> {
    let name = &config.video_settings.encoder_preset;

    match config.hw_accel.as_ref().map(|h| &h.device) {
        Some(HwAccelDevice::NVENC) => {
            // NVENC: p1 最快，p7 最高画质。
            let p = match name.as_str() {
                "ultrafast" | "superfast" => "p1",
                "veryfast" | "faster" => "p2",
                "fast" => "p3",
                "medium" => "p4",
                "slow" => "p5",
                "slower" => "p6",
                "veryslow" => "p7",
                other => other, // p1..p7 或传统预设值直接透传
            };
            vec!["-preset".into(), p.into()]
        }
        Some(HwAccelDevice::QSV) => {
            // QSV 直接接受 veryfast..veryslow 预设名称。
            vec!["-preset".into(), name.clone()]
        }
        Some(HwAccelDevice::AMF) => {
            // AMF 使用 -quality（或别名 -preset）：speed / balanced / quality。
            let q = match name.as_str() {
                "ultrafast" | "superfast" | "veryfast" | "faster" | "fast" => "speed",
                "medium" => "balanced",
                "slow" | "slower" | "veryslow" => "quality",
                other => other, // speed / balanced / quality / high_quality 直接透传
            };
            vec!["-quality".into(), q.into()]
        }
        Some(HwAccelDevice::VAAPI) => {
            // VAAPI: 使用 -compression_level，1 为最慢/最高画质，7 为最快。
            let lvl = match name.as_str() {
                "ultrafast" => "7",
                "superfast" | "veryfast" => "6",
                "faster" | "fast" => "5",
                "medium" => "4",
                "slow" => "3",
                "slower" => "2",
                "veryslow" => "1",
                other => other, // 数字级别直接透传
            };
            vec!["-compression_level".into(), lvl.into()]
        }
        Some(HwAccelDevice::VideoToolbox) => {
            // VideoToolbox 在 FFmpeg 5.0 中移除了 -preset；完全省略该参数。
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
                    _ => 8, // SVT-AV1 默认预设
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
                    _ => 1, // libvpx-vp9 默认 cpu-used
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

// ============================================================
// rate-control 标志映射（原 args.rs，原样搬运）
// ============================================================

/// 根据目标编码器映射码率控制配置。
///
/// 硬件及特定软件编码器的码率控制 CLI 选项各不相同：
/// - NVENC：不支持 -crf；恒定质量使用 `-rc:v vbr -cq <val>`，CQP 使用 `-rc:v constqp -qp <val>`。
/// - QSV：恒定质量使用 `-global_quality <val>`，CQP 使用 `-q:v <val>`。
/// - AMF：CQP 使用 `-rc cqp -qp_i <val> -qp_p <val>`。
/// - VAAPI：`-qp <val>`。
/// - VideoToolbox：`-q:v <val>`。
/// - libvpx-vp9：恒定质量需要 `-crf <val> -b:v 0`。
/// - libsvtav1：不支持 -qp；使用 `-crf <val>`。
/// - libx264 / libx265：标准的 `-crf <val>` 或 `-qp <val>`。
pub fn rate_control_args(config: &EncodeConfig, encoder: &str) -> Vec<String> {
    match &config.video_settings.rate_control {
        RateControl::Crf { value } => {
            if encoder.contains("nvenc") {
                // NVENC VBR 恒定质量模式（所见即所得：UI 设定值直接传入 -cq）：
                // 1. 必须附加 `-b:v 0`，解除默认 target bitrate 约束，由 -cq 决定质量；
                // 2. 启用 `-spatial-aq 1`（空间自适应量化）与 `-rc-lookahead 32`（前瞻分析）以显著提升压缩效率。
                vec![
                    "-rc:v".into(), "vbr".into(),
                    "-cq".into(), value.to_string(),
                    "-b:v".into(), "0".into(),
                    "-spatial-aq".into(), "1".into(),
                    "-rc-lookahead".into(), "32".into(),
                ]
            } else if encoder.contains("qsv") {
                vec!["-global_quality".into(), value.to_string()]
            } else if encoder.contains("amf") {
                vec!["-rc".into(), "cqp".into(), "-qp_i".into(), value.to_string(), "-qp_p".into(), value.to_string()]
            } else if encoder.contains("vaapi") {
                vec!["-qp".into(), value.to_string()]
            } else if encoder.contains("videotoolbox") {
                vec!["-q:v".into(), value.to_string()]
            } else if encoder == "libvpx-vp9" {
                vec!["-crf".into(), value.to_string(), "-b:v".into(), "0".into()]
            } else {
                vec!["-crf".into(), value.to_string()]
            }
        }
        RateControl::Cqp { value } => {
            if encoder.contains("nvenc") {
                vec![
                    "-rc:v".into(), "constqp".into(),
                    "-qp".into(), value.to_string(),
                    "-spatial-aq".into(), "1".into(),
                ]
            } else if encoder.contains("qsv") {
                vec!["-q:v".into(), value.to_string()]
            } else if encoder.contains("amf") {
                vec!["-rc".into(), "cqp".into(), "-qp_i".into(), value.to_string(), "-qp_p".into(), value.to_string()]
            } else if encoder.contains("vaapi") {
                vec!["-qp".into(), value.to_string()]
            } else if encoder.contains("videotoolbox") {
                vec!["-q:v".into(), value.to_string()]
            } else if encoder == "libsvtav1" {
                vec!["-crf".into(), value.to_string()]
            } else if encoder == "libvpx-vp9" {
                vec!["-crf".into(), value.to_string(), "-b:v".into(), "0".into()]
            } else {
                vec!["-qp".into(), value.to_string()]
            }
        }
        RateControl::Abr {
            bitrate_kbps,
            max_bitrate_kbps,
        } => {
            let mut args = vec!["-b:v".into(), format!("{}k", bitrate_kbps)];
            if let Some(max) = max_bitrate_kbps {
                args.push("-maxrate".into());
                args.push(format!("{}k", max));
                args.push("-bufsize".into());
                args.push(format!("{}k", max * 2));
            }
            args
        }
    }
}

// ============================================================
// maxrate guard（args 与 estimate 共用的 1.25x 规则，原样收敛）
// ============================================================

/// 是否应对当前配置启用 Auto Maxrate Guard（硬件 + 非 ABR）。
pub fn should_apply_auto_maxrate_guard(config: &EncodeConfig) -> bool {
    config.hw_accel.is_some()
        && !matches!(config.video_settings.rate_control, RateControl::Abr { .. })
}

/// 用户是否在 additional_params 中显式指定了 -maxrate（此时不自动注入）。
pub fn has_custom_maxrate(config: &EncodeConfig) -> bool {
    config
        .video_settings
        .additional_params
        .iter()
        .any(|a| a == "-maxrate")
}

/// 1.25x guard 换算（kbps），带 500k 下限。
pub fn guard_maxrate_kbps(input_kbps: f64) -> f64 {
    (input_kbps * MAXRATE_GUARD_FACTOR).round().max(MAXRATE_GUARD_FLOOR_KBPS)
}

/// 解析用户显式指定的 -maxrate 值（kbps）：支持 `6000k` / `5M` / 裸数字（bps→kbps）。
/// 解析失败返回 None（调用方回退到自动 guard）。
pub fn parse_custom_maxrate_kbps(params: &[String]) -> Option<f64> {
    let pos = params.iter().position(|a| a == "-maxrate")?;
    let val_str = params.get(pos + 1)?;
    let s = val_str.trim().to_lowercase();
    if let Some(m) = s.strip_suffix('m') {
        m.parse::<f64>().ok().map(|n| n * 1000.0)
    } else if let Some(k) = s.strip_suffix('k') {
        k.parse::<f64>().ok()
    } else {
        s.parse::<f64>().ok().map(|n| n / 1000.0)
    }
}

/// 获取当前转码任务的有效 -maxrate 限制（Auto Maxrate Guard 或用户自定义 -maxrate），用于封顶截断。
/// 与搬运前逐行一致：自定义值可解析则优先；不可解析/缺值时穿透到自动 guard。
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

/// args 侧的自动 guard 标志（`-maxrate .. -bufsize ..`），供 `build_ffmpeg_args` 直接扩展。
/// 行为与搬运前完全一致：CPU / ABR / 已有自定义 -maxrate / 无输入码率时返回空。
pub fn auto_maxrate_guard_args(
    config: &EncodeConfig,
    input_bitrate_kbps: Option<u32>,
) -> Vec<String> {
    if !should_apply_auto_maxrate_guard(config) {
        return vec![];
    }
    if has_custom_maxrate(config) {
        return vec![];
    }
    if let Some(in_kbps) = input_bitrate_kbps.filter(|&k| k > 0) {
        let guard_maxrate = guard_maxrate_kbps(in_kbps as f64) as u32;
        let guard_bufsize = guard_maxrate.saturating_mul(2);
        return vec![
            "-maxrate".into(),
            format!("{}k", guard_maxrate),
            "-bufsize".into(),
            format!("{}k", guard_bufsize),
        ];
    }
    vec![]
}

/// 组合入口：一次学会——preset + rate-control + guard 按固定顺序产出，
/// 与搬运前 `build_ffmpeg_args` 内的扩展顺序逐 flag 一致。
pub fn video_encoding_args(
    config: &EncodeConfig,
    encoder: &str,
    input_bitrate_kbps: Option<u32>,
) -> Vec<String> {
    let mut args = encoder_preset_args(config);
    args.extend(rate_control_args(config, encoder));
    args.extend(auto_maxrate_guard_args(config, input_bitrate_kbps));
    args
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

    // ---- 矩阵：encoder_name（codec × device 快照） ----

    #[test]
    fn matrix_encoder_name_snapshot() {
        // CPU
        assert_eq!(encoder_name(&VideoCodec::H264, None), "libx264");
        assert_eq!(encoder_name(&VideoCodec::H265, None), "libx265");
        assert_eq!(encoder_name(&VideoCodec::AV1, None), "libsvtav1");
        assert_eq!(encoder_name(&VideoCodec::VP9, None), "libvpx-vp9");
        assert_eq!(encoder_name(&VideoCodec::Copy, None), "copy");
        // NVENC / QSV / AMF / VAAPI / VTB：VP9 与 Copy 永远回退软件实现
        for device in [HwAccelDevice::NVENC, HwAccelDevice::QSV, HwAccelDevice::AMF, HwAccelDevice::VAAPI, HwAccelDevice::VideoToolbox] {
            let hw = HwAccelConfig { device, device_index: None };
            let hw_ref = Some(&hw);
            assert_eq!(encoder_name(&VideoCodec::VP9, hw_ref), "libvpx-vp9");
            assert_eq!(encoder_name(&VideoCodec::Copy, hw_ref), "copy");
        }
        let nvenc = HwAccelConfig { device: HwAccelDevice::NVENC, device_index: None };
        assert_eq!(encoder_name(&VideoCodec::H264, Some(&nvenc)), "h264_nvenc");
        assert_eq!(encoder_name(&VideoCodec::H265, Some(&nvenc)), "hevc_nvenc");
        assert_eq!(encoder_name(&VideoCodec::AV1, Some(&nvenc)), "av1_nvenc");
        let qsv = HwAccelConfig { device: HwAccelDevice::QSV, device_index: None };
        assert_eq!(encoder_name(&VideoCodec::H264, Some(&qsv)), "h264_qsv");
        assert_eq!(encoder_name(&VideoCodec::H265, Some(&qsv)), "hevc_qsv");
        assert_eq!(encoder_name(&VideoCodec::AV1, Some(&qsv)), "av1_qsv");
        let amf = HwAccelConfig { device: HwAccelDevice::AMF, device_index: None };
        assert_eq!(encoder_name(&VideoCodec::H264, Some(&amf)), "h264_amf");
        assert_eq!(encoder_name(&VideoCodec::H265, Some(&amf)), "hevc_amf");
        assert_eq!(encoder_name(&VideoCodec::AV1, Some(&amf)), "av1_amf");
        let vaapi = HwAccelConfig { device: HwAccelDevice::VAAPI, device_index: None };
        assert_eq!(encoder_name(&VideoCodec::H264, Some(&vaapi)), "h264_vaapi");
        assert_eq!(encoder_name(&VideoCodec::AV1, Some(&vaapi)), "av1_vaapi");
        let vtb = HwAccelConfig { device: HwAccelDevice::VideoToolbox, device_index: None };
        assert_eq!(encoder_name(&VideoCodec::H264, Some(&vtb)), "h264_videotoolbox");
        assert_eq!(encoder_name(&VideoCodec::H265, Some(&vtb)), "hevc_videotoolbox");
        assert_eq!(encoder_name(&VideoCodec::AV1, Some(&vtb)), "av1_videotoolbox");
    }

    // ---- 矩阵：preset 词汇（device × preset 快照） ----

    #[test]
    fn matrix_preset_vocab_snapshot() {
        // 软件：H264 透传、AV1 数值、VP9 cpu-used
        assert_eq!(encoder_preset_args(&base_config()), vec!["-preset".to_string(), "medium".to_string()]);
        let mut av1 = base_config();
        av1.video_codec = VideoCodec::AV1;
        assert_eq!(encoder_preset_args(&av1), vec!["-preset".to_string(), "6".to_string()]);
        let mut vp9 = base_config();
        vp9.video_codec = VideoCodec::VP9;
        assert_eq!(encoder_preset_args(&vp9), vec!["-cpu-used".to_string(), "3".to_string()]);
        // 硬件 medium 快照
        assert_eq!(encoder_preset_args(&hw_config(HwAccelDevice::NVENC)), vec!["-preset".to_string(), "p4".to_string()]);
        assert_eq!(encoder_preset_args(&hw_config(HwAccelDevice::QSV)), vec!["-preset".to_string(), "medium".to_string()]);
        assert_eq!(encoder_preset_args(&hw_config(HwAccelDevice::AMF)), vec!["-quality".to_string(), "balanced".to_string()]);
        assert_eq!(encoder_preset_args(&hw_config(HwAccelDevice::VAAPI)), vec!["-compression_level".to_string(), "4".to_string()]);
        assert_eq!(encoder_preset_args(&hw_config(HwAccelDevice::VideoToolbox)), Vec::<String>::new());
        // 边界词汇快照（与搬运前逐 flag 一致）
        let mut nvenc_fast = hw_config(HwAccelDevice::NVENC);
        nvenc_fast.video_settings.encoder_preset = "ultrafast".into();
        assert_eq!(encoder_preset_args(&nvenc_fast)[1], "p1");
        let mut nvenc_slow = hw_config(HwAccelDevice::NVENC);
        nvenc_slow.video_settings.encoder_preset = "veryslow".into();
        assert_eq!(encoder_preset_args(&nvenc_slow)[1], "p7");
        let mut amf_fast = hw_config(HwAccelDevice::AMF);
        amf_fast.video_settings.encoder_preset = "fast".into();
        assert_eq!(encoder_preset_args(&amf_fast)[1], "speed");
        let mut vaapi_fast = hw_config(HwAccelDevice::VAAPI);
        vaapi_fast.video_settings.encoder_preset = "ultrafast".into();
        assert_eq!(encoder_preset_args(&vaapi_fast)[1], "7");
        let mut cpu_av1_slow = base_config();
        cpu_av1_slow.video_codec = VideoCodec::AV1;
        cpu_av1_slow.video_settings.encoder_preset = "veryslow".into();
        assert_eq!(encoder_preset_args(&cpu_av1_slow)[1], "1");
        let mut cpu_vp9_fast = base_config();
        cpu_vp9_fast.video_codec = VideoCodec::VP9;
        cpu_vp9_fast.video_settings.encoder_preset = "ultrafast".into();
        assert_eq!(encoder_preset_args(&cpu_vp9_fast)[1], "8");
    }

    // ---- 矩阵：rate-control 标志（encoder × rate-control 快照） ----

    #[test]
    fn matrix_rate_control_flags_snapshot() {
        let cfg = base_config(); // CRF 23
        assert_eq!(rate_control_args(&cfg, "libx264"), vec!["-crf", "23"]);
        assert_eq!(
            rate_control_args(&cfg, "h264_nvenc"),
            vec!["-rc:v", "vbr", "-cq", "23", "-b:v", "0", "-spatial-aq", "1", "-rc-lookahead", "32"]
        );
        assert_eq!(rate_control_args(&cfg, "h264_qsv"), vec!["-global_quality", "23"]);
        assert_eq!(
            rate_control_args(&cfg, "h264_amf"),
            vec!["-rc", "cqp", "-qp_i", "23", "-qp_p", "23"]
        );
        assert_eq!(rate_control_args(&cfg, "h264_vaapi"), vec!["-qp", "23"]);
        assert_eq!(rate_control_args(&cfg, "h264_videotoolbox"), vec!["-q:v", "23"]);
        assert_eq!(rate_control_args(&cfg, "libvpx-vp9"), vec!["-crf", "23", "-b:v", "0"]);

        let mut cqp = base_config();
        cqp.video_settings.rate_control = RateControl::Cqp { value: 23 };
        assert_eq!(
            rate_control_args(&cqp, "h264_nvenc"),
            vec!["-rc:v", "constqp", "-qp", "23", "-spatial-aq", "1"]
        );
        assert_eq!(rate_control_args(&cqp, "h264_qsv"), vec!["-q:v", "23"]);
        assert_eq!(rate_control_args(&cqp, "libsvtav1"), vec!["-crf", "23"]);
        assert_eq!(rate_control_args(&cqp, "libvpx-vp9"), vec!["-crf", "23", "-b:v", "0"]);
        assert_eq!(rate_control_args(&cqp, "libx264"), vec!["-qp", "23"]);

        let mut abr = base_config();
        abr.video_settings.rate_control = RateControl::Abr { bitrate_kbps: 4000, max_bitrate_kbps: Some(6000) };
        assert_eq!(
            rate_control_args(&abr, "libx264"),
            vec!["-b:v", "4000k", "-maxrate", "6000k", "-bufsize", "12000k"]
        );
        abr.video_settings.rate_control = RateControl::Abr { bitrate_kbps: 4000, max_bitrate_kbps: None };
        assert_eq!(rate_control_args(&abr, "libx264"), vec!["-b:v", "4000k"]);
    }

    // ---- 矩阵：guard（1.25x + 自定义解析 + 生效条件） ----

    #[test]
    fn matrix_maxrate_guard_snapshot() {
        // 换算：4000 → 5000k，下限 500k
        assert_eq!(guard_maxrate_kbps(4000.0), 5000.0);
        assert_eq!(guard_maxrate_kbps(100.0), 500.0);
        // 自定义解析：k / M / 裸数字
        assert_eq!(parse_custom_maxrate_kbps(&["-maxrate".into(), "6000k".into()]), Some(6000.0));
        assert_eq!(parse_custom_maxrate_kbps(&["-maxrate".into(), "5M".into()]), Some(5000.0));
        assert_eq!(parse_custom_maxrate_kbps(&["-maxrate".into(), "3000k".into()]), Some(3000.0));
        assert_eq!(parse_custom_maxrate_kbps(&[]), None);
        // 生效条件：CPU 永不 guard，ABR 永不 guard
        assert!(!should_apply_auto_maxrate_guard(&base_config()));
        let mut abr_hw = hw_config(HwAccelDevice::NVENC);
        abr_hw.video_settings.rate_control = RateControl::Abr { bitrate_kbps: 4000, max_bitrate_kbps: None };
        assert!(!should_apply_auto_maxrate_guard(&abr_hw));
        assert!(should_apply_auto_maxrate_guard(&hw_config(HwAccelDevice::NVENC)));
        // 有效 guard：NVENC CRF 4000 → 5000；自定义 3000k 优先；CPU → None
        assert_eq!(effective_maxrate_kbps(&hw_config(HwAccelDevice::NVENC), 4000.0), Some(5000.0));
        let mut custom = hw_config(HwAccelDevice::NVENC);
        custom.video_settings.additional_params = vec!["-maxrate".into(), "3000k".into()];
        assert_eq!(effective_maxrate_kbps(&custom, 4000.0), Some(3000.0));
        assert_eq!(effective_maxrate_kbps(&base_config(), 4000.0), None);
        // args 侧 guard 标志快照
        assert_eq!(
            auto_maxrate_guard_args(&hw_config(HwAccelDevice::NVENC), Some(4000)),
            vec!["-maxrate", "5000k", "-bufsize", "10000k"]
        );
        assert!(auto_maxrate_guard_args(&base_config(), Some(4000)).is_empty());
        assert!(auto_maxrate_guard_args(&hw_config(HwAccelDevice::NVENC), Some(0)).is_empty());
        assert_eq!(
            auto_maxrate_guard_args(&custom, Some(4000)),
            Vec::<String>::new()
        );
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

    #[test]
    fn matrix_joint_flags_guard_clamp_snapshot() {
        // NVENC H264 CRF23 + 输入 4000k：preset p4 + vbr/cq + 自动 guard 5000k，顺序锁死
        let cfg = hw_config(HwAccelDevice::NVENC);
        let encoder = encoder_name(&cfg.video_codec, cfg.hw_accel.as_ref());
        assert_eq!(encoder, "h264_nvenc");
        assert_eq!(
            video_encoding_args(&cfg, encoder, Some(4000)),
            vec![
                "-preset", "p4",
                "-rc:v", "vbr", "-cq", "23", "-b:v", "0", "-spatial-aq", "1", "-rc-lookahead", "32",
                "-maxrate", "5000k", "-bufsize", "10000k",
            ]
        );
        // 同配置 ABR：无 guard，只有 -b:v
        let mut abr = hw_config(HwAccelDevice::NVENC);
        abr.video_settings.rate_control = RateControl::Abr { bitrate_kbps: 2000, max_bitrate_kbps: None };
        let encoder = encoder_name(&abr.video_codec, abr.hw_accel.as_ref());
        assert_eq!(
            video_encoding_args(&abr, encoder, Some(4000)),
            vec!["-preset", "p4", "-b:v", "2000k"]
        );
        // 体积侧：同 NVENC CRF10 高画质在 8000k 输入下被 guard 截断到 10000k
        let mut crf10 = hw_config(HwAccelDevice::NVENC);
        crf10.video_settings.rate_control = RateControl::Crf { value: 10 };
        let (exp, _min, max) = estimate_video_kbps(&crf10, 8000.0, &sample_media(1.0));
        assert!(exp <= 10000.0 + 1e-6, "exp {exp} should be clamped to guard 10000");
        assert!(max <= 10000.0 + 1e-6, "max {max} should be clamped to guard 10000");
        // CPU H264 CRF23 无 guard：video flags 无 maxrate，estimate 无截断
        let cpu = base_config();
        let encoder = encoder_name(&cpu.video_codec, cpu.hw_accel.as_ref());
        let flags = video_encoding_args(&cpu, encoder, Some(4000));
        assert!(!flags.contains(&"-maxrate".to_string()), "{flags:?}");
        assert_eq!(effective_maxrate_kbps(&cpu, 8000.0), None);
    }
}
