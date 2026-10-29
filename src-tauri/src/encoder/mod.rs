pub mod args;
pub mod codec;
pub mod engine;
pub mod estimate;
pub mod flags;
pub mod hw_accel;
pub mod probe;
pub mod progress;
pub mod supervisor;
pub mod vmaf;
pub mod volume;

/// 取路径的文件名（含扩展名）；路径无文件名时返回空字符串。
/// 供日志脱敏与前端展示统一使用，避免各处手写 `Path::file_name`。
pub fn file_name_from_path(p: &str) -> String {
    std::path::Path::new(p)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string()
}
