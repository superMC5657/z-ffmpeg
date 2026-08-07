pub mod library;
pub mod downloader;
pub mod version;

pub use library::*;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Windows: prevent spawned console apps (ffmpeg/ffprobe) from
/// popping up a terminal window when launched from a GUI app.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Create a `Command` that never shows a console window (Windows).
/// On other platforms this behaves exactly like `Command::new`.
pub fn hidden_command(program: impl AsRef<std::ffi::OsStr>) -> std::process::Command {
    let mut cmd = std::process::Command::new(program);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}
