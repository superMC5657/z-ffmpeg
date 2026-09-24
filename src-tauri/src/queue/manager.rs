use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use parking_lot::RwLock;
use rusqlite::Connection;
use tauri::{AppHandle, Emitter};
use crate::encoder::engine;
use crate::encoder::codec::EncodeConfig;
use crate::error::AppResult;
use crate::queue::job::{EncodeJob, JobSnapshot, JobStatus, QueueStatus};

const DEFAULT_MAX_CONCURRENT: usize = 2;
use crate::queue::settings;
use crate::queue::settings::SETTINGS_KEY_MAX_CONCURRENT;

pub struct QueueManager {
    /// 内存队列与 DB 句柄由 `history` 模块的持久化方法共用（同 crate 可见）。
    pub(crate) jobs: RwLock<VecDeque<EncodeJob>>,
    active_count: RwLock<usize>,
    max_concurrent: RwLock<usize>,
    pub(crate) db: StdMutex<Connection>,  // 使用 std Mutex，因为 Connection 实现了 Send 但未实现 Sync
    /// 针对每个任务的取消标记。由 `cancel_job` 设置并由编码 worker 读取，
    /// 确保在 ffmpeg 子进程尚未注册的窗口期间（即 `dequeue_next` 与 PROCESSES.insert 之间）取消的任务仍能生效取消。
    cancel_flags: RwLock<HashMap<String, Arc<AtomicBool>>>,
    /// 串行化 `process_queue` 循环，防止并发调用（用户点击按钮与自动推进竞态）产生超出 max_concurrent 的任务。
    processing: tokio::sync::Mutex<()>,
    /// 队列级暂停开关：true 时 can_start 恒为 false，不再自动启动新任务；
    /// 正在编码的任务不受影响。仅运行态，不持久化（重启后默认恢复调度）。
    paused: RwLock<bool>,
}

