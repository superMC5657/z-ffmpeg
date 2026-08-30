use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

/// Parse `out_time=HH:MM:SS.micro` into seconds
pub(crate) fn out_time_to_seconds(s: &str) -> Option<f64> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() == 3 {
        let h: f64 = parts[0].parse().ok()?;
        let m: f64 = parts[1].parse().ok()?;
        let sec: f64 = parts[2].parse().ok()?;
        Some(h * 3600.0 + m * 60.0 + sec)
    } else {
        None
    }
}

/// Extract the numeric part from `1600.0kbits/s`
pub(crate) fn parse_bitrate_kbps(s: &str) -> f64 {
    s.chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect::<String>()
        .parse()
        .unwrap_or(0.0)
}

/// Compute percentage from a completed `-progress` key/value block
pub(crate) fn compute_percentage(kv: &HashMap<String, String>, total_duration: Option<f64>) -> f64 {
    match (
        kv.get("out_time").and_then(|s| out_time_to_seconds(s)),
        total_duration,
    ) {
        (Some(t), Some(d)) if d > 0.0 => (t / d * 100.0).min(99.9),
        _ => 0.0,
    }
}
