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

// Implement Serialize for Tauri command results
impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
