//! 会话聚合埋点：Rust 侧全局原子计数器，随命令处理累加；
//! 正常退出时一次性上报（见 `report.rs`）。失败静默，绝不影响应用功能。

pub mod report;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

/// 会话级计数器（进程内静态，随进程结束销毁——正好符合"会话聚合"语义）
pub struct Counters {
    pub files_added: AtomicU64,
    pub jobs_added: AtomicU64,
    pub encode_completed: AtomicU64,
    pub encode_failed: AtomicU64,
    pub encode_cancelled: AtomicU64,
    pub retries: AtomicU64,
    pub vmaf_runs: AtomicU64,
    pub presets_saved: AtomicU64,
    pub presets_imported: AtomicU64,
    pub presets_exported: AtomicU64,
    pub hw_accel_jobs: AtomicU64,
    pub ffmpeg_downloaded: AtomicU64,
    pub commands_exported: AtomicU64,
}

pub static COUNTERS: Counters = Counters {
    files_added: AtomicU64::new(0),
    jobs_added: AtomicU64::new(0),
    encode_completed: AtomicU64::new(0),
    encode_failed: AtomicU64::new(0),
    encode_cancelled: AtomicU64::new(0),
    retries: AtomicU64::new(0),
    vmaf_runs: AtomicU64::new(0),
    presets_saved: AtomicU64::new(0),
    presets_imported: AtomicU64::new(0),
    presets_exported: AtomicU64::new(0),
    hw_accel_jobs: AtomicU64::new(0),
    ffmpeg_downloaded: AtomicU64::new(0),
    commands_exported: AtomicU64::new(0),
};

/// 编码器使用分布（h264/h265/av1/vp9 → 次数）
fn codecs() -> &'static Mutex<HashMap<String, u64>> {
    static CODECS: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();
    CODECS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 纯 UI 行为事件计数（页面导航等，经 `track_event` 命令上报）
fn events() -> &'static Mutex<HashMap<String, u64>> {
    static EVENTS: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();
    EVENTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn add(map: &Mutex<HashMap<String, u64>>, key: &str, times: u64) {
    if let Ok(mut m) = map.lock() {
        *m.entry(key.to_string()).or_insert(0) += times;
    }
}

pub fn record_codec(codec: &str, times: u64) {
    add(codecs(), codec, times);
}

pub fn record_event(name: &str) {
    add(events(), name, 1);
}

pub fn bump(counter: &AtomicU64, times: u64) {
    counter.fetch_add(times, Ordering::Relaxed);
}

/// 会话开始时间（Unix 秒），首次调用时初始化
pub fn session_start() -> i64 {
    static SESSION_START: OnceLock<i64> = OnceLock::new();
    *SESSION_START.get_or_init(|| chrono::Utc::now().timestamp())
}

/// 读取 codecs / events 快照（上报用）
pub fn snapshots() -> (Vec<(String, u64)>, Vec<(String, u64)>) {
    let to_sorted = |m: &Mutex<HashMap<String, u64>>| {
        m.lock()
            .map(|m| {
                let mut v: Vec<(String, u64)> = m.iter().map(|(k, n)| (k.clone(), *n)).collect();
                v.sort();
                v
            })
            .unwrap_or_default()
    };
    (to_sorted(codecs()), to_sorted(events()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_and_maps_accumulate() {
        bump(&COUNTERS.files_added, 3);
        bump(&COUNTERS.files_added, 2);
        assert_eq!(COUNTERS.files_added.load(Ordering::Relaxed), 5);

        record_codec("h264", 4);
        record_codec("h264", 1);
        record_codec("h265", 2);
        record_event("page_view");

        let (codecs, events) = snapshots();
        assert!(codecs.contains(&("h264".into(), 5)));
        assert!(codecs.contains(&("h265".into(), 2)));
        assert!(events.contains(&("page_view".into(), 1)));
    }

    #[test]
    fn session_start_is_stable() {
        assert_eq!(session_start(), session_start());
    }
}
