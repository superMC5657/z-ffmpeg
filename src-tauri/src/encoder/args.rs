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
}
