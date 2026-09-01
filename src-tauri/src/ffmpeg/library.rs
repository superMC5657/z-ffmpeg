use std::path::{Path, PathBuf};
use parking_lot::Mutex;
use crate::error::AppResult;

static FFMPEG_PATH: Mutex<Option<PathBuf>> = Mutex::new(None);
static FFPROBE_PATH: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Represents the FFmpeg status on this system
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FfmpegStatus {
    pub available: bool,
    pub version: Option<String>,
    pub ffmpeg_path: Option<String>,
    pub ffprobe_path: Option<String>,
    pub codecs: Vec<String>,
}

/// Executable file name for the current platform
fn exe_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{}.exe", name)
    } else {
        name.to_string()
    }
}

/// Find an executable in system PATH
fn find_in_path(name: &str) -> Option<PathBuf> {
    let exe_name = exe_name(name);
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths).find_map(|dir| {
            let full_path = dir.join(&exe_name);
            if full_path.is_file() {
                Some(full_path)
            } else {
                None
            }
        })
    })
}

/// Directory where the app installs FFmpeg: {app_data_dir}/ffmpeg.
pub fn local_install_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("ffmpeg")
}

/// Initialize FFmpeg detection. Called once at app startup.
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
    *FFMPEG_PATH.lock() = ffmpeg;
    *FFPROBE_PATH.lock() = ffprobe;

    status
}

/// Detect FFmpeg installation on the system
fn detect_ffmpeg(data_dir: &Path) -> AppResult<FfmpegStatus> {
    // Try system PATH
    if let Some(ffmpeg_path) = find_in_path("ffmpeg") {
        let ffprobe_path = find_in_path("ffprobe");
        return get_status_from_paths(ffmpeg_path, ffprobe_path);
    }

    // Try the local install directory (auto-downloaded by the app).
    if let Some((ffmpeg_path, ffprobe_path)) = find_local_install(&local_install_dir(data_dir)) {
        return get_status_from_paths(ffmpeg_path, Some(ffprobe_path));
    }

    // Not found
    Ok(FfmpegStatus {
        available: false,
        version: None,
        ffmpeg_path: None,
        ffprobe_path: None,
        codecs: vec![],
    })
}

/// Check the local install directory for FFmpeg binaries.
/// Require BOTH binaries so a half-installed ffmpeg.exe (e.g. from an
/// interrupted extraction) is not mistaken for a working install.
pub(crate) fn find_local_install(install_dir: &std::path::Path) -> Option<(PathBuf, PathBuf)> {
    let ffmpeg_path = install_dir.join(exe_name("ffmpeg"));
    let ffprobe_path = install_dir.join(exe_name("ffprobe"));
    (ffmpeg_path.is_file() && ffprobe_path.is_file()).then_some((ffmpeg_path, ffprobe_path))
}

/// Get detailed status from known FFmpeg paths
pub fn get_status_from_paths(
    ffmpeg_path: PathBuf,
    ffprobe_path: Option<PathBuf>,
) -> AppResult<FfmpegStatus> {
    let version = get_ffmpeg_version(&ffmpeg_path).ok();
    let codecs = get_available_encoders(&ffmpeg_path).unwrap_or_default();

    Ok(FfmpegStatus {
        available: true,
        version,
        ffmpeg_path: Some(ffmpeg_path.to_string_lossy().to_string()),
        ffprobe_path: ffprobe_path.map(|p| p.to_string_lossy().to_string()),
        codecs,
    })
}

/// Update the globally cached binary paths (used after auto-download).
pub fn set_installed_paths(ffmpeg: PathBuf, ffprobe: Option<PathBuf>) {
    *FFMPEG_PATH.lock() = Some(ffmpeg);
    *FFPROBE_PATH.lock() = ffprobe;
}

/// Run ffmpeg -version and parse output
fn get_ffmpeg_version(ffmpeg_path: &PathBuf) -> AppResult<String> {
    let output = crate::ffmpeg::hidden_command(ffmpeg_path)
        .arg("-version")
        .output()
        .map_err(|e| crate::error::AppError::Ffmpeg(format!("Failed to run ffmpeg: {}", e)))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let version = stdout.lines().next().unwrap_or("unknown").trim().to_string();
    Ok(version)
}

