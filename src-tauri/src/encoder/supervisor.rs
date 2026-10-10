//! 进程监管：ffmpeg 子进程生命周期的可测试接口。
//!
//! 收敛前取消正确性散在三处：`QueueManager::cancel_job` 设 flag、
//! `engine::cancel_process` 经 PROCESSES 全局注册表 kill、
//! `start_encode` 内三处检查（pre-spawn / post-spawn / running loop）
//! 各带 emit+cleanup+log 分支，外加 manager 侧 `Arc::ptr_eq` 移除舞蹈。
//! 本模块唯一拥有：job-handle 注册表（spawn / poll-progress-block /
//! cancel / wait）、取消状态机（requested → killed → reaped →
//! emitted，恰好一次终态）、进度块纯拼装（输入 -progress kv 行，
//! 输出进度事件，不依赖子进程即可测试）。一律不提 Tauri 类型；
//! `engine.rs` 只做事件→emit 薄映射。
//!
//! TODO: VMAF 的 `run_segment`（阻塞式 `output()` 取全量结果，非
//! -progress 流）暂不迁移；其行为不受本模块影响。

use std::collections::HashMap;
use std::io::BufRead;
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use crate::encoder::progress::{EncodeProgress, compute_percentage, parse_bitrate_kbps};

// ============================================================
// 取消状态机：requested → killed → reaped → emitted（恰好一次终态）
// ============================================================

/// 取消在哪一阶段赢得竞态。`as_str` 与 engine 日志中的 stage 字面量逐字一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelPoint {
    PreSpawn,
    PostSpawn,
    Running,
}

impl CancelPoint {
    pub fn as_str(&self) -> &'static str {
        match self {
            CancelPoint::PreSpawn => "pre-spawn",
            CancelPoint::PostSpawn => "post-spawn",
            CancelPoint::Running => "running",
        }
    }
}

/// 单次编码运行的监管状态：携带取消 flag 与“终态已认领”位。
/// 三处取消分支（pre-spawn / post-spawn / running）都经
/// `claim_cancellation` 认领，首个命中的分支产出唯一终态事件，
/// 后续分支一律让行——原来三分支各带 emit 分支的重复收进一处。
pub struct Supervision {
    cancel: Arc<AtomicBool>,
    start: Instant,
    terminal_taken: bool,
}

impl Supervision {
    pub fn new(cancel: Arc<AtomicBool>) -> Self {
        Self {
            cancel,
            start: Instant::now(),
            terminal_taken: false,
        }
    }

    pub fn cancel_flag(&self) -> Arc<AtomicBool> {
        self.cancel.clone()
    }

    pub fn is_cancel_requested(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// 认领终态：仅当“已请求取消且终态尚未被认领”时返回 `Some(point)`；
    /// 调用方在 `Some` 时产出终态事件（emit + cleanup + log），`None` 时继续。
    pub fn claim_cancellation(&mut self, point: CancelPoint) -> Option<CancelPoint> {
        if self.terminal_taken {
            return None;
        }
        if self.is_cancel_requested() {
            self.terminal_taken = true;
            Some(point)
        } else {
            None
        }
    }

    pub fn elapsed_secs(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }
}

// ============================================================
// 终态数据（Tauri-free；engine 映射为 EncodeResult 后 emit）
// ============================================================

/// 终态事件数据，与 `engine::EncodeResult` 逐字段对应。
#[derive(Debug, Clone, PartialEq)]
pub struct TerminalOutcome {
    pub job_id: String,
    pub file_name: String,
    pub success: bool,
    pub output_path: Option<String>,
    pub output_size_bytes: Option<u64>,
    pub duration_seconds: f64,
    pub cancelled: bool,
    pub error: Option<String>,
}

impl TerminalOutcome {
    /// 取消路径的统一载荷：三处取消分支共用（与搬运前 `EncodeResult::cancelled` 逐字段一致）。
    pub fn cancelled(job_id: String, file_name: String, duration_seconds: f64) -> Self {
        Self {
            job_id,
            file_name,
            success: false,
            output_path: None,
            output_size_bytes: None,
            duration_seconds,
            cancelled: true,
            error: Some("用户已取消".into()),
        }
    }

    pub fn completed(
        job_id: String,
        file_name: String,
        output_path: String,
        output_size: u64,
        duration_seconds: f64,
    ) -> Self {
        Self {
            job_id,
            file_name,
            success: true,
            output_path: Some(output_path),
            output_size_bytes: Some(output_size),
            duration_seconds,
            cancelled: false,
            error: None,
        }
    }

