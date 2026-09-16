//! FFmpeg 参数与输出路径构建：从 `EncodeConfig` 推导 ffmpeg CLI 参数、
//! 编码器 preset 映射，以及输入 → 输出路径的推导与批量去重。

use std::collections::HashMap;

use crate::encoder::codec::{EncodeConfig, VideoCodec};

/// Build the ffmpeg command arguments from config
pub fn build_ffmpeg_args(
    config: &EncodeConfig,
    input_path: &str,
    output_path: &str,
) -> Vec<String> {
    build_ffmpeg_args_with_bitrate(config, input_path, output_path, None)
}

/// Build the ffmpeg command arguments from config with optional input bitrate safety guard
pub fn build_ffmpeg_args_with_bitrate(
    config: &EncodeConfig,
    input_path: &str,
    output_path: &str,
    input_bitrate_kbps: Option<u32>,
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

        // Rate control (mapped to encoder capabilities)
        args.extend(rate_control_args(config, encoder));

        // Auto Maxrate Guard:
        // 当使用硬件加速（如 NVENC/AMF/QSV/VAAPI）恒定画质（CRF/CQP）转码时，
        // 面对高帧率(如 60fps)、复杂噪点或高动态源视频，硬件芯片在恒定质量量化下
        // 容易分配极大码率，导致转码后文件体积反超原片数倍。
        // 若输入码率已知且用户未在 additionalParams 中手动指定 -maxrate，
        // 自动将最大峰值码率安全限制在 1.25x 输入码率（辅以 2x bufsize），彻底杜绝负压缩。
        if config.hw_accel.is_some()
            && !matches!(config.video_settings.rate_control, crate::encoder::codec::RateControl::Abr { .. })
        {
            let has_custom_maxrate = config
                .video_settings
                .additional_params
                .iter()
                .any(|a| a == "-maxrate");
            if !has_custom_maxrate {
                if let Some(in_kbps) = input_bitrate_kbps.filter(|&k| k > 0) {
                    let guard_maxrate = ((in_kbps as f64) * 1.25).round().max(500.0) as u32;
                    let guard_bufsize = guard_maxrate.saturating_mul(2);
                    args.push("-maxrate".into());
                    args.push(format!("{}k", guard_maxrate));
                    args.push("-bufsize".into());
                    args.push(format!("{}k", guard_bufsize));
                }
            }
        }

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

        // Resolution scaling (force even dimensions for chroma subsampling compatibility)
        if let Some(ref res) = config.video_settings.resolution {
            let mut w = res.width.max(2);
            let mut h = res.height.max(2);
            if w % 2 == 1 { w = w.saturating_sub(1).max(2); }
            if h % 2 == 1 { h = h.saturating_sub(1).max(2); }
            args.push("-vf".into());
            args.push(format!("scale={}:{}", w, h));
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
    use crate::encoder::codec::HwAccelDevice;

    let name = &config.video_settings.encoder_preset;

    match config.hw_accel.as_ref().map(|h| &h.device) {
        Some(HwAccelDevice::NVENC) => {
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
        Some(HwAccelDevice::QSV) => {
            // QSV accepts the veryfast..veryslow names directly.
            vec!["-preset".into(), name.clone()]
        }
        Some(HwAccelDevice::AMF) => {
            // AMF uses -quality (or the synonym -preset): speed / balanced / quality.
            let q = match name.as_str() {
                "ultrafast" | "superfast" | "veryfast" | "faster" | "fast" => "speed",
                "medium" => "balanced",
                "slow" | "slower" | "veryslow" => "quality",
                other => other, // speed / balanced / quality / high_quality pass through
            };
            vec!["-quality".into(), q.into()]
        }
        Some(HwAccelDevice::VAAPI) => {
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
        Some(HwAccelDevice::VideoToolbox) => {
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

/// Map rate control configuration according to the target encoder.
///
/// Hardware and specialty software encoders have divergent rate control CLI options:
/// - NVENC: does not support -crf; constant quality uses `-rc:v vbr -cq <val>`, CQP uses `-rc:v constqp -qp <val>`.
/// - QSV: constant quality uses `-global_quality <val>`, CQP uses `-q:v <val>`.
/// - AMF: CQP uses `-rc cqp -qp_i <val> -qp_p <val>`.
/// - VAAPI: `-qp <val>`.
/// - VideoToolbox: `-q:v <val>`.
/// - libvpx-vp9: constant quality requires `-crf <val> -b:v 0`.
/// - libsvtav1: does not support -qp; uses `-crf <val>`.
/// - libx264 / libx265: standard `-crf <val>` or `-qp <val>`.
fn rate_control_args(config: &EncodeConfig, encoder: &str) -> Vec<String> {
    use crate::encoder::codec::RateControl;

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

/// Build output path from input path + config (shared by queue and command preview)
pub fn derive_output_path(input: &str, config: &EncodeConfig, output_dir: Option<&str>) -> String {
    let path = std::path::Path::new(input);
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();

    let parent = match output_dir {
        Some(dir) if !dir.trim().is_empty() => std::path::Path::new(dir).to_path_buf(),
        _ => path.parent().unwrap_or(std::path::Path::new(".")).to_path_buf(),
    };

    let ext = config.container_format.extension();

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

/// Format an array of ffmpeg arguments into a single display-ready shell command line.
/// Arguments containing spaces, quotes, or empty strings are safely quoted.
pub fn format_command_line(args: &[String]) -> String {
    let mut parts = vec!["ffmpeg".to_string()];
    for arg in args {
        if arg.contains(' ') || arg.contains('"') || arg.is_empty() {
            parts.push(format!("\"{}\"", arg.replace('"', "\\\"")));
        } else {
            parts.push(arg.clone());
        }
    }
    parts.join(" ")
}

/// Build a display-ready ffmpeg command line (`ffmpeg <args...>`) from a config.
/// Paths containing spaces or quotes are quoted so the command can be copied
/// and pasted into a terminal directly.
pub fn build_ffmpeg_command_line(
    config: &EncodeConfig,
    input_path: &str,
    output_path: &str,
) -> String {
    let args = build_ffmpeg_args(config, input_path, output_path);
    format_command_line(&args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoder::codec::{
        AudioCodec, EncodeConfig, HwAccelConfig, HwAccelDevice, RateControl, Resolution,
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
        assert_args_contain(&args, &["-c:v", "h264_nvenc", "-preset", "p4", "-rc:v", "vbr", "-cq", "23", "-b:v", "0", "-spatial-aq", "1"]);
    }

    #[test]
    fn build_args_nvenc_wysiwyg_cq() {
        let mut hevc = config_with_hw(Some(HwAccelConfig {
            device: HwAccelDevice::NVENC,
            device_index: None,
        }));
        hevc.video_codec = VideoCodec::H265;
        let args = build_ffmpeg_args(&hevc, "in.mp4", "out.mp4");
        // 所见即所得：UI 设置 23，底层直接传递 -cq 23，不做黑盒偏移
        assert_args_contain(&args, &["-c:v", "hevc_nvenc", "-preset", "p4", "-rc:v", "vbr", "-cq", "23", "-b:v", "0", "-spatial-aq", "1"]);

        let mut av1 = config_with_hw(Some(HwAccelConfig {
            device: HwAccelDevice::NVENC,
            device_index: None,
        }));
        av1.video_codec = VideoCodec::AV1;
        let args_av1 = build_ffmpeg_args(&av1, "in.mp4", "out.mp4");
        // 所见即所得：UI 设置 23，底层直接传递 -cq 23
        assert_args_contain(&args_av1, &["-c:v", "av1_nvenc", "-preset", "p4", "-rc:v", "vbr", "-cq", "23", "-b:v", "0", "-spatial-aq", "1"]);
    }

    #[test]
    fn build_args_qsv_keeps_preset_name() {
        let config = config_with_hw(Some(HwAccelConfig {
            device: HwAccelDevice::QSV,
            device_index: None,
        }));
        let args = build_ffmpeg_args(&config, "in.mp4", "out.mp4");
        assert_args_contain(&args, &["-c:v", "h264_qsv", "-preset", "medium", "-global_quality", "23"]);
    }

    #[test]
    fn build_args_amf_maps_preset_to_quality() {
        let config = config_with_hw(Some(HwAccelConfig {
            device: HwAccelDevice::AMF,
            device_index: None,
        }));
        let args = build_ffmpeg_args(&config, "in.mp4", "out.mp4");
        assert_args_contain(&args, &["-c:v", "h264_amf", "-quality", "balanced", "-rc", "cqp", "-qp_i", "23", "-qp_p", "23"]);
    }

    #[test]
    fn build_args_av1_software_maps_preset_to_number() {
        let mut config = sample_config();
        config.video_codec = VideoCodec::AV1;
        let args = build_ffmpeg_args(&config, "in.mp4", "out.mp4");
        // medium → 6
        assert_args_contain(&args, &["-c:v", "libsvtav1", "-preset", "6", "-crf", "23"]);
    }

    #[test]
    fn build_args_vp9_software_uses_cpu_used() {
        let mut config = sample_config();
        config.video_codec = VideoCodec::VP9;
        let args = build_ffmpeg_args(&config, "in.mp4", "out.mp4");
        // medium → 3, VP9 requires -crf 23 -b:v 0
        assert_args_contain(&args, &["-c:v", "libvpx-vp9", "-cpu-used", "3", "-crf", "23", "-b:v", "0"]);
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
        copy.audio_settings.codec = AudioCodec::Copy;
        let args = build_ffmpeg_args(&copy, "in.mp4", "out.mp4");
        assert_args_contain(&args, &["-c:a", "copy"]);

        let mut none = sample_config();
        none.audio_settings.codec = AudioCodec::None;
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
    fn build_args_auto_maxrate_guard_nvenc() {
        let config = config_with_hw(Some(HwAccelConfig {
            device: HwAccelDevice::NVENC,
            device_index: None,
        }));
        // Input bitrate 4000 kbps -> maxrate = 4000 * 1.25 = 5000k, bufsize = 10000k
        let args = build_ffmpeg_args_with_bitrate(&config, "in.mp4", "out.mp4", Some(4000));
        assert_args_contain(&args, &["-maxrate", "5000k", "-bufsize", "10000k"]);

        // When CPU encoding (hw is None), auto maxrate guard should NOT be injected
        let cpu_args = build_ffmpeg_args_with_bitrate(&sample_config(), "in.mp4", "out.mp4", Some(4000));
        assert!(!cpu_args.contains(&"-maxrate".to_string()));

        // When user explicitly supplied -maxrate in additional_params, do not override
        let mut custom = config;
        custom.video_settings.additional_params = vec!["-maxrate".into(), "3000k".into()];
        let custom_args = build_ffmpeg_args_with_bitrate(&custom, "in.mp4", "out.mp4", Some(4000));
        let pos = custom_args.iter().position(|a| a == "-maxrate").unwrap();
        assert_eq!(custom_args[pos + 1], "3000k");
    }
}