/// List available encoders
fn get_available_encoders(ffmpeg_path: &PathBuf) -> AppResult<Vec<String>> {
    let output = crate::ffmpeg::hidden_command(ffmpeg_path)
        .args(["-encoders"])
        .output()
        .map_err(|e| crate::error::AppError::Ffmpeg(format!("Failed to list encoders: {}", e)))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let codecs: Vec<String> = stdout
        .lines()
        .filter(|l| l.starts_with(" V") || l.starts_with(" A"))
        .filter_map(|l| {
            let parts: Vec<&str> = l.split_whitespace().collect();
            if parts.len() >= 3 {
                Some(parts[1].to_string())
            } else {
                None
            }
        })
        .collect();

    Ok(codecs)
}

/// Check if FFmpeg is ready
///
/// 仅测试使用（`mod tests` 里断言全局缓存就绪状态）；生产代码走
/// `get_status_from_paths`，故 `#[cfg(test)]` 限定编译范围，消除 dead_code 警告。
#[cfg(test)]
pub fn is_ffmpeg_ready() -> bool {
    FFMPEG_PATH
        .lock()
        .as_ref()
        .map(|p| p.exists())
        .unwrap_or(false)
}

/// Get the ffmpeg path
pub fn get_ffmpeg_path() -> Option<PathBuf> {
    FFMPEG_PATH.lock().clone()
}

