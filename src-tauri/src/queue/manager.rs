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
    pub(crate) db: StdMutex<Connection>,  // std Mutex because Connection is Send but not Sync
    /// Per-job cancellation flags. Set by `cancel_job` and read by the encode
    /// worker, so a job cancelled before its ffmpeg child is registered (in the
    /// window between `dequeue_next` and PROCESSES.insert) still cancels.
    cancel_flags: RwLock<HashMap<String, Arc<AtomicBool>>>,
    /// Serializes `process_queue` loops so concurrent invocations (user button
    /// + auto-advance) can't over-spawn past max_concurrent.
    processing: tokio::sync::Mutex<()>,
    /// 队列级暂停开关：true 时 can_start 恒为 false，不再自动启动新任务；
    /// 正在编码的任务不受影响。仅运行态，不持久化（重启后默认恢复调度）。
    paused: RwLock<bool>,
}

impl QueueManager {
    pub fn new(db_path: &str) -> AppResult<Arc<Self>> {
        let db = Connection::open(db_path)
            .map_err(|e| crate::error::AppError::Internal(e.to_string()))?;

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
        ).map_err(|e| crate::error::AppError::Internal(e.to_string()))?;

        // 迁移：老库无 estimated_output_size / vmaf 列，补上（列已存在时 ALTER 报错属预期）
        for stmt in [
            "ALTER TABLE jobs ADD COLUMN input_size INTEGER",
            "ALTER TABLE jobs ADD COLUMN estimated_output_size INTEGER",
            "ALTER TABLE jobs ADD COLUMN output_size INTEGER",
            "ALTER TABLE jobs ADD COLUMN vmaf_score REAL",
            "ALTER TABLE jobs ADD COLUMN vmaf_detail TEXT",
        ] {
            if let Err(e) = db.execute(stmt, []) {
                if !e.to_string().contains("duplicate column") {
                    log::warn!("DB migration {stmt:?} failed: {e}");
                }
            }
        }

