use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use parking_lot::RwLock;
use tauri::{AppHandle, Emitter};
use crate::encoder::engine;
use crate::encoder::codec::EncodeConfig;
use crate::encoder::supervisor;
use crate::error::{AppResult, LogDisplay};
use crate::queue::job::{EncodeJob, JobSnapshot, JobStatus, QueueStatus};

const DEFAULT_MAX_CONCURRENT: usize = 2;
use crate::queue::store::{JobStore, SETTINGS_KEY_MAX_CONCURRENT};

pub struct QueueManager {
    /// 内存调度状态（私有：全部跨文件协作经 store 显式接口，不再跨 seam 互摸）。
    jobs: RwLock<VecDeque<EncodeJob>>,
    active_count: RwLock<usize>,
    max_concurrent: RwLock<usize>,
    /// 持久化 seam：全部 SQLite 访问归 store，manager 不再直接持有 Connection。
    store: JobStore,
    /// 调度期取消标记由 supervisor 注册表拥有（dispatch 时注册，完成后经
    /// `supervisor::finish_if_same` 按 `Arc::ptr_eq` 移除）；本地不再保留
    /// 第二份 flag 映射。`cancel_job` 经 supervisor 置位，编码 worker 经
    /// 传参 flag 读取，覆盖子进程尚未挂接的窗口。
    /// 串行化 `process_queue` 循环，防止并发调用（用户点击按钮与自动推进竞态）产生超出 max_concurrent 的任务。
    processing: tokio::sync::Mutex<()>,
    /// 队列级暂停开关：true 时 can_start 恒为 false，不再自动启动新任务；
    /// 正在编码的任务不受影响。仅运行态，不持久化（重启后默认恢复调度）。
    paused: RwLock<bool>,
}

impl QueueManager {
    pub fn new(db_path: &str) -> AppResult<Arc<Self>> {
        let store = JobStore::open(db_path)?;

        let max_concurrent = store
            .load_setting_usize(SETTINGS_KEY_MAX_CONCURRENT)
            .unwrap_or(DEFAULT_MAX_CONCURRENT);

        Ok(Arc::new(Self {
            jobs: RwLock::new(VecDeque::from(store.load_active_jobs())),
            active_count: RwLock::new(0),
            max_concurrent: RwLock::new(max_concurrent),
            store,
            processing: tokio::sync::Mutex::new(()),
            paused: RwLock::new(false),
        }))
    }

    /// 读取一个 usize 设置项（settings 表），缺失时返回默认值。
    pub fn get_setting_usize(&self, key: &str, default: usize) -> usize {
        self.store.load_setting_usize(key).unwrap_or(default)
    }

