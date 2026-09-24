//! FFmpeg 参数与输出路径构建：从 `EncodeConfig` 推导 ffmpeg CLI 参数、
//! 编码器 preset 映射，以及输入 → 输出路径的推导与批量去重。

use std::collections::HashSet;

use crate::encoder::codec::{EncodeConfig, VideoCodec};

/// 从配置构建 ffmpeg 命令行参数
pub fn build_ffmpeg_args(
    config: &EncodeConfig,
    input_path: &str,
    output_path: &str,
) -> Vec<String> {
    build_ffmpeg_args_with_bitrate(config, input_path, output_path, None)
}

/// 从配置构建 ffmpeg 命令行参数，并附带可选的输入码率安全防护
pub fn build_ffmpeg_args_with_bitrate(
    config: &EncodeConfig,
    input_path: &str,
    output_path: &str,
    input_bitrate_kbps: Option<u32>,
) -> Vec<String> {
    let mut args: Vec<String> = vec![];

    // 输入文件
    args.push("-y".into()); // 覆盖输出文件
    args.push("-i".into());
    args.push(input_path.into());

    // 视频编码器
    let encoder = config.video_codec.encoder_name(config.hw_accel.as_ref());
    if config.video_codec != VideoCodec::Copy {
        args.push("-c:v".into());
        args.push(encoder.into());

        // 编码器预设 preset（具体取值取决于具体编码器——参见 encoder_preset_args）
        args.extend(encoder_preset_args(config));

        // 码率控制（根据编码器特性映射）
        args.extend(rate_control_args(config, encoder));

        // 自动最大码率保护（Auto Maxrate Guard）:
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

        // Profile 配置
        if let Some(ref profile) = config.video_settings.profile {
            args.push("-profile:v".into());
            args.push(profile.clone());
        }

        // 像素格式
        if let Some(ref pix_fmt) = config.video_settings.pixel_format {
            args.push("-pix_fmt".into());
            args.push(pix_fmt.clone());
        }

        // 分辨率缩放（强制偶数尺寸以满足色度抽样兼容性要求）
        if let Some(ref res) = config.video_settings.resolution {
            let mut w = res.width.max(2);
            let mut h = res.height.max(2);
            if w % 2 == 1 { w = w.saturating_sub(1).max(2); }
            if h % 2 == 1 { h = h.saturating_sub(1).max(2); }
            args.push("-vf".into());
            args.push(format!("scale={}:{}", w, h));
        }

        // 帧率设置
        if let Some(fps) = config.video_settings.frame_rate {
            args.push("-r".into());
            args.push(fps.to_string());
        }
    } else {
        args.push("-c:v".into());
        args.push("copy".into());
    }

    // 音频设置
    args.extend(config.audio_settings.to_args());

    // 附加自定义参数
    args.extend(config.video_settings.additional_params.clone());

    // 输出路径
    args.push(output_path.into());

    args
}

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
fn encoder_preset_args(config: &EncodeConfig) -> Vec<String> {
    use crate::encoder::codec::HwAccelDevice;

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

/// 根据输入路径 + 配置构建输出路径（供队列和命令预览共用）
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

/// 辅助函数：标准化路径字符串以便在内存集合中进行键匹配（Windows 下不区分大小写）
fn normalize_path_key(path: &str) -> String {
    #[cfg(windows)]
    {
        path.replace('/', "\\").to_lowercase()
    }
    #[cfg(not(windows))]
    {
        path.to_string()
    }
}

/// 为批量输入文件推导互不冲突的唯一输出路径。
///
/// 确保输出路径避免与以下项发生碰撞：
/// 1. 磁盘上已存在的文件
/// 2. 队列中正在运行或等待的任务（通过 `already_claimed`）
/// 3. 同一批次中较早的输入文件
///
/// 如果与 `{stem}_encoded.{ext}` 发生重名冲突，会在扩展名前递增追加
/// 数字后缀（`_1`、`_2`、`_3`...），直到找到未被占用的文件名。
#[allow(dead_code)]
pub fn derive_output_paths_unique(
    inputs: &[String],
    config: &EncodeConfig,
    output_dir: Option<&str>,
) -> Vec<String> {
    derive_output_paths_unique_with_claimed(inputs, config, output_dir, &[])
}

/// 为批量输入推导唯一输出路径，并附带已占用的显式路径集合
/// （例如当前活跃/排队的任务路径）。
pub fn derive_output_paths_unique_with_claimed(
    inputs: &[String],
    config: &EncodeConfig,
    output_dir: Option<&str>,
    already_claimed: &[String],
) -> Vec<String> {
    let mut claimed: HashSet<String> = already_claimed
        .iter()
        .map(|p| normalize_path_key(p))
        .collect();

    inputs
        .iter()
        .map(|f| {
            let base = derive_output_path(f, config, output_dir);
            let base_key = normalize_path_key(&base);

            // 若 base 文件在磁盘上不存在且尚未被占用，直接使用 base
            if !std::path::Path::new(&base).exists() && !claimed.contains(&base_key) {
                claimed.insert(base_key);
                return base;
            }

            let p = std::path::Path::new(&base);
            let stem = p.file_stem().unwrap_or_default().to_string_lossy();
            let ext = p.extension().unwrap_or_default().to_string_lossy();
            let parent = p.parent().unwrap_or(std::path::Path::new(""));

            let mut n = 1;
            loop {
                let candidate_name = if ext.is_empty() {
                    format!("{}_{}", stem, n)
                } else {
                    format!("{}_{}.{}", stem, n, ext)
                };
                let candidate = if parent.as_os_str().is_empty() {
                    std::path::PathBuf::from(candidate_name)
                } else {
                    parent.join(candidate_name)
                }
                .to_string_lossy()
                .to_string();

                let candidate_key = normalize_path_key(&candidate);

                if !std::path::Path::new(&candidate).exists() && !claimed.contains(&candidate_key) {
                    claimed.insert(candidate_key);
                    return candidate;
                }
                n += 1;
            }
        })
        .collect()
}

/// 将 ffmpeg 参数数组格式化为可直接在终端展示与运行的 shell 命令行。
/// 包含空格、引号或空字符串的参数会被安全加上双引号包裹。
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

/// 根据配置构建可直接展示与运行的 ffmpeg 命令行（`ffmpeg <args...>`）。
/// 包含空格或引号的路径会被加上双引号，以便直接复制粘贴到终端运行。
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
        // 不指定输出目录时，不同文件夹下的输入会生成不同的路径
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

        // 自定义输出目录：来自不同文件夹的同名文件会发生冲突，后续条目会追加数字后缀
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
        assert_eq!(outputs[1], r"D:\out\movie_encoded_1.mp4");
        assert_eq!(outputs[2], r"D:\out\movie_encoded_2.mp4");

        // 同一文件被添加两次时，即使不指定输出目录也会发生冲突
        let outputs = derive_output_paths_unique(
            &[r"C:\a\movie.mp4".into(), r"C:\a\movie.mp4".into()],
            &config,
            None,
        );
        assert_eq!(outputs[0], r"C:\a\movie_encoded.mp4");
        assert_eq!(outputs[1], r"C:\a\movie_encoded_1.mp4");
    }

    #[test]
    fn derive_output_paths_unique_avoids_disk_file_collision() {
        let temp_dir = std::env::temp_dir().join(format!("zffmpeg_test_collision_{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let temp_dir_str = temp_dir.to_string_lossy().to_string();

        let config = sample_config();
        let input = "video.mp4";

        // 预先在磁盘上创建基础输出文件：video_encoded.mp4
        let base_file = temp_dir.join("video_encoded.mp4");
        std::fs::write(&base_file, b"existing").unwrap();

        // 第 1 次推导：应检测到 video_encoded.mp4 已存在，并生成 video_encoded_1.mp4
        let outputs = derive_output_paths_unique(&[input.into()], &config, Some(&temp_dir_str));
        assert_eq!(
            outputs[0],
            temp_dir.join("video_encoded_1.mp4").to_string_lossy().to_string()
        );

        // 同样在磁盘上预先创建 video_encoded_1.mp4
        let second_file = temp_dir.join("video_encoded_1.mp4");
        std::fs::write(&second_file, b"existing 1").unwrap();

        // 第 2 次推导：应检测到两者均存在，并生成 video_encoded_2.mp4
        let outputs_next = derive_output_paths_unique(&[input.into()], &config, Some(&temp_dir_str));
        assert_eq!(
            outputs_next[0],
            temp_dir.join("video_encoded_2.mp4").to_string_lossy().to_string()
        );

        // 清理临时目录
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn derive_output_paths_unique_avoids_claimed_collision() {
        let config = sample_config();
        let claimed = vec![r"D:\out\movie_encoded.mp4".to_string()];
        let outputs = derive_output_paths_unique_with_claimed(
            &[r"C:\a\movie.mp4".into()],
            &config,
            Some(r"D:\out"),
            &claimed,
        );
        assert_eq!(outputs[0], r"D:\out\movie_encoded_1.mp4");
    }

    // ---- build_ffmpeg_args 测试 ----

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
        // medium 映射为数值 6
        assert_args_contain(&args, &["-c:v", "libsvtav1", "-preset", "6", "-crf", "23"]);
    }

    #[test]
    fn build_args_vp9_software_uses_cpu_used() {
        let mut config = sample_config();
        config.video_codec = VideoCodec::VP9;
        let args = build_ffmpeg_args(&config, "in.mp4", "out.mp4");
        // medium → 3, VP9 需要 -crf 23 -b:v 0
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
        // 输入码率 4000 kbps -> maxrate = 4000 * 1.25 = 5000k, bufsize = 10000k
        let args = build_ffmpeg_args_with_bitrate(&config, "in.mp4", "out.mp4", Some(4000));
        assert_args_contain(&args, &["-maxrate", "5000k", "-bufsize", "10000k"]);

        // CPU 编码（hw 为 None）时，不应注入自动 maxrate 保护
        let cpu_args = build_ffmpeg_args_with_bitrate(&sample_config(), "in.mp4", "out.mp4", Some(4000));
        assert!(!cpu_args.contains(&"-maxrate".to_string()));

        // 当用户在 additional_params 中显式提供了 -maxrate 时，不要覆盖
        let mut custom = config;
        custom.video_settings.additional_params = vec!["-maxrate".into(), "3000k".into()];
        let custom_args = build_ffmpeg_args_with_bitrate(&custom, "in.mp4", "out.mp4", Some(4000));
        let pos = custom_args.iter().position(|a| a == "-maxrate").unwrap();
        assert_eq!(custom_args[pos + 1], "3000k");
    }
}
