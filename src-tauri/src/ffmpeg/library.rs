use std::path::{Path, PathBuf};

use crate::ffmpeg::facade;

/// 表示当前系统上的 FFmpeg 状态
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FfmpegStatus {
    pub available: bool,
    pub version: Option<String>,
    pub ffmpeg_path: Option<String>,
    pub ffprobe_path: Option<String>,
    pub codecs: Vec<String>,
}

/// 初始化 FFmpeg 检测。在应用启动时调用一次。
pub fn init_ffmpeg(data_dir: &Path) -> FfmpegStatus {
    let status = detect_ffmpeg(data_dir);

    let ffmpeg = status.ffmpeg_path.clone().map(PathBuf::from);
    let ffprobe = status.ffprobe_path.clone().map(PathBuf::from);

    // 启动检测结果：只记来源 + 版本，完整路径只进 debug
    if status.available {
        let version = status.version.as_deref().unwrap_or("unknown");
        let kind = match status.ffmpeg_path.as_deref() {
            Some(p) if Path::new(p).starts_with(facade::local_install_dir(data_dir)) => "bundled",
            _ => "external",
        };
        log::info!("ffmpeg detected {kind} version {version}");
        // 路径脱敏：只记 basename，不记全路径原文
        let basename = status
            .ffmpeg_path
            .as_deref()
            .and_then(|p| Path::new(p).file_name())
            .unwrap_or_default()
            .to_string_lossy();
        log::debug!("ffmpeg detected file {basename}");
    } else {
        log::info!("ffmpeg missing");
    }

    facade::set_resolved_paths(ffmpeg, ffprobe);

    status
}

/// 检测系统中安装的 FFmpeg：二进制解析与能力查询经门面直调
/// （优先级链与吞错语义不变），状态组装走本地纯函数。
fn detect_ffmpeg(data_dir: &Path) -> FfmpegStatus {
    let (ffmpeg_path, ffprobe_path) = facade::resolve_paths(data_dir);
    match ffmpeg_path {
        Some(ref ffmpeg) => {
            let version = facade::query_version(ffmpeg);
            let codecs = facade::query_encoders(ffmpeg);
            assemble_status(Some(ffmpeg.clone()), ffprobe_path, version, codecs)
        }
        None => assemble_status(None, None, None, Vec::new()),
    }
}

/// 已解析路径 + 已查询能力 → 状态的纯组装（无环境依赖、无子进程）：
/// 有 ffmpeg 则透传版本/编码器与路径字串，否则统一不可用
/// （传入的版本/编码器一律丢弃，保证缺二进制语义单一）。
pub fn assemble_status(
    ffmpeg_path: Option<PathBuf>,
    ffprobe_path: Option<PathBuf>,
    version: Option<String>,
    codecs: Vec<String>,
) -> FfmpegStatus {
    match ffmpeg_path {
        Some(ffmpeg_path) => FfmpegStatus {
            available: true,
            version,
            ffmpeg_path: Some(ffmpeg_path.to_string_lossy().to_string()),
            ffprobe_path: ffprobe_path.map(|p| p.to_string_lossy().to_string()),
            codecs,
        },
        None => FfmpegStatus {
            available: false,
            version: None,
            ffmpeg_path: None,
            ffprobe_path: None,
            codecs: Vec::new(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assemble_available_passthrough_with_both_paths() {
        // 有 ffmpeg：版本/编码器/路径字串透传，ffprobe 有值
        let ffmpeg = PathBuf::from("/bin/ffmpeg");
        let ffprobe = PathBuf::from("/bin/ffprobe");
        let status = assemble_status(
            Some(ffmpeg.clone()),
            Some(ffprobe.clone()),
            Some("ffmpeg version 7.1".to_string()),
            vec!["h264_nvenc".to_string(), "aac".to_string()],
        );
        assert!(status.available);
        assert_eq!(status.version.as_deref(), Some("ffmpeg version 7.1"));
        assert_eq!(
            status.ffmpeg_path.as_deref(),
            Some(ffmpeg.to_string_lossy().as_ref())
        );
        assert_eq!(
            status.ffprobe_path.as_deref(),
            Some(ffprobe.to_string_lossy().as_ref())
        );
        assert_eq!(status.codecs, vec!["h264_nvenc".to_string(), "aac".to_string()]);
    }

    #[test]
    fn assemble_available_without_ffprobe() {
        // 有 ffmpeg、无 ffprobe：ffprobe 字串为 None，其余透传
        let ffmpeg = PathBuf::from("/bin/ffmpeg");
        let status = assemble_status(
            Some(ffmpeg.clone()),
            None,
            None,
            Vec::new(),
        );
        assert!(status.available);
        assert!(status.version.is_none());
        assert!(status.codecs.is_empty());
        assert_eq!(
            status.ffmpeg_path.as_deref(),
            Some(ffmpeg.to_string_lossy().as_ref())
        );
        assert!(status.ffprobe_path.is_none());
    }

    #[test]
    fn assemble_unavailable_discards_version_and_codecs() {
        // 无 ffmpeg：统一不可用，传入的版本/编码器一律丢弃
        let status = assemble_status(
            None,
            Some(PathBuf::from("/bin/ffprobe")),
            Some("ffmpeg version 7.1".to_string()),
            vec!["h264_nvenc".to_string()],
        );
        assert!(!status.available);
        assert!(status.version.is_none());
        assert!(status.ffmpeg_path.is_none());
        assert!(status.ffprobe_path.is_none());
        assert!(status.codecs.is_empty());
    }
}
