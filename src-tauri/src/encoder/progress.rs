use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 通过 `encode://progress` 上报给前端的编码进度信息。
///
/// 进度数据源自 ffmpeg 机器可读的 `-progress pipe:1` 输出（在 `engine.rs` 中解析），
/// 而不是来自 stderr 上的易读统计信息（ffmpeg 仅在 stdio 为终端时输出 stderr 统计）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncodeProgress {
    pub job_id: String,
    pub file_name: String,
    pub fps: f64,
    pub bitrate: f64,
    pub elapsed: String,
    pub percentage: f64,
    pub speed: f64,
    /// out_time 原文（HH:MM:SS.micro）
    pub time: String,
}

/// 将 `out_time=HH:MM:SS.micro` 格式解析为秒数
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

/// 从 `1600.0kbits/s` 中提取数值部分
pub(crate) fn parse_bitrate_kbps(s: &str) -> f64 {
    s.chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect::<String>()
        .parse()
        .unwrap_or(0.0)
}

/// 根据完整的 `-progress` 键值对计算编码百分比
pub(crate) fn compute_percentage(kv: &HashMap<String, String>, total_duration: Option<f64>) -> f64 {
    match (
        kv.get("out_time").and_then(|s| out_time_to_seconds(s)),
        total_duration,
    ) {
        (Some(t), Some(d)) if d > 0.0 => (t / d * 100.0).min(99.9),
        _ => 0.0,
    }
}
