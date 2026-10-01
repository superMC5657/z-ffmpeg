use serde::Serialize;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("FFmpeg error: {0}")]
    Ffmpeg(String),
    #[error("FFmpeg not found. Please install FFmpeg or configure the path.")]
    FfmpegNotFound,
    #[error("Encoding failed: {0}")]
    EncodingFailed(String),
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
    #[error("Job not found: {0}")]
    JobNotFound(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("{0}")]
    Internal(String),
}

// 为 Tauri 命令返回值实现 Serialize 特征
impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl From<String> for AppError {
    fn from(s: String) -> Self {
        AppError::Internal(s)
    }
}

pub type AppResult<T> = Result<T, AppError>;

/// 取错误展示文本的首行（多行错误只记首行，避免全路径/body 噪音进日志）。
pub trait LogDisplay {
    fn top_line(&self) -> String;
}

impl<T: std::fmt::Display> LogDisplay for T {
    fn top_line(&self) -> String {
        self.to_string().lines().next().unwrap_or("unknown").to_string()
    }
}

/// 统一展开 `spawn_blocking` 的双层 `Result`（JoinError / tauri::Error / 内层业务错误）。
/// 外层泛型为 `Display`（兼容 `tokio::task::JoinError` 与 `tauri::Error`），
/// 两层失败都记 `warn`（首行），调用方只需把 `String` 转为 `AppError`
///（`String -> AppError` 已有 `From`，可直接 `?`）。
pub fn flatten_blocking_result<T, E: std::fmt::Display, F: std::fmt::Display>(
    r: Result<Result<T, E>, F>,
) -> Result<T, String> {
    match r {
        Err(join) => {
            let top = join.top_line();
            log::warn!("blocking task join failed reason {top}");
            Err(join.to_string())
        }
        Ok(inner) => inner.map_err(|e| {
            let top = e.top_line();
            log::warn!("blocking task failed reason {top}");
            e.to_string()
        }),
    }
}
