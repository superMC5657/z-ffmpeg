use std::path::{Path, PathBuf};

use crate::error::AppResult;
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
    let status = detect_ffmpeg(data_dir).unwrap_or(FfmpegStatus {
        available: false,
        version: None,
        ffmpeg_path: None,
        ffprobe_path: None,
        codecs: vec![],
    });

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

/// 检测系统中安装的 FFmpeg（二进制解析经门面，优先级链不变）。
fn detect_ffmpeg(data_dir: &Path) -> AppResult<FfmpegStatus> {
    let (ffmpeg_path, ffprobe_path) = facade::resolve_paths(data_dir);
    status_for_resolved(ffmpeg_path, ffprobe_path)
}

/// 已解析路径 → 状态的纯映射（可单测，无环境依赖）：
/// 有 ffmpeg 则聚合版本/编码器（缺二进制吞错见 `get_status_from_paths`），
/// 否则不可用。
fn status_for_resolved(
    ffmpeg_path: Option<PathBuf>,
    ffprobe_path: Option<PathBuf>,
) -> AppResult<FfmpegStatus> {
    match ffmpeg_path {
        Some(ffmpeg_path) => get_status_from_paths(ffmpeg_path, ffprobe_path),
        None => Ok(FfmpegStatus {
            available: false,
            version: None,
            ffmpeg_path: None,
            ffprobe_path: None,
            codecs: vec![],
        }),
    }
}