        let max_concurrent = settings::load_usize(&db, SETTINGS_KEY_MAX_CONCURRENT)
            .unwrap_or(DEFAULT_MAX_CONCURRENT);
        log::info!("QueueManager: {} restored jobs, max_concurrent={}", Self::load_jobs(&db).len(), max_concurrent);

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
            Err(_) => return vec![],
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
                        // Interrupted jobs are re-queued after restart
                        j.status = JobStatus::Pending;
                    }
                    j
                })
                .collect()
        })
        .unwrap_or_default()
    }

    // --- Public API ---
    //
    // 注：DB 写原语（save_job / delete_job_db）与历史查询已移入 `history.rs`，
    // 本文件只保留内存调度。

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
            jobs.push_back(job);
        }
        log::info!("Queue: added {} jobs", ids.len());
        ids
    }

    /// Remove jobs from the in-memory queue.
    ///
    /// Pending/Encoding entries are also deleted from the DB (otherwise they'd
    /// resurrect on restart); Finished entries (Completed / Failed / Cancelled)
    /// are kept — their DB rows are the History page's records, which is the
    /// only place that manages them (see `delete_history` / `clear_history`).
    pub fn remove_jobs(&self, ids: &[String]) {
        let mut jobs = self.jobs.write();
        for id in ids {
            let is_finished = jobs
                .iter()
                .find(|j| &j.id == id)
                .map(|j| matches!(
                    j.status,
                    JobStatus::Completed | JobStatus::Failed | JobStatus::Cancelled
                ))
                // Unknown id (e.g. only a History row remains): treat as
                // finished so we never delete a DB record the queue page
                // doesn't own.
                .unwrap_or(true);
            if !is_finished {
                self.delete_job_db(id);
            }
        }
        jobs.retain(|j| !ids.contains(&j.id));
    }

    /// Remove finished jobs (Completed / Failed / Cancelled) from the in-memory
    /// queue only. The DB records are intentionally kept — the History page
    /// reads from the same table and manages its entries via `delete_history` /
    /// `clear_history`, so the queue's "清除已完成" must not wipe them.
    pub fn clear_completed(&self) {
        let mut jobs = self.jobs.write();
        jobs.retain(|j| !matches!(
            j.status,
            JobStatus::Completed | JobStatus::Failed | JobStatus::Cancelled
        ));
    }

    pub fn update_progress(&self, job_id: &str, pct: f64) {
        if let Some(job) = self.jobs.write().iter_mut().find(|j| j.id == job_id) {
            job.progress = Some(pct);
        }
    }

    pub fn cancel_job(&self, job_id: &str) {
        // 1. Signal the job's cancel flag first — this covers the window where
        //    the ffmpeg child has not been registered yet (still queued on a
        //    blocking worker, or between dequeue and spawn). `start_encode`
        //    checks this flag before spawning the process.
        if let Some(flag) = self.cancel_flags.read().get(job_id) {
            flag.store(true, Ordering::Relaxed);
        }
        // 2. Kill the underlying ffmpeg process if it is already running
        let killed = crate::encoder::engine::cancel_process(job_id);

        if let Some(job) = self.jobs.write().iter_mut().find(|j| j.id == job_id) {
            if job.status == JobStatus::Pending || job.status == JobStatus::Encoding {
                job.status = JobStatus::Cancelled;
                job.completed_at = Some(chrono::Utc::now().to_rfc3339());
                self.save_job(job);
                crate::analytics::bump(&crate::analytics::COUNTERS.encode_cancelled, 1);
            }
        }

        if !killed {
            log::warn!(
                "cancel_job: no ffmpeg process registered yet for {}; cancel flag set so the job will not start",
                job_id
            );
        }
    }

    /// Re-queue a finished job (Failed / Cancelled) so it can be encoded again.
    /// Returns false if the job doesn't exist or isn't in a retryable state.
    pub fn retry_job(&self, job_id: &str) -> bool {
        let mut jobs = self.jobs.write();
        let Some(job) = jobs.iter_mut().find(|j| j.id == job_id) else {
            return false;
        };
        if !matches!(job.status, JobStatus::Failed | JobStatus::Cancelled) {
            return false;
        }
        job.status = JobStatus::Pending;
        job.error = None;
        job.completed_at = None;
        job.progress = None;
        job.output_size = None;
        self.save_job(job);
        crate::analytics::bump(&crate::analytics::COUNTERS.retries, 1);
        true
    }

    /// 完成收尾（`history.rs` 的回归测试经此路径覆盖 DB 落盘，crate 内可见）。
    pub(crate) fn complete_job(&self, job_id: &str, success: bool, error: Option<String>) {
        if let Some(job) = self.jobs.write().iter_mut().find(|j| j.id == job_id) {
            // Never overwrite a user-cancelled job
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
            self.save_job(job);
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
        log::info!("Queue: paused (auto-advance disabled)");
    }

    /// 解除队列暂停。返回解除前的状态，方便调用方判断是否需要重新拉起调度。
    pub fn resume_queue(&self) -> bool {
        let was = std::mem::replace(&mut *self.paused.write(), false);
        if was {
            log::info!("Queue: resumed");
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

    /// Current maximum number of concurrent encoding jobs.
    pub fn max_concurrent(&self) -> usize {
        *self.max_concurrent.read()
    }

    /// Update the concurrency limit. Clamped to 1..=16 and persisted so the
    /// choice survives app restarts. Only takes effect for jobs started after
    /// the change (already-running jobs are not affected).
    pub fn set_max_concurrent(&self, value: usize) -> usize {
        let clamped = value.clamp(1, 16);
        *self.max_concurrent.write() = clamped;
        self.set_setting_usize(SETTINGS_KEY_MAX_CONCURRENT, clamped);
        log::info!("Queue: max_concurrent set to {}", clamped);
        clamped
    }

    /// Core: process queue, starting jobs up to max_concurrent.
    /// After each job finishes, this method is called again to start the next.
    /// A per-manager lock serializes the check-then-act loop so concurrent
    /// invocations (the user's 开始执行 button racing auto-advance) can't
    /// over-spawn past max_concurrent.
    pub fn process_queue(self: &Arc<Self>, app_handle: AppHandle) {
        let qm = self.clone();

        tokio::spawn(async move {
            let _guard = qm.processing.lock().await;
            let mut started = 0usize;

            while qm.can_start() {
                let job = match qm.dequeue_next() {
                    Some(j) => j,
                    None => break,
                };

                let job_id = job.id.clone();
                let config = match &job.config {
                    Some(c) => c.clone(),
                    None => {
                        log::error!("Job {} has no config", job_id);
                        qm.complete_job(&job_id, false, Some("Missing config".into()));
                        continue;
                    }
                };

                let app = app_handle.clone();
                let manager = qm.clone();
                qm.inc_active();

                // Shared cancel flag: cancel_job sets it even before the ffmpeg
                // child exists; start_encode checks it before spawning.
                let cancel_flag = Arc::new(AtomicBool::new(false));
                qm.cancel_flags.write().insert(job_id.clone(), cancel_flag.clone());
                // Close the window between dequeue_next and flag registration:
                // cancel_job that ran in that gap couldn't find a flag, but did
                // mark the job Cancelled — honour it so the encode never starts.
                if qm.jobs.read().iter().any(|j| j.id == job_id && j.status == JobStatus::Cancelled) {
                    cancel_flag.store(true, Ordering::Relaxed);
                }

                // Spawn encoding on blocking thread
                tokio::task::spawn_blocking(move || {
                    log::info!("Queue executing job: {}", job_id);

                    // Run the encoding engine; surface its error instead of the
                    // misleading "Output file not created" for every failure.
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
                    // complete_job never overwrites a user-cancelled job
                    manager.complete_job(&job_id, success, error);
                    // Remove our cancel flag only — a retried job may have
                    // re-inserted a fresh flag under the same id.
                    {
                        let mut flags = manager.cancel_flags.write();
                        if let Some(flag) = flags.get(&job_id) {
                            if Arc::ptr_eq(flag, &cancel_flag) {
                                flags.remove(&job_id);
                            }
                        }
                    }
                    manager.dec_active();

                    // Emit updated queue state
                    let status = manager.get_status();
                    let _ = app.emit("queue://updated", &status);

                    // Auto-advance: process next pending job
                    manager.process_queue(app.clone());
                });

                started += 1;
            }

            if started == 0 {
                log::debug!("Queue: no jobs to process (active={})",
                    *qm.active_count.read());
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

    // 历史/持久化回归（增删查、分页、VMAF 回退）已移入 `history.rs` tests。

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

        // Set a custom value and verify it is applied
        assert_eq!(manager.set_max_concurrent(4), 4);
        assert_eq!(manager.max_concurrent(), 4);

        // Values outside 1..=16 are clamped
        assert_eq!(manager.set_max_concurrent(0), 1);
        assert_eq!(manager.set_max_concurrent(99), 16);

        // Persisted across restart
        drop(manager);
        let reopened = QueueManager::new(&db_path).unwrap();
        assert_eq!(reopened.max_concurrent(), 16);

        let _ = std::fs::remove_dir_all(&dir);
    }

    // 历史语义回归（clear/remove 后 DB 行保留）见 `history.rs` tests。

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

        // Cancel while the job is still Pending (no ffmpeg child exists yet).
        manager.cancel_job(&ids[0]);

        // dequeue_next must never start a Cancelled job — the encode would
        // otherwise run in the background while the UI shows 已取消.
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

        // Retry failed job
        assert!(manager.retry_job(&ids[0]));
        let job = manager.jobs.read().iter().find(|j| j.id == ids[0]).unwrap().clone();
        assert_eq!(job.status, JobStatus::Pending);
        assert!(job.error.is_none());
        assert!(job.completed_at.is_none());

        // Retry cancelled job (no ffmpeg process was ever started)
        assert!(manager.retry_job(&ids[1]));
        assert!(manager.jobs.read().iter().find(|j| j.id == ids[1]).unwrap().status == JobStatus::Pending);

        // Unknown id
        assert!(!manager.retry_job("nope"));

        let _ = std::fs::remove_dir_all(&dir);
    }

}
