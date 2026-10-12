//! ffmpeg 执行门面：二进制解析、能力查询、子进程派生的统一入口。
//!
//! 收敛前人人直达全局量（`FFMPEG_PATH` / `FFPROBE_PATH`），各模块手搓
//! 子进程习语，且 `ffmpeg -encoders` 输出被解析两次（`library` 留
//! ` V`+` A` 行，`hw_accel` 只留 ` V` 行）——同一能力面两个 adapter。
//! 本模块唯一拥有：二进制状态（PATH vs 本地安装 vs 下载件的优先级链）、
//! 能力查询（encoders/filters/version 解析各一份、可缓存）、子进程派生
//!（sync/async、Windows 隐藏窗口、cwd）单一接口。`hw_accel`、`vmaf`、
//! `probe`、`engine`、`downloader` 改为消费它；`library` 只保留状态聚合
//!（`FfmpegStatus`）与启动日志。纯解析/检索函数可注入 fake 测试，
//! 不再篡改进程级 PATH，也不再需要 ENV_LOCK 串行。

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use parking_lot::Mutex;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Windows：防止从 GUI 应用启动控制台程序（ffmpeg/ffprobe）时弹出终端窗口。
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

// ============================================================
// 子进程派生（唯一入口；`ffmpeg/mod.rs` 保留同名薄委托）
// ============================================================

/// 创建一个绝不显示控制台窗口的 `Command`（Windows）。
/// 在其他平台上的行为与 `Command::new` 完全一致。
pub fn hidden_command(program: impl AsRef<std::ffi::OsStr>) -> std::process::Command {
    #[cfg(windows)]
    {
        let mut cmd = std::process::Command::new(program);
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new(program)
    }
}

/// `hidden_command` 的异步版本（`tokio::process::Command`）。
pub fn async_hidden_command(program: impl AsRef<std::ffi::OsStr>) -> tokio::process::Command {
    #[cfg(windows)]
    {
        let mut cmd = tokio::process::Command::new(program);
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd
    }
    #[cfg(not(windows))]
    {
        tokio::process::Command::new(program)
    }
}

/// 带工作目录的同步版本（VMAF 把日志写相对文件名，cwd 由调用方隔离）。
pub fn hidden_command_in(
    program: impl AsRef<std::ffi::OsStr>,
    cwd: &Path,
) -> std::process::Command {
    let mut cmd = hidden_command(program);
    cmd.current_dir(cwd);
    cmd
}

// ============================================================
// 二进制状态（原 library 全局量收为私有状态 + 可注入 handle）
// ============================================================

/// 一组二进制路径。全局缓存经 `GLOBAL_BINARIES` 走，测试走本地 handle，
/// 不再触碰进程级环境。
#[derive(Debug, Default, Clone)]
pub struct Binaries {
    pub ffmpeg: Option<PathBuf>,
    pub ffprobe: Option<PathBuf>,
}

impl Binaries {
    pub fn get_ffmpeg(&self) -> Option<PathBuf> {
        self.ffmpeg.clone()
    }

    pub fn get_ffprobe(&self) -> Option<PathBuf> {
        self.ffprobe.clone()
    }

    /// 下载完成后刷新缓存（经门面走，不再跨模块直写全局量）。
    pub fn set_installed(&mut self, ffmpeg: PathBuf, ffprobe: Option<PathBuf>) {
        self.ffmpeg = Some(ffmpeg);
        self.ffprobe = ffprobe;
    }

    pub fn set_resolved(&mut self, ffmpeg: Option<PathBuf>, ffprobe: Option<PathBuf>) {
        self.ffmpeg = ffmpeg;
        self.ffprobe = ffprobe;
    }

    /// 本地 handle 是否就绪（ffmpeg 路径存在）。仅测试使用。
    #[cfg(test)]
    pub fn is_ready(&self) -> bool {
        self.ffmpeg.as_ref().map(|p| p.exists()).unwrap_or(false)
    }
}

static GLOBAL_BINARIES: Mutex<Binaries> = Mutex::new(Binaries {
    ffmpeg: None,
    ffprobe: None,
});

/// 获取当前缓存的 ffmpeg 路径
pub fn get_ffmpeg_path() -> Option<PathBuf> {
    GLOBAL_BINARIES.lock().get_ffmpeg()
}