impl QueueManager {
    pub fn new(db_path: &str) -> AppResult<Arc<Self>> {
        let db = Connection::open(db_path).map_err(|e| {
            let msg = e.to_string();
            let top = msg.lines().next().unwrap_or("unknown").to_string();
            log::error!("queue db open failed reason {top}");
            crate::error::AppError::Internal(msg)
        })?;

        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS jobs (
                id TEXT PRIMARY KEY,
                input_path TEXT NOT NULL,
                output_path TEXT NOT NULL,
                config_json TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'Pending',
                progress REAL,
                input_size INTEGER,
                estimated_output_size INTEGER,
                output_size INTEGER,
                vmaf_score REAL,
                vmaf_detail TEXT,
                created_at TEXT NOT NULL,
                started_at TEXT,
                completed_at TEXT,
                error TEXT
            );
            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );"
        ).map_err(|e| {
            let msg = e.to_string();
            let top = msg.lines().next().unwrap_or("unknown").to_string();
            log::error!("queue db init failed reason {top}");
            crate::error::AppError::Internal(msg)
        })?;

        let max_concurrent = settings::load_usize(&db, SETTINGS_KEY_MAX_CONCURRENT)
            .unwrap_or(DEFAULT_MAX_CONCURRENT);

        Ok(Arc::new(Self {
            jobs: RwLock::new(VecDeque::from(Self::load_jobs(&db))),
            active_count: RwLock::new(0),
            max_concurrent: RwLock::new(max_concurrent),
            db: StdMutex::new(db),
            cancel_flags: RwLock::new(HashMap::new()),
            processing: tokio::sync::Mutex::new(()),
            paused: RwLock::new(false),
        }))
    }

    /// 读取一个 usize 设置项（settings 表），缺失时返回默认值。
    pub fn get_setting_usize(&self, key: &str, default: usize) -> usize {
        let db = self.db.lock().unwrap();
        settings::load_usize(&db, key).unwrap_or(default)
    }

    /// 写入一个 usize 设置项（settings 表）。
    pub fn set_setting_usize(&self, key: &str, value: usize) {
        let db = self.db.lock().unwrap();
        settings::save_usize(&db, key, value);
    }

    fn load_jobs(db: &Connection) -> Vec<EncodeJob> {
        let mut stmt = match db.prepare(
            "SELECT id, input_path, output_path, config_json, status, progress,
                    input_size, estimated_output_size, output_size, vmaf_score, vmaf_detail, created_at, started_at, completed_at, error
             FROM jobs WHERE status IN ('Pending', 'Encoding')
             ORDER BY created_at ASC"
        ) {
            Ok(s) => s,
            Err(e) => {
                let msg = e.to_string();
                let top = msg.lines().next().unwrap_or("unknown").to_string();
                log::debug!("queue load jobs skipped reason {top}");
                return vec![];
            }
        };

        stmt.query_map([], |row| {
            let status_str: String = row.get(4)?;
            Ok(EncodeJob {
                id: row.get(0)?,
                input_path: row.get(1)?,
                output_path: row.get(2)?,
                config: serde_json::from_str(&row.get::<_, String>(3)?).ok(),
                status: JobStatus::from_str(&status_str),
                progress: row.get(5)?,
                input_size: row.get(6)?,
                estimated_output_size: row.get(7)?,
                output_size: row.get(8)?,
                vmaf_score: row.get(9)?,
                vmaf_detail: row.get(10)?,
                created_at: row.get(11)?,
                started_at: row.get(12)?,
                completed_at: row.get(13)?,
                error: row.get(14)?,
            })
        })
        .ok()
        .map(|rows| {
            rows.filter_map(|r| r.ok())
                .map(|mut j| {
                    if j.status == JobStatus::Encoding {
                        // 重启后被中断的任务重新排队
                        j.status = JobStatus::Pending;
                    }
                    j
                })
                .collect()
        })
        .unwrap_or_default()
    }

    // --- 公共 API ---

    pub fn add_jobs(&self, files: Vec<(String, String)>, config: EncodeConfig) -> Vec<String> {
        self.add_jobs_estimated(files, vec![], config)
    }

    /// 与 `add_jobs` 相同，但可附带每个文件的预估输出体积（字节）；
    /// `estimates` 长度可小于 `files`，缺失项按 None 处理。
    pub fn add_jobs_estimated(
        &self,
        files: Vec<(String, String)>,
        estimates: Vec<Option<u64>>,
        config: EncodeConfig,
    ) -> Vec<String> {
        let mut jobs = self.jobs.write();
        let mut ids = Vec::new();
        for ((input, output), estimate) in files
            .into_iter()
            .zip(estimates.into_iter().chain(std::iter::repeat(None)))
        {
            let mut job = EncodeJob::new(input, output, config.clone());
            // 入队时记录原始文件大小（stat，快）；文件已删/不可读时保持 None
            job.input_size = std::fs::metadata(&job.input_path).ok().map(|m| m.len());
            job.estimated_output_size = estimate;
            self.save_job(&job);
            ids.push(job.id.clone());
            // 入队只记一行：id + basename + 队列长度（禁 {:?} 全 dump，config 明细不落盘）
            log::info!("queue enqueued job {} file {} len={}", job.id, job.file_name(), jobs.len() + 1);
            jobs.push_back(job);
        }
        ids
    }

    /// 从内存队列中移除任务。
    ///
    /// Pending/Encoding 状态的条目会同时从数据库中删除（否则重启后会复活）；
    /// 已结束（Completed / Failed / Cancelled）的条目会被保留——其数据库记录属于历史页面管理，
    /// 仅由历史记录功能操作（参见 `delete_history` / `clear_history`）。
    pub fn remove_jobs(&self, ids: &[String]) {
        let mut jobs = self.jobs.write();
        log::info!("queue remove jobs count={} ids={:?}", ids.len(), ids);
        for id in ids {
            let is_finished = jobs
                .iter()
                .find(|j| &j.id == id)
                .map(|j| matches!(
                    j.status,
                    JobStatus::Completed | JobStatus::Failed | JobStatus::Cancelled
                ))
                // 未知 ID（例如仅在历史记录中残留）：视为已结束任务处理，
                // 绝不删除不属于队列页面拥有的数据库记录。
                .unwrap_or(true);
            if !is_finished {
                self.delete_job_db(id);
            }
        }
        jobs.retain(|j| !ids.contains(&j.id));
    }

    /// 仅从内存队列中移除已结束的任务（Completed / Failed / Cancelled）。
    /// 数据库记录会被特意保留——历史页面读取同一张表，
    /// 并通过 `delete_history` / `clear_history` 进行管理，因此队列的“清除已完成”绝不能抹除它们。
    pub fn clear_completed(&self) {
        let mut jobs = self.jobs.write();
        let before = jobs.len();
        jobs.retain(|j| !matches!(
            j.status,
            JobStatus::Completed | JobStatus::Failed | JobStatus::Cancelled
        ));
        let cleared = before - jobs.len();
        log::info!("queue clear completed count={cleared}");
    }

    /// 获取当前队列中待处理（Pending）或正在编码（Encoding）任务的输出路径，
    /// 用于新任务入队时重名去重，避免覆盖在途任务。
    pub fn get_active_output_paths(&self) -> Vec<String> {
        let jobs = self.jobs.read();
        jobs.iter()
            .filter(|j| matches!(j.status, JobStatus::Pending | JobStatus::Encoding))
            .map(|j| j.output_path.clone())
            .collect()
    }

    pub fn update_progress(&self, job_id: &str, pct: f64) {
        if let Some(job) = self.jobs.write().iter_mut().find(|j| j.id == job_id) {
            job.progress = Some(pct);
        }
    }

    pub fn cancel_job(&self, job_id: &str) {
        // 1. 先触发任务的取消标记——这覆盖了 ffmpeg 子进程尚未注册的时间窗口
        //    （仍在 blocking worker 队列中，或处于出队与启动之间）。
        //    `start_encode` 会在启动进程前检查此标记。
        if let Some(flag) = self.cancel_flags.read().get(job_id) {
            flag.store(true, Ordering::Relaxed);
        }
        // 2. 如果底层的 ffmpeg 进程已经在运行，则终止该进程
        crate::encoder::engine::cancel_process(job_id);

        if let Some(job) = self.jobs.write().iter_mut().find(|j| j.id == job_id) {
            if job.status == JobStatus::Pending || job.status == JobStatus::Encoding {
                // Running 取消由 engine 侧记 warn（pre-spawn/post-spawn/running），
                // 这里只补 Pending（未进 engine）的行为盲区，避免双记。
                let was_pending = job.status == JobStatus::Pending;
                let id = job.id.clone();
                job.status = JobStatus::Cancelled;
                job.completed_at = Some(chrono::Utc::now().to_rfc3339());
                self.save_job(job);
                if was_pending {
                    log::warn!("queue cancelled job {id}");
                }
                crate::analytics::bump(&crate::analytics::COUNTERS.encode_cancelled, 1);
            }
        }
    }

    /// 将已结束的任务（Failed / Cancelled）重新加入队列，以便重新编码。
    /// 若任务不存在或不处于可重试状态，则返回 false。
    pub fn retry_job(&self, job_id: &str) -> bool {
        let mut jobs = self.jobs.write();
        let Some(job) = jobs.iter_mut().find(|j| j.id == job_id) else {
            return false;
        };
        if !matches!(job.status, JobStatus::Failed | JobStatus::Cancelled) {
            return false;
        }
        // 手动重进队列：原因取上次失败的 error 首行（basename 不记全路径）
        let reason = job.error.clone().unwrap_or_default();
        let top = reason.lines().next().unwrap_or("unknown").to_string();
        let name = job.file_name();
        let id = job.id.clone();
        job.status = JobStatus::Pending;
        job.error = None;
        job.completed_at = None;
        job.progress = None;
        job.output_size = None;
        self.save_job(job);
        log::warn!("queue retry job {id} file {name} reason {top}");
        crate::analytics::bump(&crate::analytics::COUNTERS.retries, 1);
        true
    }

    /// 完成收尾（`history.rs` 的回归测试经此路径覆盖 DB 落盘，crate 内可见）。
    pub(crate) fn complete_job(&self, job_id: &str, success: bool, error: Option<String>) {
        if let Some(job) = self.jobs.write().iter_mut().find(|j| j.id == job_id) {
            // 绝不覆盖已被用户取消的任务
            if job.status == JobStatus::Cancelled {
                return;
            }
            job.status = if success { JobStatus::Completed } else { JobStatus::Failed };
            job.completed_at = Some(chrono::Utc::now().to_rfc3339());
            crate::analytics::bump(
                if success {
                    &crate::analytics::COUNTERS.encode_completed
                } else {
                    &crate::analytics::COUNTERS.encode_failed
                },
                1,
            );
            if success {
                // 完成时读取实际输出体积（读不到则保留 None，仅影响展示）
                job.output_size = std::fs::metadata(&job.output_path).ok().map(|m| m.len());
            }
            if error.is_some() { job.error = error; }
            // 失败即终态 Failed（本项目无自动重试，重试仅用户手动 retry_job）
            if !success {
                let reason = job.error.clone().unwrap_or_default();
                let top = reason.lines().next().unwrap_or("unknown").to_string();
                let id = job.id.clone();
                let name = job.file_name();
                self.save_job(job);
                log::error!("queue failed job {id} file {name} reason {top}");
            } else {
                self.save_job(job);
            }
        }
    }

    pub fn get_status(&self) -> QueueStatus {
        let jobs = self.jobs.read();
        let snapshots: Vec<JobSnapshot> = jobs.iter().map(JobSnapshot::from).collect();
        let pending = jobs.iter().filter(|j| j.status == JobStatus::Pending).count();
        let encoding = jobs.iter().filter(|j| j.status == JobStatus::Encoding).count();
        let completed = jobs.iter().filter(|j| j.status == JobStatus::Completed).count();
        let failed = jobs.iter().filter(|j| j.status == JobStatus::Failed).count();
        QueueStatus {
            total: jobs.len(),
            pending,
            encoding,
            completed,
            failed,
            paused: *self.paused.read(),
            jobs: snapshots,
        }
    }

    /// 队列级暂停：暂停自动调度（正在编码的任务继续到结束）。
    pub fn pause_queue(&self) {
        *self.paused.write() = true;
        log::debug!("queue paused");
    }

    /// 解除队列暂停。返回解除前的状态，方便调用方判断是否需要重新拉起调度。
    pub fn resume_queue(&self) -> bool {
        let was = std::mem::replace(&mut *self.paused.write(), false);
        if was {
            log::debug!("queue resumed");
        }
        was
    }

    pub fn is_paused(&self) -> bool {
        *self.paused.read()
    }

    // 历史/持久化方法（get_job_paths / set_vmaf_score / history /
    // history_filtered / delete_history / clear_history / save_job /
    // delete_job_db）见 `history.rs`：DB 行比内存队列活得更久，单独成模块。

    fn dequeue_next(&self) -> Option<EncodeJob> {
        let mut jobs = self.jobs.write();
        let pos = jobs.iter().position(|j| j.status == JobStatus::Pending)?;
        let job = jobs.get_mut(pos)?;
        job.status = JobStatus::Encoding;
        job.started_at = Some(chrono::Utc::now().to_rfc3339());
        let job = job.clone();
        self.save_job(&job);
        Some(job)
    }

    fn can_start(&self) -> bool {
        // 队列暂停时不启动任何新任务（正在编码的不受影响）
        !self.is_paused() && *self.active_count.read() < *self.max_concurrent.read()
    }

    fn inc_active(&self) { *self.active_count.write() += 1; }
    fn dec_active(&self) { let mut c = self.active_count.write(); if *c > 0 { *c -= 1; } }

    /// 当前最大并发编码任务数。
    pub fn max_concurrent(&self) -> usize {
        *self.max_concurrent.read()
    }

    /// 更新并发数限制。限制在 1..=16 范围并持久化，使选择在应用重启后依然有效。
    /// 仅对修改后启动的任务生效（已在运行的任务不受影响）。
    pub fn set_max_concurrent(&self, value: usize) -> usize {
        let clamped = value.clamp(1, 16);
        *self.max_concurrent.write() = clamped;
        self.set_setting_usize(SETTINGS_KEY_MAX_CONCURRENT, clamped);
        clamped
    }

    /// 核心逻辑：处理队列，启动最多 max_concurrent 个任务。
    /// 每个任务结束后，会再次调用此方法启动下一个任务。
    /// 实例级锁串行化了“检查-执行”循环，防止并发调用（用户“开始执行”按钮与自动推进竞态）突破最大并发数。
    pub fn process_queue(self: &Arc<Self>, app_handle: AppHandle) {
        let qm = self.clone();

        tokio::spawn(async move {
            let _guard = qm.processing.lock().await;

            while qm.can_start() {
                let job = match qm.dequeue_next() {
                    Some(j) => j,
                    None => break,
                };

                let job_id = job.id.clone();
                let config = match &job.config {
                    Some(c) => c.clone(),
                    None => {
                        qm.complete_job(&job_id, false, Some("Missing config".into()));
                        continue;
                    }
                };

                let app = app_handle.clone();
                let manager = qm.clone();
                qm.inc_active();
                log::debug!(
                    "queue dispatching job {job_id} file {} active={} max_concurrent={}",
                    job.file_name(),
                    *qm.active_count.read(),
                    *qm.max_concurrent.read()
                );

                // 共享取消标记：即使在 ffmpeg 子进程创建之前，cancel_job 也可以设置该标记；
                // start_encode 会在派生进程前进行检查。
                let cancel_flag = Arc::new(AtomicBool::new(false));
                qm.cancel_flags.write().insert(job_id.clone(), cancel_flag.clone());
                // 消除 dequeue_next 与标记注册之间的时间窗口：
                // 如果 cancel_job 在该间隙运行而未找到标记，但已将任务标记为 Cancelled，
                // 则在此予以识别，阻止编码启动。
                if qm.jobs.read().iter().any(|j| j.id == job_id && j.status == JobStatus::Cancelled) {
                    cancel_flag.store(true, Ordering::Relaxed);
                }

                // 在 blocking 线程池中启动编码
                tokio::task::spawn_blocking(move || {
                    // 运行编码引擎；遇到失败时抛出具体错误，而不是对所有失败都显示具误导性的“未创建输出文件”。
                    let result = engine::start_encode(
                        app.clone(),
                        job_id.clone(),
                        config.clone(),
                        job.input_path.clone(),
                        job.output_path.clone(),
                        cancel_flag.clone(),
                    );

                    let output_exists = std::path::Path::new(&job.output_path).exists();
                    let (success, error) = match result {
                        Ok(()) if output_exists => (true, None),
                        Ok(()) => (false, Some("Output file not created".into())),
                        Err(e) => (false, Some(e.to_string())),
                    };
                    // complete_job 绝不覆盖已被用户取消的任务
                    manager.complete_job(&job_id, success, error);
                    // 仅移除我们自己的取消标记——被重试的任务可能已在相同 ID 下插入了新标记。
                    {
                        let mut flags = manager.cancel_flags.write();
                        if let Some(flag) = flags.get(&job_id) {
                            if Arc::ptr_eq(flag, &cancel_flag) {
                                flags.remove(&job_id);
                            }
                        }
                    }
                    manager.dec_active();

                    // 发送更新后的队列状态事件
                    let status = manager.get_status();
                    let _ = app.emit("queue://updated", &status);

                    // 自动推进：处理下一个待处理任务
                    manager.process_queue(app.clone());
                });
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoder::codec::EncodeConfig;

    fn sample_config() -> EncodeConfig {
        serde_json::from_str(
            r#"{"videoCodec":"H264","videoSettings":{"rateControl":{"type":"CRF","value":23},"encoderPreset":"medium","resolution":null,"frameRate":null,"pixelFormat":null,"profile":null,"additionalParams":[]},"audioSettings":{"codec":"AAC","bitrateKbps":192,"channels":2,"sampleRate":48000},"containerFormat":"MP4","hwAccel":null}"#,
        ).unwrap()
    }

    #[test]
    fn queue_pause_blocks_scheduling_until_resume() {
        let dir = std::env::temp_dir().join(format!("z-ffmpeg_qtest_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("queue.db").to_string_lossy().to_string();

        let manager = QueueManager::new(&db_path).unwrap();
        manager.add_jobs(
            vec![("C:\\in\\a.mp4".into(), "C:\\in\\a_encoded.mp4".into())],
            sample_config(),
        );

        assert!(!manager.is_paused());
        assert!(manager.can_start());
        assert!(!manager.get_status().paused);

        manager.pause_queue();
        assert!(manager.is_paused());
        assert!(manager.get_status().paused);
        assert!(!manager.can_start(), "暂停后不应再启动新任务");
        // Pending 任务本身不受影响，仍在队列中等待恢复
        assert_eq!(manager.get_status().pending, 1);

        manager.resume_queue();
        assert!(!manager.is_paused());
        assert!(manager.can_start(), "恢复后应能继续调度");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn max_concurrent_is_persisted_and_clamped() {
        let dir = std::env::temp_dir().join(format!("z-ffmpeg_qtest_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("queue.db").to_string_lossy().to_string();

        let manager = QueueManager::new(&db_path).unwrap();
        assert_eq!(manager.max_concurrent(), 2, "default should be 2");

        // 设置自定义值并验证是否生效
        assert_eq!(manager.set_max_concurrent(4), 4);
        assert_eq!(manager.max_concurrent(), 4);

        // 超出 1..=16 范围的值会被截断限制
        assert_eq!(manager.set_max_concurrent(0), 1);
        assert_eq!(manager.set_max_concurrent(99), 16);

        // 重启后保持持久化
        drop(manager);
        let reopened = QueueManager::new(&db_path).unwrap();
        assert_eq!(reopened.max_concurrent(), 16);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cancelled_pending_job_is_never_dequeued() {
        let dir = std::env::temp_dir().join(format!("z-ffmpeg_qtest_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("queue.db").to_string_lossy().to_string();

        let manager = QueueManager::new(&db_path).unwrap();
        let ids = manager.add_jobs(
            vec![("C:\\in\\a.mp4".into(), "C:\\in\\a_encoded.mp4".into())],
            sample_config(),
        );

        // 当任务仍处于 Pending 状态时取消（此时尚未创建 ffmpeg 子进程）。
        manager.cancel_job(&ids[0]);

        // dequeue_next 绝不能启动已取消（Cancelled）的任务——
        // 否则 UI 显示“已取消”时后台却仍在运行编码。
        assert!(manager.dequeue_next().is_none(), "cancelled job must not be dequeued");
        let job = manager.jobs.read().iter().find(|j| j.id == ids[0]).unwrap().clone();
        assert_eq!(job.status, JobStatus::Cancelled);
        assert!(job.completed_at.is_some());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn retry_job_requeues_failed_and_cancelled() {
        let dir = std::env::temp_dir().join(format!("z-ffmpeg_qtest_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("queue.db").to_string_lossy().to_string();

        let manager = QueueManager::new(&db_path).unwrap();
        let ids = manager.add_jobs(
            vec![
                ("C:\\in\\a.mp4".into(), "C:\\in\\a_encoded.mp4".into()),
                ("C:\\in\\b.mp4".into(), "C:\\in\\b_encoded.mp4".into()),
                ("C:\\in\\c.mp4".into(), "C:\\in\\c_encoded.mp4".into()),
            ],
            sample_config(),
        );
        manager.complete_job(&ids[0], false, Some("boom".into()));
        manager.cancel_job(&ids[1]);
        assert!(!manager.retry_job(&ids[2]), "pending jobs are not retryable");

        // 重试失败的任务
        assert!(manager.retry_job(&ids[0]));
        let job = manager.jobs.read().iter().find(|j| j.id == ids[0]).unwrap().clone();
        assert_eq!(job.status, JobStatus::Pending);
        assert!(job.error.is_none());
        assert!(job.completed_at.is_none());

        // 重试已取消的任务（未曾启动过 ffmpeg 进程）
        assert!(manager.retry_job(&ids[1]));
        assert!(manager.jobs.read().iter().find(|j| j.id == ids[1]).unwrap().status == JobStatus::Pending);

        // 未知 ID
        assert!(!manager.retry_job("nope"));

        let _ = std::fs::remove_dir_all(&dir);
    }

}
