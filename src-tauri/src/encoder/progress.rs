use serde::{Deserialize, Serialize};

/// Encoding progress reported to the frontend via `encode://progress`.
///
/// The source of truth is ffmpeg's machine-readable `-progress pipe:1`
/// output (parsed in `engine.rs`), not the human-readable stats on stderr
/// (which ffmpeg only emits when stdio is a terminal).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncodeProgress {
    pub job_id: String,
    pub file_name: String,
    pub frame: u64,
    pub fps: f64,
    pub bitrate: f64,
    pub total_size_kb: u64,
    /// 预估压缩后的输出体积（KB）：按已写出大小 / 当前进度线性外推；
    /// 进度未知（percentage 为 0 或无 total_size）时为 null，编码开始后才有值。
    pub estimated_size_kb: Option<u64>,
    pub elapsed: String,
    pub percentage: f64,
    pub speed: f64,
    pub stage: String, // "encoding", "complete", "error"
    pub time: String,
}