/// 获取当前缓存的 ffprobe 路径
pub fn get_ffprobe_path() -> Option<PathBuf> {
    GLOBAL_BINARIES.lock().get_ffprobe()
}

/// 更新全局缓存的二进制文件路径（在自动下载完成后调用）。
pub fn set_installed_paths(ffmpeg: PathBuf, ffprobe: Option<PathBuf>) {
    GLOBAL_BINARIES.lock().set_installed(ffmpeg, ffprobe);
}

pub(crate) fn set_resolved_paths(ffmpeg: Option<PathBuf>, ffprobe: Option<PathBuf>) {
    GLOBAL_BINARIES.lock().set_resolved(ffmpeg, ffprobe);
}

// ============================================================
// 二进制解析（PATH vs 本地安装优先级链，逐条与搬运前一致）
// ============================================================

/// 当前平台对应的可执行文件名
pub(crate) fn exe_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{}.exe", name)
    } else {
        name.to_string()
    }
}

/// 在系统环境变量 PATH 中查找指定可执行文件
pub fn find_in_path(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH")
        .and_then(|paths| find_in_path_in(&paths, name))
}

/// 可注入的 PATH 检索内核：`path_var` 为 PATH 变量取值，纯函数、可单测，
/// 不触碰进程级环境。
pub(crate) fn find_in_path_in(path_var: &OsStr, name: &str) -> Option<PathBuf> {
    let exe_name = exe_name(name);
    std::env::split_paths(path_var).find_map(|dir| {
        let full_path = dir.join(&exe_name);
        if full_path.is_file() {
            Some(full_path)
        } else {
            None
        }
    })
}

/// 本应用安装 FFmpeg 的目标目录：{app_data_dir}/ffmpeg。
pub fn local_install_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("ffmpeg")
}

/// 检查本地安装目录下的 FFmpeg 可执行二进制文件。
/// 必须同时包含 ffmpeg 和 ffprobe 两个二进制文件，
/// 避免将仅解压一半的文件（如解压中断）误判为可用安装。
pub(crate) fn find_local_install(install_dir: &Path) -> Option<(PathBuf, PathBuf)> {
    let ffmpeg_path = install_dir.join(exe_name("ffmpeg"));
    let ffprobe_path = install_dir.join(exe_name("ffprobe"));
    (ffmpeg_path.is_file() && ffprobe_path.is_file()).then_some((ffmpeg_path, ffprobe_path))
}

/// 解析二进制路径（PATH 优先，其次本地安装）：
///
/// 1. PATH 有 ffmpeg → ffprobe 取 PATH，否则回退本地安装的 ffprobe；
/// 2. PATH 无 ffmpeg → 本地安装成对出现才可用；
/// 3. 否则均无。只做路径检索，不派生子进程。
pub fn resolve_paths(data_dir: &Path) -> (Option<PathBuf>, Option<PathBuf>) {
    resolve_paths_with(
        data_dir,
        &|name| find_in_path(name),
        &|dir| find_local_install(dir),
    )
}

/// 可注入的解析内核：检索函数由调用方传入，测试用 fake 闭包，
/// 不篡改 PATH、不派生子进程。
pub(crate) fn resolve_paths_with(
    data_dir: &Path,
    find_path: &dyn Fn(&str) -> Option<PathBuf>,
    find_local: &dyn Fn(&Path) -> Option<(PathBuf, PathBuf)>,
) -> (Option<PathBuf>, Option<PathBuf>) {
    // 尝试从系统环境变量 PATH 中查找
    if let Some(ffmpeg_path) = find_path("ffmpeg") {
        let ffprobe_path = find_path("ffprobe")
            .or_else(|| find_local(&local_install_dir(data_dir)).map(|(_, p)| p));
        return (Some(ffmpeg_path), ffprobe_path);
    }

    // 尝试从本地安装目录（由应用自动下载）中查找。
    if let Some((ffmpeg_path, ffprobe_path)) = find_local(&local_install_dir(data_dir)) {
        return (Some(ffmpeg_path), Some(ffprobe_path));
    }

    // 未找到
    (None, None)
}