    /// 写入一个 usize 设置项（settings 表）。
    pub fn set_setting_usize(&self, key: &str, value: usize) {
        self.store.save_setting_usize(key, value);
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
            self.store.save_job(&job);
            ids.push(job.id.clone());
            // 入队只记一行：id + basename + 队列长度（禁 {:?} 全 dump，config 明细不落盘）
            log::info!("queue enqueued job {} file {} len={}", job.id, job.file_name(), jobs.len() + 1);
            jobs.push_back(job);
        }
        ids
    }

    /// 从内存队列中移除任务。
    ///
    /// DB 侧留存规则唯一拥有者为 `JobStore::remove_from_queue`
    ///（Pending/Encoding 行删、Finished 行留、未知 ID 不删库）；
    /// 此处只做内存 retain，不再手写状态分支。
    pub fn remove_jobs(&self, ids: &[String]) {
        let mut jobs = self.jobs.write();
        log::info!("queue remove jobs count={} ids={:?}", ids.len(), ids);
        for id in ids {
            let status = jobs.iter().find(|j| &j.id == id).map(|j| &j.status);
            self.store.remove_from_queue(id, status);
        }
        jobs.retain(|j| !ids.contains(&j.id));
    }

    /// 仅从内存队列中移除已结束的任务（Completed / Failed / Cancelled）。
    /// 数据库记录会被特意保留——历史页面读取 store，
    /// 并通过 `delete_history` / `clear_history` 进行管理，因此队列的“清除已完成”绝不能抹除它们。
    /// 留存三者分工（行为不变）：本方法仅内存 retain 不碰 DB；
    /// `store::clear_history_rows` 删库已结束行；`store::remove_from_queue`
    /// 按状态逐行删库（未知 ID 不删库）。详见 store 侧注释。
    pub fn clear_completed(&self) {
        let mut jobs = self.jobs.write();
        let before = jobs.len();
        jobs.retain(|j| !JobStore::is_finished(&j.status));
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
        // 取消语义归 supervisor 注册表：一处置位 flag（含子进程尚未挂接的
        // 窗口）+ kill 已挂接进程（直调 supervisor，语义与此前“先设 flag
        // 再 kill”一致）。恰好一次终态由编码侧状态机保证。
        supervisor::request_cancel(job_id);

        if let Some(job) = self.jobs.write().iter_mut().find(|j| j.id == job_id) {
            if job.status == JobStatus::Pending || job.status == JobStatus::Encoding {
                // Running 取消由 engine 侧记 warn（pre-spawn/post-spawn/running），
                // 这里只补 Pending（未进 engine）的行为盲区，避免双记。
                let was_pending = job.status == JobStatus::Pending;
                let id = job.id.clone();
                job.status = JobStatus::Cancelled;
                job.completed_at = Some(chrono::Utc::now().to_rfc3339());
                self.store.save_job(job);
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
        let top = reason.top_line();
        let name = job.file_name();
        let id = job.id.clone();
        job.status = JobStatus::Pending;
        job.error = None;
        job.completed_at = None;
        job.progress = None;
        job.output_size = None;
        self.store.save_job(job);
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
                let top = reason.top_line();
                let id = job.id.clone();
                let name = job.file_name();
                self.store.save_job(job);
                log::error!("queue failed job {id} file {name} reason {top}");
            } else {
                self.store.save_job(job);
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

    /// 广播最新队列状态（`queue://updated`），失败只记首行 warn。
    pub fn emit_updated(&self, app: &tauri::AppHandle) {
        let status = self.get_status();
        if let Err(e) = app.emit("queue://updated", &status) {
            let top = e.top_line();
            log::warn!("queue emit updated failed reason {top}");
        }
    }

    /// 为批量输入推导唯一输出路径（去重范围含磁盘文件、活跃任务与批次内占位）。
    pub fn resolve_unique_outputs(
        &self,
        files: &[String],
        config: &EncodeConfig,
        output_dir: Option<&str>,
    ) -> Vec<String> {
        let claimed = self.get_active_output_paths();
        crate::encoder::args::derive_output_paths_unique_with_claimed(
            files,
            config,
            output_dir,
            &claimed,
        )
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

    // --- 历史页：store 之上的薄编排（内存 retain + store 调用）。
    // --- 内存优先→DB 回退的分支真相归 store（`resolve_job_paths` /
    // --- `write_vmaf_score`），此处只做内存预查并透传，不手写回退分支。

    /// 从数据库中删除指定的历史条目（已完成 / 失败 / 已取消），
    /// 并从内存队列中同步移除（若存在）。
    pub fn delete_history(&self, ids: &[String]) {
        self.store.delete_history_rows(ids);
        let mut jobs = self.jobs.write();
        jobs.retain(|j| !ids.contains(&j.id));
    }

    /// 从数据库和内存队列中删除所有历史记录条目（已完成 / 失败 / 已取消）。
    /// 即使在重启后内存队列为空时也能正常生效（与 clear_completed 不同：
    /// 后者仅内存 retain 不碰 DB，本方法连库一起删已结束行）。
    pub fn clear_history(&self) {
        self.store.clear_history_rows();
        let mut jobs = self.jobs.write();
        jobs.retain(|j| !JobStore::is_finished(&j.status));
    }

    /// 取任务的输入/输出路径（VMAF 计算需要原始与压缩后的成对文件）。
    /// 先查内存队列；已完成且被移出内存的任务（clear_completed / 重启后）回退查 store，
    /// 保证历史任务仍可计算 VMAF。回退分支归 store，本方法只做内存预查。
    pub fn get_job_paths(&self, job_id: &str) -> Option<(String, String)> {
        let cached = self
            .jobs
            .read()
            .iter()
            .find(|j| j.id == job_id)
            .map(|j| (j.input_path.clone(), j.output_path.clone()));
        self.store.resolve_job_paths(cached, job_id)
    }

    /// 写入 VMAF 计算结果（平均分 + 各段明细 JSON）。
    /// 内存队列中的任务直接更新并经 store 持久化；已移出内存的任务（历史）直接 UPDATE store，
    /// 避免计算结果被静默丢弃。双写分支归 store，本方法只传内存持锁查到的借用。
    pub fn set_vmaf_score(&self, job_id: &str, score: f64, detail_json: Option<String>) {
        let mut jobs = self.jobs.write();
        let memory_job = jobs.iter_mut().find(|j| j.id == job_id);
        self.store.write_vmaf_score(memory_job, job_id, score, detail_json);
    }

    /// 直接从数据库加载历史条目（已完成 / 失败 / 已取消）。
    /// 与内存队列（启动时仅恢复活动任务）不同，数据库会持久保留已结束任务，
    /// 使历史记录在应用重启后依然有效。
    pub fn history(&self) -> Vec<JobSnapshot> {
        self.store.history()
    }

    /// 带筛选/搜索/分页的历史查询。status 过滤单个状态；search 对
    /// input_path 做 LIKE 匹配（%/_ 转义）；limit == None 表示不分页。
    /// 返回 (当前页条目, 匹配总数)，总数供前端计算页数。
    pub fn history_filtered(
        &self,
        limit: Option<usize>,
        offset: usize,
        status: Option<&str>,
        search: Option<&str>,
    ) -> (Vec<JobSnapshot>, usize) {
        self.store.history_filtered(limit, offset, status, search)
    }

    fn dequeue_next(&self) -> Option<EncodeJob> {
        let mut jobs = self.jobs.write();
        let pos = jobs.iter().position(|j| j.status == JobStatus::Pending)?;
        let job = jobs.get_mut(pos)?;
        job.status = JobStatus::Encoding;
        job.started_at = Some(chrono::Utc::now().to_rfc3339());
        let job = job.clone();
        self.store.save_job(&job);
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
                // start_encode 会在派生进程前进行检查。标记归 supervisor 注册表拥有。
                let cancel_flag = Arc::new(AtomicBool::new(false));
                supervisor::dispatch_register(job_id.clone(), cancel_flag.clone());
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
                    // 仅移除本次调度的注册——被重试的任务可能已在相同 ID 下注册了新标记，
                    // ptr_eq 语义收进 supervisor 一处。
                    supervisor::finish_if_same(&job_id, &cancel_flag);
                    manager.dec_active();

                    // 发送更新后的队列状态事件
                    manager.emit_updated(&app);

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

    // --- 历史页回归（原 `history.rs`，随实现并入本文件；行为不变） ---

    #[test]
    fn history_survives_restart() {
        let dir = std::env::temp_dir().join(format!("z-ffmpeg_qtest_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("queue.db").to_string_lossy().to_string();

        // 第一次“会话”：添加任务并完成它
        let manager = QueueManager::new(&db_path).unwrap();
        let ids = manager.add_jobs(
            vec![(
                dir.join("a.mp4").to_string_lossy().to_string(),
                dir.join("a_encoded.mp4").to_string_lossy().to_string(),
            )],
            sample_config(),
        );
        manager.complete_job(&ids[0], true, None);

        // 第二次“会话”：重启管理器——仅数据库数据保留
        drop(manager);
        let reopened = QueueManager::new(&db_path).unwrap();
        assert_eq!(reopened.jobs.read().len(), 0, "active queue should be empty after restart");

        let history = reopened.history();
        assert_eq!(history.len(), 1, "finished job must be visible in history");
        assert_eq!(history[0].status, "Completed");
        assert_eq!(history[0].file_name, "a.mp4");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn history_can_be_deleted_and_cleared() {
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
        manager.complete_job(&ids[0], true, None);
        manager.complete_job(&ids[1], false, Some("boom".into()));
        manager.complete_job(&ids[2], true, None);
        assert_eq!(manager.history().len(), 3);

        // 删除单个条目
        manager.delete_history(&[ids[1].clone()]);
        let history = manager.history();
        assert_eq!(history.len(), 2);
        assert!(history.iter().all(|h| h.id != ids[1]));

        // 清空剩余条目
        manager.clear_history();
        assert!(manager.history().is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn history_filtered_supports_status_search_and_pagination() {
        let dir = std::env::temp_dir().join(format!("z-ffmpeg_qtest_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("queue.db").to_string_lossy().to_string();

        let manager = QueueManager::new(&db_path).unwrap();
        let ids = manager.add_jobs(
            vec![
                (
                    dir.join("alpha.mp4").to_string_lossy().to_string(),
                    dir.join("alpha_encoded.mp4").to_string_lossy().to_string(),
                ),
                (
                    dir.join("beta.mp4").to_string_lossy().to_string(),
                    dir.join("beta_encoded.mp4").to_string_lossy().to_string(),
                ),
                (
                    dir.join("gamma.mp4").to_string_lossy().to_string(),
                    dir.join("gamma_encoded.mp4").to_string_lossy().to_string(),
                ),
                (
                    dir.join("delta.mp4").to_string_lossy().to_string(),
                    dir.join("delta_encoded.mp4").to_string_lossy().to_string(),
                ),
            ],
            sample_config(),
        );
        manager.complete_job(&ids[0], true, None);
        manager.complete_job(&ids[1], false, Some("boom".into()));
        manager.complete_job(&ids[2], true, None);
        manager.complete_job(&ids[3], true, None);

        // 不带条件 = 与 history() 等价
        let (all, total) = manager.history_filtered(None, 0, None, None);
        assert_eq!(total, 4);
        assert_eq!(all.len(), 4);

        // 状态过滤
        let (failed, total) = manager.history_filtered(None, 0, Some("Failed"), None);
        assert_eq!(total, 1);
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].status, "Failed");

        // 搜索（文件名按字面量匹配，% 不当通配符）
        let (hits, total) = manager.history_filtered(None, 0, None, Some("beta"));
        assert_eq!(total, 1);
        assert_eq!(hits[0].file_name, "beta.mp4");
        let (_, wildcard_total) = manager.history_filtered(None, 0, None, Some("%"));
        assert_eq!(wildcard_total, 0, "% 应被转义为字面量而非 LIKE 通配符");

        // 分页：limit=2 offset=0 / offset=2
        let (page0, total) = manager.history_filtered(Some(2), 0, None, None);
        assert_eq!(total, 4);
        assert_eq!(page0.len(), 2);
        let (page1, total) = manager.history_filtered(Some(2), 2, None, None);
        assert_eq!(total, 4);
        assert_eq!(page1.len(), 2);
        // 两页合起来覆盖全部 id
        let mut seen: Vec<&str> = page0.iter().chain(page1.iter()).map(|j| j.id.as_str()).collect();
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), 4, "分页不应重复或丢失条目");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn complete_job_records_output_size_and_survives_restart() {
        let dir = std::env::temp_dir().join(format!("z-ffmpeg_qtest_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("queue.db").to_string_lossy().to_string();
        let out_path = dir.join("out.mp4");

        let manager = QueueManager::new(&db_path).unwrap();
        let ids = manager.add_jobs(
            vec![("C:\\in\\a.mp4".into(), out_path.to_string_lossy().to_string())],
            sample_config(),
        );

        // 无输出文件时 output_size 保持 None
        manager.complete_job(&ids[0], true, None);
        assert_eq!(manager.history()[0].output_size, None);

        // 重建一个任务，写一个真实输出文件再完成 → 记录实际大小
        let ids2 = manager.add_jobs(
            vec![("C:\\in\\b.mp4".into(), out_path.to_string_lossy().to_string())],
            sample_config(),
        );
        std::fs::write(&out_path, vec![0u8; 4096]).unwrap();
        manager.complete_job(&ids2[0], true, None);
        assert_eq!(manager.history()[0].output_size, Some(4096));

        // 重启后大小保留在历史里
        drop(manager);
        let reopened = QueueManager::new(&db_path).unwrap();
        let hist = reopened.history();
        assert!(hist.iter().any(|h| h.id == ids2[0] && h.output_size == Some(4096)));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn vmaf_paths_and_score_fall_back_to_db_for_finished_jobs() {
        let dir = std::env::temp_dir().join(format!("z-ffmpeg_qtest_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("queue.db").to_string_lossy().to_string();

        let manager = QueueManager::new(&db_path).unwrap();
        let ids = manager.add_jobs(
            vec![("C:\\in\\a.mp4".into(), "C:\\out\\a_encoded.mp4".into())],
            sample_config(),
        );
        manager.complete_job(&ids[0], true, None);

        // 内存中可解析（已完成任务仍在内存）
        assert_eq!(
            manager.get_job_paths(&ids[0]),
            Some(("C:\\in\\a.mp4".into(), "C:\\out\\a_encoded.mp4".into()))
        );

        // 清除已完成 → 任务移出内存，但 DB 行保留
        manager.clear_completed();
        assert_eq!(manager.history().len(), 1);
        // 回退 DB 仍能解析路径
        assert_eq!(
            manager.get_job_paths(&ids[0]),
            Some(("C:\\in\\a.mp4".into(), "C:\\out\\a_encoded.mp4".into()))
        );

        // 重启后（内存无该任务）同样回退 DB 解析
        drop(manager);
        let reopened = QueueManager::new(&db_path).unwrap();
        assert!(reopened.get_job_paths(&ids[0]).is_some());

        // DB-only 任务写 VMAF 分数 → history 可读（不丢结果）
        reopened.set_vmaf_score(&ids[0], 91.25, Some(r#"{"mode":"sampled","scores":[90.1,92.4]}"#.into()));
        let hist = reopened.history();
        let entry = hist.iter().find(|h| h.id == ids[0]).unwrap();
        assert_eq!(entry.vmaf_score, Some(91.25));
        assert!(entry.vmaf_detail.is_some());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn clear_completed_keeps_history_records() {
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
        manager.complete_job(&ids[0], true, None);
        manager.complete_job(&ids[1], false, Some("boom".into()));
        // c 保持 Pending

        // 队列页的“清除已完成”仅从内存队列中移出已结束任务——历史页面读取同一张数据库表。
        manager.clear_completed();
        assert_eq!(manager.jobs.read().len(), 1, "only the pending job remains");
        assert_eq!(manager.jobs.read()[0].id, ids[2]);

        // 历史记录保持不受影响
        let history = manager.history();
        assert_eq!(history.len(), 2, "finished jobs must survive clear_completed");
        assert!(history.iter().any(|h| h.id == ids[0]));
        assert!(history.iter().any(|h| h.id == ids[1]));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn remove_jobs_keeps_history_records_and_drops_pending_db_rows() {
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
        manager.complete_job(&ids[0], true, None); // 已完成 -> 历史记录
        // b 保持 Pending，c 保持 Pending

        // 队列页移除一个已完成任务和一个待处理任务
        manager.remove_jobs(&[ids[0].clone(), ids[1].clone()]);

        // 历史记录中仍然保留该已完成记录
        let history = manager.history();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].id, ids[0]);

        // 待处理任务的数据库行已被删除：重启后不得复活，仅保留 c 任务排队。
        drop(manager);
        let reopened = QueueManager::new(&db_path).unwrap();
        let remaining: Vec<String> = reopened
            .jobs
            .read()
            .iter()
            .map(|j| j.id.clone())
            .collect();
        assert_eq!(remaining, vec![ids[2].clone()]);

        let _ = std::fs::remove_dir_all(&dir);
    }

}
