pub mod library;
pub mod downloader;

pub use library::*;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Windows：防止从 GUI 应用启动控制台程序（ffmpeg/ffprobe）时弹出终端窗口。
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 创建一个绝不显示控制台窗口的 `Command`（Windows）。
/// 在其他平台上的行为与 `Command::new` 完全一致。
pub fn hidden_command(program: impl AsRef<std::ffi::OsStr>) -> std::process::Command {
    let mut cmd = std::process::Command::new(program);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

/// `hidden_command` 的异步版本（`tokio::process::Command`）。
/// Windows 同样加 `CREATE_NO_WINDOW`（复用已有常量），同步版本保留。
pub fn async_hidden_command(program: impl AsRef<std::ffi::OsStr>) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new(program);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}