// ============================================================
// 能力查询（解析各一份：encoders / version / filters）
// ============================================================

/// 运行子进程并取 stdout 文本；派生失败一律吞为 None（调用方按缺失处理）。
pub fn run_stdout_text(program: &Path, args: &[&str]) -> Option<String> {
    let output = hidden_command(program).args(args).output().ok()?;
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// 解析 `ffmpeg -encoders` 输出：保留视频（` V`）与音频（` A`）编码器行，
/// 取第二列名称；列不足或它行一律丢弃。
/// 单一过滤语义：`hw_accel` 只查 `{h264,hevc,av1}_*` 视频编码器名，
/// 音频行永不命中，故与它原先的纯 ` V` 过滤行为完全一致。
pub fn parse_encoders_output(text: &str) -> Vec<String> {
    text.lines()
        .filter(|l| l.starts_with(" V") || l.starts_with(" A"))
        .filter_map(|l| {
            let parts: Vec<&str> = l.split_whitespace().collect();
            if parts.len() >= 3 {
                Some(parts[1].to_string())
            } else {
                None
            }
        })
        .collect()
}

/// 解析 `ffmpeg -version` 输出：取首行；空输出记 `unknown`（与搬运前一致）。
pub fn parse_version_output(text: &str) -> String {
    text.lines().next().unwrap_or("unknown").trim().to_string()
}

/// 解析 `ffmpeg -filters` 输出：是否编译了 libvmaf。
pub fn parse_filters_support(text: &str) -> bool {
    text.lines().any(|l| l.contains("libvmaf"))
}

/// 查询可用编码器列表；二进制缺失/派生失败一律吞为空（调用方按无能力处理）。
pub fn query_encoders(ffmpeg_path: &Path) -> Vec<String> {
    run_stdout_text(ffmpeg_path, &["-encoders"])
        .map(|t| parse_encoders_output(&t))
        .unwrap_or_default()
}

/// 查询版本行；失败返回 None。
pub fn query_version(ffmpeg_path: &Path) -> Option<String> {
    run_stdout_text(ffmpeg_path, &["-version"]).map(|t| parse_version_output(&t))
}

/// 当前 ffmpeg 是否编译了 libvmaf 滤镜。
pub fn supports_libvmaf(ffmpeg_path: &Path) -> bool {
    run_stdout_text(ffmpeg_path, &["-hide_banner", "-filters"])
        .map(|t| parse_filters_support(&t))
        .unwrap_or(false)
}

/// 经全局缓存查询可用编码器；未缓存一律为空（调用方按无能力处理）。
pub fn available_encoders() -> Vec<String> {
    get_ffmpeg_path()
        .map(|p| query_encoders(&p))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("z-ffmpeg-test-{}-{}", tag, uuid::Uuid::new_v4()));
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
    fn find_in_path_in_finds_and_misses_without_touching_env() {
        let dir = temp_dir("find-in-path");
        std::fs::write(dir.join(exe_name("ffmpeg")), b"x").unwrap();

        // 显式传入 PATH 取值：不读不写进程级环境，可并行
        let path_var = std::ffi::OsString::from(dir.as_os_str());
        let found = find_in_path_in(&path_var, "ffmpeg");
        assert_eq!(found, Some(dir.join(exe_name("ffmpeg"))));
        assert!(find_in_path_in(&path_var, "ffprobe").is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_local_install_requires_both_binaries() {
        let dir = temp_dir("local-install");
        let ffmpeg = dir.join(exe_name("ffmpeg"));
        let ffprobe = dir.join(exe_name("ffprobe"));

        assert!(find_local_install(&dir).is_none());

        std::fs::write(&ffmpeg, b"x").unwrap();
        assert!(find_local_install(&dir).is_none());

        std::fs::write(&ffprobe, b"x").unwrap();
        let found = find_local_install(&dir).unwrap();
        assert_eq!(found.0, ffmpeg);
        assert_eq!(found.1, ffprobe);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn local_install_dir_under_data_dir() {
        let data_dir = temp_dir("install-dir");
        assert_eq!(local_install_dir(&data_dir), data_dir.join("ffmpeg"));
        let _ = std::fs::remove_dir_all(&data_dir);
    }

    /// 解析优先级矩阵（注入 fake 检索，无 PATH 篡改、无子进程）：
    /// PATH 命中 > PATH ffmpeg + 本地 ffprobe 回退 > 本地成对 > 全 miss。
    #[test]
    fn resolve_priority_matrix_with_injected_search() {
        let data_dir = temp_dir("resolve");
        let path_ff = PathBuf::from("/path/ffmpeg");
        let path_fp = PathBuf::from("/path/ffprobe");
        let local_ff = PathBuf::from("/local/ffmpeg");
        let local_fp = PathBuf::from("/local/ffprobe");

        // 1. PATH 双命中 → 取 PATH
        let (ff, fp) = resolve_paths_with(
            &data_dir,
            &|n| match n {
                "ffmpeg" => Some(path_ff.clone()),
                "ffprobe" => Some(path_fp.clone()),
                _ => None,
            },
            &|_| None,
        );
        assert_eq!((ff, fp), (Some(path_ff.clone()), Some(path_fp.clone())));

        // 2. PATH 有 ffmpeg、无 ffprobe → ffprobe 回退本地
        let (ff, fp) = resolve_paths_with(
            &data_dir,
            &|n| if n == "ffmpeg" { Some(path_ff.clone()) } else { None },
            &|_| Some((local_ff.clone(), local_fp.clone())),
        );
        assert_eq!((ff, fp), (Some(path_ff.clone()), Some(local_fp.clone())));

        // 3. PATH 全无 → 本地成对
        let (ff, fp) = resolve_paths_with(
            &data_dir,
            &|_| None,
            &|_| Some((local_ff.clone(), local_fp.clone())),
        );
        assert_eq!((ff, fp), (Some(local_ff.clone()), Some(local_fp.clone())));

        // 4. 全 miss → (None, None)
        assert_eq!(
            resolve_paths_with(&data_dir, &|_| None, &|_| None),
            (None, None)
        );

        let _ = std::fs::remove_dir_all(&data_dir);
    }

    /// `-encoders` 单一解析语义快照：V+A 行保留、列不足/它行丢弃。
    #[test]
    fn parse_encoders_output_keeps_video_and_audio() {
        // 注意：不用 `\` 续行写多行字面量（Rust 会剥掉续行首空格，
        // 而过滤语义恰恰依赖行首空格），改用显式 \n 拼接。
        let text = "Encoders:\n V..... libx264              Test\n V..... h264_nvenc            Test\n A..... aac                  Test\nBADLINE\n V.... short\n S..... ass                  Test\n";
        let list = parse_encoders_output(text);
        assert!(list.contains(&"libx264".to_string()));
        assert!(list.contains(&"h264_nvenc".to_string()));
        // 音频行保留（单一语义）；hw_accel 只查视频名故行为不变
        assert!(list.contains(&"aac".to_string()));
        assert!(!list.iter().any(|e| e == "short" || e == "ass" || e == "BADLINE"));
    }

    #[test]
    fn parse_version_output_takes_first_line() {
        assert_eq!(
            parse_version_output("ffmpeg version 7.1\nbuilt with gcc\n"),
            "ffmpeg version 7.1"
        );
        assert_eq!(parse_version_output(""), "unknown");
        assert_eq!(parse_version_output("   "), "");
    }

    #[test]
    fn parse_filters_support_detects_libvmaf() {
        assert!(parse_filters_support(" ... libvmaf Video Multimethod Assessment Fusion ...\n"));
        assert!(!parse_filters_support(" ... scale filter ...\n"));
        assert!(!parse_filters_support(""));
    }

    /// 缺失二进制：查询一律吞为 None/空/false，不抛错（无 ffmpeg 可派生）。
    #[test]
    fn queries_on_missing_binary_swallow_errors() {
        let missing = temp_dir("missing").join("nope.exe");
        assert!(query_version(&missing).is_none());
        assert!(query_encoders(&missing).is_empty());
        assert!(!supports_libvmaf(&missing));
    }

    /// 本地 handle 语义：set/get/ready/ghost（不碰全局量，可并行）。
    #[test]
    fn binaries_handle_roundtrip_and_readiness() {
        let dir = temp_dir("binaries-handle");
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
}