    pub fn failed(job_id: String, file_name: String, duration_seconds: f64, error: String) -> Self {
        Self {
            job_id,
            file_name,
            success: false,
            output_path: None,
            output_size_bytes: None,
            duration_seconds,
            cancelled: false,
            error: Some(error),
        }
    }
}

// ============================================================
// 进度块：poll（ feeding -progress 行）+ 纯拼装（无子进程可测）
// ============================================================

/// `-progress` 流的块轮询器：累积 `key=value` 行，直到 `progress=` 行
/// 组成完整一块。取消检查逐行进行，与搬运前读取循环的检查点一致。
pub struct BlockPoller<R> {
    reader: R,
    kv: HashMap<String, String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PollOutcome {
    /// 一块完整报告；`is_end` 对应 `progress=end`。
    Block { is_end: bool },
    /// 读取错误或流结束（调用方按原语义 break）。
    Eof,
    /// 行间检测到取消请求（调用方按原语义 break，终态由状态机认领）。
    Cancelled,
}

impl<R: BufRead> BlockPoller<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            kv: HashMap::new(),
        }
    }

    pub fn kv(&self) -> &HashMap<String, String> {
        &self.kv
    }

    pub fn clear_block(&mut self) {
        self.kv.clear();
    }

    pub fn poll_block(&mut self, supervision: &Supervision) -> PollOutcome {
        for line in self.reader.by_ref().lines() {
            if supervision.is_cancel_requested() {
                return PollOutcome::Cancelled;
            }

            let line = match line {
                Ok(l) => l,
                Err(_) => break,
            };

            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim().to_string();
            let value = value.trim().to_string();

            if key == "progress" {
                return PollOutcome::Block {
                    is_end: value == "end",
                };
            }
            self.kv.insert(key, value);
        }
        PollOutcome::Eof
    }
}

/// 从一段完整 `-progress` kv 块拼装进度事件（纯函数，与搬运前循环体内构造逐字段一致）。
pub fn assemble_progress_event(
    job_id: &str,
    file_name: &str,
    kv: &HashMap<String, String>,
    total_duration: Option<f64>,
    elapsed_secs: u64,
) -> EncodeProgress {
    let percentage = compute_percentage(kv, total_duration);
    let total_size_kb = kv
        .get("total_size")
        .and_then(|s| s.parse::<u64>().ok())
        .map(|b| b / 1024)
        .unwrap_or(0);
    EncodeProgress {
        job_id: job_id.to_string(),
        file_name: file_name.to_string(),
        frame: kv.get("frame").and_then(|s| s.parse().ok()).unwrap_or(0),
        fps: kv.get("fps").and_then(|s| s.parse().ok()).unwrap_or(0.0),
        bitrate: kv.get("bitrate").map(|s| parse_bitrate_kbps(s)).unwrap_or(0.0),
        total_size_kb,
        // 线性外推预估最终体积；进度为 0（编码刚开始）时没有可靠估算
        estimated_size_kb: (total_size_kb > 0 && percentage > 0.0)
            .then(|| (total_size_kb as f64 * 100.0 / percentage) as u64),
        elapsed: format!(
            "{:02}:{:02}:{:02}",
            elapsed_secs / 3600,
            (elapsed_secs % 3600) / 60,
            elapsed_secs % 60,
        ),
        percentage,
        speed: kv
            .get("speed")
            .and_then(|s| s.trim_end_matches('x').parse().ok())
            .unwrap_or(0.0),
        stage: "encoding".into(),
        time: kv.get("out_time").cloned().unwrap_or_default(),
    }
}

/// 25% 里程碑判定（纯函数，与搬运前 `pct_u32 >= last+25 && pct<100` 一致）。
/// 命中时返回新的里程碑值（调用方记 log），未命中返回 None。
pub fn next_milestone(last_milestone: u32, percentage: f64) -> Option<u32> {
    let pct_u32 = percentage.floor() as u32;
    if pct_u32 >= last_milestone + 25 && pct_u32 < 100 {
        Some((pct_u32 / 25) * 25)
    } else {
        None
    }
}

// ============================================================
// job-handle 注册表：spawn / cancel / wait（Tauri-free）
// ============================================================

struct JobHandle {
    cancel: Arc<AtomicBool>,
    child: Option<Child>,
}

static JOBS: OnceLock<Mutex<HashMap<String, JobHandle>>> = OnceLock::new();

fn jobs() -> &'static Mutex<HashMap<String, JobHandle>> {
    JOBS.get_or_init(Default::default)
}

fn take_handle(job_id: &str) -> Option<JobHandle> {
    jobs().lock().unwrap().remove(job_id)
}

/// 调度时注册取消标记（子进程尚未 spawn 的窗口也可用 `request_cancel` 取消）。
pub fn dispatch_register(job_id: String, cancel: Arc<AtomicBool>) {
    jobs()
        .lock()
        .unwrap()
        .insert(job_id, JobHandle { cancel, child: None });
}