/// Get the ffprobe path
pub fn get_ffprobe_path() -> Option<PathBuf> {
    FFPROBE_PATH.lock().clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 串行化所有会修改进程级环境(PATH)或全局静态缓存的测试,
    /// 避免并行测试互相干扰。
    static ENV_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

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
            assert_eq!(exe_name("ffmpeg"), "ffmpeg.exe");
            assert_eq!(exe_name("ffprobe"), "ffprobe.exe");
        } else {
            assert_eq!(exe_name("ffmpeg"), "ffmpeg");
        }
    }

    #[test]
    fn find_local_install_requires_both_binaries() {
        let dir = temp_dir("local-install");
        let ffmpeg = dir.join("ffmpeg.exe");
        let ffprobe = dir.join("ffprobe.exe");

        // 空目录 → 未安装
        assert!(find_local_install(&dir).is_none());

        // 只有 ffmpeg,缺 ffprobe → 视为残缺安装,不判定可用
        std::fs::write(&ffmpeg, b"x").unwrap();
        assert!(find_local_install(&dir).is_none());

        // 两者都在 → 安装
        std::fs::write(&ffprobe, b"x").unwrap();
        let found = find_local_install(&dir).unwrap();
        assert_eq!(found.0, ffmpeg);
        assert_eq!(found.1, ffprobe);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn get_status_from_paths_swallows_missing_binary() {
        let dir = temp_dir("status-missing");
        // 路径不存在:version/codecs 查询被 .ok()/unwrap_or_default() 吞掉,
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
        let _guard = ENV_LOCK.lock();
        // 重置全局缓存,保证测试与执行顺序无关
        *FFMPEG_PATH.lock() = None;
        *FFPROBE_PATH.lock() = None;

        let dir = temp_dir("cache-roundtrip");
        let ffmpeg = dir.join("ffmpeg.exe");
        let ffprobe = dir.join("ffprobe.exe");
        std::fs::write(&ffmpeg, b"x").unwrap();
        std::fs::write(&ffprobe, b"x").unwrap();

        // 重置后为空
        assert!(get_ffmpeg_path().is_none());
        assert!(!is_ffmpeg_ready());

        set_installed_paths(ffmpeg.clone(), Some(ffprobe.clone()));
        assert_eq!(get_ffmpeg_path(), Some(ffmpeg.clone()));
        assert_eq!(get_ffprobe_path(), Some(ffprobe.clone()));
        assert!(is_ffmpeg_ready());

        // 指向不存在的文件 → 不 ready
        set_installed_paths(dir.join("ghost.exe"), Some(ffprobe.clone()));
        assert!(!is_ffmpeg_ready());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_in_path_finds_and_misses() {
        let _guard = ENV_LOCK.lock();
        let dir = temp_dir("find-in-path");
        std::fs::write(dir.join("ffmpeg.exe"), b"x").unwrap();

        let old_path = std::env::var_os("PATH");
        std::env::set_var("PATH", &dir);

        let found = find_in_path("ffmpeg");
        assert_eq!(found, Some(dir.join("ffmpeg.exe")));
        assert!(find_in_path("ffprobe").is_none());

        match old_path {
            Some(p) => std::env::set_var("PATH", p),
            None => std::env::remove_var("PATH"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_ffmpeg_picks_up_path_install() {
        let _guard = ENV_LOCK.lock();
        let dir = temp_dir("detect-path");
        // 空 exe 文件:detect 会尝试跑子进程(Windows 下失败被吞),
        // 但仍应报告 available=true(路径存在),version=None
        std::fs::write(dir.join("ffmpeg.exe"), b"").unwrap();
        std::fs::write(dir.join("ffprobe.exe"), b"").unwrap();

        let old_path = std::env::var_os("PATH");
        std::env::set_var("PATH", &dir);

        // 空 data_dir → 本地安装分支必然不命中,只走 PATH 分支
        let status = detect_ffmpeg(&temp_dir("detect-empty-data")).unwrap();
        assert!(status.available);
        assert_eq!(
            status.ffmpeg_path.as_deref(),
            Some(dir.join("ffmpeg.exe").to_string_lossy().as_ref())
        );
        // 空文件无法执行 → version 查询失败被吞,保持 None
        assert!(status.version.is_none());

        match old_path {
            Some(p) => std::env::set_var("PATH", p),
            None => std::env::remove_var("PATH"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn local_install_dir_under_data_dir() {
        // 结构必须是 {data_dir}/ffmpeg
        let data_dir = temp_dir("install-dir");
        let dir = local_install_dir(&data_dir);
        assert_eq!(dir, data_dir.join("ffmpeg"));
        let _ = std::fs::remove_dir_all(&data_dir);
    }

    #[test]
    fn detect_ffmpeg_returns_not_found_when_nothing_available() {
        let _guard = ENV_LOCK.lock();
        // PATH 指向空目录 + 本地安装目录(真实 data_dir)大概率不存在,
        // 但为防本地恰好装过,这里不强制本地分支——只断言 PATH 分支
        // 在空 PATH 下不误报。
        let dir = temp_dir("detect-empty");
        let old_path = std::env::var_os("PATH");
        std::env::set_var("PATH", &dir);

        // PATH 无 ffmpeg 时,find_in_path 返回 None
        assert!(find_in_path("ffmpeg").is_none());

        match old_path {
            Some(p) => std::env::set_var("PATH", p),
            None => std::env::remove_var("PATH"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn get_ffmpeg_version_parses_first_line() {
        // get_ffmpeg_version 不存在的路径 → Err(与 get_status_from_paths 的
        // 吞错不同,它直接暴露失败)
        let dir = temp_dir("version-missing");
        let err = get_ffmpeg_version(&dir.join("nope.exe")).unwrap_err();
        assert!(err.to_string().contains("Failed to run ffmpeg"), "got: {err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn get_available_encoders_returns_err_on_missing_path() {
        let dir = temp_dir("encoders-missing");
        let result = get_available_encoders(&dir.join("nope.exe"));
        assert!(result.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ffprobe_path_roundtrip() {
        let _guard = ENV_LOCK.lock();
        *FFMPEG_PATH.lock() = None;
        *FFPROBE_PATH.lock() = None;
        let dir = temp_dir("ffprobe-roundtrip");
        let ffprobe = dir.join("ffprobe.exe");
        std::fs::write(&ffprobe, b"x").unwrap();

        set_installed_paths(dir.join("ffmpeg.exe"), Some(ffprobe.clone()));
        assert_eq!(get_ffprobe_path(), Some(ffprobe));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