/// 从已知的 FFmpeg 路径获取详细状态
pub fn get_status_from_paths(
    ffmpeg_path: PathBuf,
    ffprobe_path: Option<PathBuf>,
) -> AppResult<FfmpegStatus> {
    let version = facade::query_version(&ffmpeg_path);
    let codecs = facade::query_encoders(&ffmpeg_path);

    Ok(FfmpegStatus {
        available: true,
        version,
        ffmpeg_path: Some(ffmpeg_path.to_string_lossy().to_string()),
        ffprobe_path: ffprobe_path.map(|p| p.to_string_lossy().to_string()),
        codecs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffmpeg::facade::Binaries;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "z-ffmpeg-test-{}-{}",
            tag,
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn exe_name_appends_exe_on_windows() {
        if cfg!(windows) {
            assert_eq!(facade::exe_name("ffmpeg"), "ffmpeg.exe");
            assert_eq!(facade::exe_name("ffprobe"), "ffprobe.exe");
        } else {
            assert_eq!(facade::exe_name("ffmpeg"), "ffmpeg");
        }
    }

    #[test]
    fn find_local_install_requires_both_binaries() {
        let dir = temp_dir("local-install");
        let ffmpeg = dir.join(facade::exe_name("ffmpeg"));
        let ffprobe = dir.join(facade::exe_name("ffprobe"));

        // 空目录 → 未安装
        assert!(facade::find_local_install(&dir).is_none());

        // 只有 ffmpeg,缺 ffprobe → 视为残缺安装,不判定可用
        std::fs::write(&ffmpeg, b"x").unwrap();
        assert!(facade::find_local_install(&dir).is_none());

        // 两者都在 → 安装
        std::fs::write(&ffprobe, b"x").unwrap();
        let found = facade::find_local_install(&dir).unwrap();
        assert_eq!(found.0, ffmpeg);
        assert_eq!(found.1, ffprobe);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn get_status_from_paths_swallows_missing_binary() {
        let dir = temp_dir("status-missing");
        // 路径不存在:version/codecs 查询被吞掉,
        // 行为是 available=true 但 version=None、codecs=[] —— 固化该契约,
        // 下载流程已用 verify_binary 前置拦截,此函数只做状态聚合。
        let status = get_status_from_paths(dir.join("nope.exe"), None).unwrap();
        assert!(status.available);
        assert!(status.version.is_none());
        assert!(status.codecs.is_empty());
        assert_eq!(
            status.ffmpeg_path.as_deref(),
            Some(dir.join("nope.exe").to_string_lossy().as_ref())
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn installed_paths_cache_roundtrip_and_readiness() {
        // 经门面本地 handle 走：不碰全局量、不串行，可并行
        let dir = temp_dir("cache-roundtrip");
        let ffmpeg = dir.join("ffmpeg.exe");
        let ffprobe = dir.join("ffprobe.exe");
        std::fs::write(&ffmpeg, b"x").unwrap();
        std::fs::write(&ffprobe, b"x").unwrap();

        let mut h = Binaries::default();
        assert!(h.get_ffmpeg().is_none());
        assert!(!h.is_ready());

        h.set_installed(ffmpeg.clone(), Some(ffprobe.clone()));
        assert_eq!(h.get_ffmpeg(), Some(ffmpeg.clone()));
        assert_eq!(h.get_ffprobe(), Some(ffprobe.clone()));
        assert!(h.is_ready());

        // 指向不存在的文件 → 不 ready
        h.set_installed(dir.join("ghost.exe"), Some(ffprobe.clone()));
        assert!(!h.is_ready());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_in_path_finds_and_misses() {
        // 显式 PATH 取值注入：不读写进程级环境，可并行
        let dir = temp_dir("find-in-path");
        std::fs::write(dir.join(facade::exe_name("ffmpeg")), b"x").unwrap();

        let path_var = std::ffi::OsString::from(dir.as_os_str());
        let found = facade::find_in_path_in(&path_var, "ffmpeg");
        assert_eq!(found, Some(dir.join(facade::exe_name("ffmpeg"))));
        assert!(facade::find_in_path_in(&path_var, "ffprobe").is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_ffmpeg_picks_up_path_install() {
        // 注入 fake 检索：PATH 双命中 → 取 PATH，无子进程、无环境篡改
        let dir = temp_dir("detect-path");
        let ffmpeg = dir.join("ffmpeg.exe");
        let ffprobe = dir.join("ffprobe.exe");

        let (ff, fp) = facade::resolve_paths_with(
            &temp_dir("detect-empty-data"),
            &|n| match n {
                "ffmpeg" => Some(ffmpeg.clone()),
                "ffprobe" => Some(ffprobe.clone()),
                _ => None,
            },
            &|_| None,
        );
        assert_eq!(ff, Some(ffmpeg.clone()));
        assert_eq!(fp, Some(ffprobe.clone()));

        // 空文件无法执行 → 版本查询吞为 None（与搬运前 detect 吞错一致）
        assert!(facade::query_version(&ffmpeg).is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn local_install_dir_under_data_dir() {
        // 结构必须是 {data_dir}/ffmpeg
        let data_dir = temp_dir("install-dir");
        let dir = facade::local_install_dir(&data_dir);
        assert_eq!(dir, data_dir.join("ffmpeg"));
        let _ = std::fs::remove_dir_all(&data_dir);
    }

    #[test]
    fn detect_ffmpeg_returns_not_found_when_nothing_available() {
        // 注入全 miss：解析得 (None, None) → 状态不可用，无环境篡改
        let dir = temp_dir("detect-empty");
        let (ff, fp) = facade::resolve_paths_with(&dir, &|_| None, &|_| None);
        assert!(ff.is_none());
        assert!(fp.is_none());
        let status = status_for_resolved(ff, fp).unwrap();
        assert!(!status.available);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn query_version_returns_none_on_missing_path() {
        // 注入形态：缺失二进制 → None（原 get_ffmpeg_version 的 Err 吞为 None，
        // 状态聚合契约见 get_status_from_paths_swallows_missing_binary）
        let dir = temp_dir("version-missing");
        assert!(facade::query_version(&dir.join("nope.exe")).is_none());
        assert_eq!(
            facade::parse_version_output("ffmpeg version 7.1\nbuilt with gcc\n"),
            "ffmpeg version 7.1"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn query_encoders_returns_empty_on_missing_path() {
        // 注入形态：缺失二进制 → 空列表（原 Err 吞为空）
        let dir = temp_dir("encoders-missing");
        assert!(facade::query_encoders(&dir.join("nope.exe")).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ffprobe_path_roundtrip() {
        // 经门面本地 handle 走：不碰全局量，可并行
        let dir = temp_dir("ffprobe-roundtrip");
        let ffprobe = dir.join("ffprobe.exe");
        std::fs::write(&ffprobe, b"x").unwrap();

        let mut h = Binaries::default();
        h.set_installed(dir.join("ffmpeg.exe"), Some(ffprobe.clone()));
        assert_eq!(h.get_ffprobe(), Some(ffprobe));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