/// spawn 后挂接子进程（管道须在此之前取走，与搬运前顺序一致）。
/// 调度已注册时只挂接 child（保留原 cancel 标记，不断 ptr_eq 链）；
/// 未注册时兜底建 handle（与搬运前“engine 必注册”语义一致）。
pub fn attach_child(job_id: &str, cancel: Arc<AtomicBool>, child: Child) {
    let mut map = jobs().lock().unwrap();
    match map.get_mut(job_id) {
        Some(handle) => {
            handle.child = Some(child);
        }
        None => {
            map.insert(
                job_id.to_string(),
                JobHandle {
                    cancel,
                    child: Some(child),
                },
            );
        }
    }
}

/// 请求取消：置位标记并 kill 已挂接的子进程（即使 ffmpeg 无输出也立即生效）。
/// 返回是否找到该任务的 handle（调用方忽略，与搬运前一致）。
pub fn request_cancel(job_id: &str) -> bool {
    if let Ok(mut map) = jobs().lock() {
        if let Some(handle) = map.get_mut(job_id) {
            handle.cancel.store(true, Ordering::Relaxed);
            if let Some(child) = handle.child.as_mut() {
                let _ = child.kill();
            }
            return true;
        }
    }
    false
}

/// post-spawn 取消路径：取走 handle 并 kill + wait（wait 结果丢弃，与搬运前一致）。
pub fn kill_and_reap(job_id: &str) {
    if let Some(mut handle) = take_handle(job_id) {
        if let Some(mut child) = handle.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// 收尾时取走子进程并 wait（`None` 表示中途已被取走，调用方按原语义走失败分支）。
pub fn reap_child(job_id: &str) -> Option<std::process::ExitStatus> {
    take_handle(job_id).and_then(|mut handle| handle.child.as_mut().and_then(|c| c.wait().ok()))
}

/// 仅移除属于本次调度的 handle——被重试的任务可能已在相同 ID 下注册了新 handle，
/// 此时不得误删（与搬运前 `Arc::ptr_eq` 语义一致）。
pub fn finish_if_same(job_id: &str, expected: &Arc<AtomicBool>) {
    let mut map = jobs().lock().unwrap();
    if let Some(handle) = map.get(job_id) {
        if Arc::ptr_eq(&handle.cancel, expected) {
            map.remove(job_id);
        }
    }
}

/// 终止所有被监管的编码子进程（应用退出时调用）。
pub fn kill_all() {
    if let Ok(mut map) = jobs().lock() {
        for (_job_id, mut handle) in map.drain() {
            handle.cancel.store(true, Ordering::Relaxed);
            if let Some(mut child) = handle.child.take() {
                let _ = child.kill();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn supervision_with_flag(requested: bool) -> (Supervision, Arc<AtomicBool>) {
        let flag = Arc::new(AtomicBool::new(requested));
        let s = Supervision::new(flag.clone());
        (s, flag)
    }

    // ---- 状态机：各阶段取消恰好一个终态 ----

    #[test]
    fn fake_pre_spawn_cancel_yields_single_terminal() {
        let (mut s, _flag) = supervision_with_flag(true);
        assert_eq!(s.claim_cancellation(CancelPoint::PreSpawn), Some(CancelPoint::PreSpawn));
        assert_eq!(s.claim_cancellation(CancelPoint::PostSpawn), None);
        assert_eq!(s.claim_cancellation(CancelPoint::Running), None);

        let t = TerminalOutcome::cancelled("j1".into(), "a.mp4".into(), 2.0);
        assert!(t.cancelled);
        assert!(!t.success);
        assert_eq!(t.output_path, None);
        assert_eq!(t.output_size_bytes, None);
        assert_eq!(t.error.as_deref(), Some("用户已取消"));
    }

    #[test]
    fn fake_post_spawn_cancel_yields_single_terminal() {
        let (mut s, flag) = supervision_with_flag(false);
        assert_eq!(s.claim_cancellation(CancelPoint::PreSpawn), None);
        // probe/spawn 期间到达的取消在 post-spawn 点获胜
        flag.store(true, Ordering::Relaxed);
        assert_eq!(s.claim_cancellation(CancelPoint::PostSpawn), Some(CancelPoint::PostSpawn));
        assert_eq!(s.claim_cancellation(CancelPoint::Running), None);
    }

    #[test]
    fn fake_running_cancel_yields_single_terminal() {
        let (mut s, flag) = supervision_with_flag(false);
        assert_eq!(s.claim_cancellation(CancelPoint::PreSpawn), None);
        assert_eq!(s.claim_cancellation(CancelPoint::PostSpawn), None);
        // 运行中途到达的取消在 running 点获胜，且后续认领一律让行
        flag.store(true, Ordering::Relaxed);
        assert_eq!(s.claim_cancellation(CancelPoint::Running), Some(CancelPoint::Running));
        assert_eq!(s.claim_cancellation(CancelPoint::Running), None);
    }

    #[test]
    fn fake_no_cancel_never_claims_terminal() {
        let (mut s, _flag) = supervision_with_flag(false);
        assert_eq!(s.claim_cancellation(CancelPoint::PreSpawn), None);
        assert_eq!(s.claim_cancellation(CancelPoint::PostSpawn), None);
        assert_eq!(s.claim_cancellation(CancelPoint::Running), None);

        let t = TerminalOutcome::completed("j4".into(), "a.mp4".into(), "out.mp4".into(), 1024, 1.5);
        assert!(!t.cancelled);
        assert!(t.success);
        assert_eq!(t.output_size_bytes, Some(1024));

        let f = TerminalOutcome::failed("j4".into(), "a.mp4".into(), 1.5, "boom".into());
        assert!(!f.cancelled);
        assert!(!f.success);
        assert_eq!(f.error.as_deref(), Some("boom"));
    }

    #[test]
    fn cancel_point_names_match_engine_log_stages() {
        // engine 日志字面量 "stage pre-spawn/post-spawn/running" 由此锁死
        assert_eq!(CancelPoint::PreSpawn.as_str(), "pre-spawn");
        assert_eq!(CancelPoint::PostSpawn.as_str(), "post-spawn");
        assert_eq!(CancelPoint::Running.as_str(), "running");
    }

    // ---- fake child：无需 Tauri/ffmpeg 的 -progress 流驱动 ----

    fn scripted_stream() -> Cursor<Vec<u8>> {
        // 垃圾行（无 =）应被跳过；两块完整报告，第二块以 end 收尾
        Cursor::new(
            b"garbage line\nframe=1500\nfps=30.0\nbitrate=1600.0kbits/s\ntotal_size=2048000\nout_time=00:00:50.000000\nspeed=1.50x\nprogress=continue\nframe=3000\nprogress=end\n"
                .to_vec(),
        )
    }

    #[test]
    fn fake_child_blocks_poll_and_assemble_without_process() {
        let (s, _flag) = supervision_with_flag(false);
        let mut poller = BlockPoller::new(scripted_stream());

        assert_eq!(poller.poll_block(&s), PollOutcome::Block { is_end: false });
        let first = assemble_progress_event("j5", "a.mp4", poller.kv(), Some(100.0), 100);
        assert_eq!(first.frame, 1500);
        assert!((first.fps - 30.0).abs() < 1e-6);
        assert!((first.bitrate - 1600.0).abs() < 1e-6);
        assert_eq!(first.total_size_kb, 2000);
        // 50/100s → 50%，外推 2000*100/50 = 4000KB
        assert!((first.percentage - 50.0).abs() < 1e-6);
        assert_eq!(first.estimated_size_kb, Some(4000));
        assert_eq!(first.elapsed, "00:01:40");
        assert!((first.speed - 1.5).abs() < 1e-6);
        assert_eq!(first.stage, "encoding");
        assert_eq!(first.time, "00:00:50.000000");

        poller.clear_block();
        assert_eq!(poller.poll_block(&s), PollOutcome::Block { is_end: true });
        let second = assemble_progress_event("j5", "a.mp4", poller.kv(), Some(100.0), 200);
        assert_eq!(second.frame, 3000);
        // 缺字段退化为 0/空，与搬运前 unwrap_or 一致
        assert_eq!(second.fps, 0.0);
        assert_eq!(second.time, "");
        assert_eq!(poller.poll_block(&s), PollOutcome::Eof);
    }

    #[test]
    fn fake_child_mid_stream_cancel_stops_polling() {
        let (s, flag) = supervision_with_flag(false);
        let mut poller = BlockPoller::new(scripted_stream());
        assert_eq!(poller.poll_block(&s), PollOutcome::Block { is_end: false });
        flag.store(true, Ordering::Relaxed);
        assert_eq!(poller.poll_block(&s), PollOutcome::Cancelled);
    }

    #[test]
    fn progress_assembler_zero_progress_has_no_estimate() {
        let kv: HashMap<String, String> = [("frame", "10")].into_iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let p = assemble_progress_event("j7", "a.mp4", &kv, None, 5);
        assert_eq!(p.percentage, 0.0);
        assert_eq!(p.estimated_size_kb, None);
        assert_eq!(p.elapsed, "00:00:05");
    }

    #[test]
    fn milestone_boundaries_match_legacy_loop() {
        assert_eq!(next_milestone(0, 24.9), None);
        assert_eq!(next_milestone(0, 25.0), Some(25));
        assert_eq!(next_milestone(25, 50.0), Some(50));
        assert_eq!(next_milestone(25, 49.9), None);
        // 100% 不记里程碑（循环以 end 收尾）
        assert_eq!(next_milestone(75, 100.0), None);
        assert_eq!(next_milestone(75, 99.9), None);
        assert_eq!(next_milestone(50, 99.0), Some(75));
    }
}
