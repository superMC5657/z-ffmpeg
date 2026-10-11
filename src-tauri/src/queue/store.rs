//! 队列持久化 seam：`jobs` 表与 `settings` 表全部 SQLite 访问的唯一拥有者。
//!
//! 收敛前 `history.rs` 名义上是持久化模块，实则直掏 manager 的
//! `jobs` / db 锁，且 `remove_jobs` / `delete_history` /
//! `clear_completed` 三处手写同一条留存规则（Pending 行删、Finished 行留）。
//! 本模块唯一拥有：全部 SQL（save/delete/history/paths/vmaf/settings KV）、
//! finished-vs-pending 留存谓词、重启恢复查询、内存优先→DB 回退的分支真相
//! （`resolve_job_paths` / `write_vmaf_score`，调用方只传内存预查结果）。
//! `QueueManager` 只保留调度状态（deque、active 计数、cancel flags、pause），
//! 经显式接口与 store 对话，不再直接触碰 rusqlite。`settings` 表原
//! `settings.rs` 薄 adapter（经 Connection 直透）已并入本模块，JobStore
//! 是 settings KV 的唯一 interface（`max_concurrent` + `vmaf_segments`
//! 语义不变）。

use std::sync::Mutex as StdMutex;

use rusqlite::Connection;

use crate::error::{AppResult, LogDisplay};
use crate::queue::job::{EncodeJob, JobSnapshot, JobStatus};

/// 队列库建表语句（jobs + settings），经 `util::db::open_sqlite_store` 执行。
/// 与搬运前逐字一致：DB 文件位置与表结构不变，重启恢复兼容。
const QUEUE_INIT_SQL: &str = "CREATE TABLE IF NOT EXISTS jobs (
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
            );";

/// settings 表中已知的 key（原 `settings.rs`，现并入本模块；
/// JobStore 是 settings KV 的唯一 interface）。
pub(crate) const SETTINGS_KEY_MAX_CONCURRENT: &str = "max_concurrent";
pub(crate) const SETTINGS_KEY_VMAF_SEGMENTS: &str = "vmaf_segments";

/// 全部 jobs 表访问的唯一拥有者。内部以 std Mutex 串行化 Connection
///（Connection 实现 Send 但未实现 Sync，与搬运前一致）。
pub(crate) struct JobStore {
    db: StdMutex<Connection>,
}

impl JobStore {
    pub(crate) fn open(db_path: &str) -> AppResult<Self> {
        let db = crate::util::db::open_sqlite_store(
            std::path::Path::new(db_path),
            QUEUE_INIT_SQL,
            "queue",
        )
        .inspect_err(|e| {
            let top = e.top_line();
            log::error!("queue db open failed reason {top}");
        })?;
        Ok(Self { db: StdMutex::new(db) })
    }

    /// settings KV 读：缺失/解析失败返回 None，调用方再套默认值。
    /// 与并入前 `settings::load_usize` 逐行一致（含 trim）。
    pub(crate) fn load_setting_usize(&self, key: &str) -> Option<usize> {
        self.load_setting_string(key)?.trim().parse().ok()
    }

    /// settings KV 写：INSERT OR REPLACE。与并入前 `settings::save_usize` 一致。
    pub(crate) fn save_setting_usize(&self, key: &str, value: usize) {
        self.save_setting_string(key, &value.to_string());
    }

    /// settings 表字符串读（私有 helper，原 `settings::load_string` 下沉）。
    fn load_setting_string(&self, key: &str) -> Option<String> {
        self.db
            .lock()
            .unwrap()
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                rusqlite::params![key],
                |row| row.get(0),
            )
            .ok()
    }

    /// settings 表字符串写（私有 helper，原 `settings::save_string` 下沉）。
    fn save_setting_string(&self, key: &str, value: &str) {
        let _ = self.db.lock().unwrap().execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
            rusqlite::params![key, value],
        );
    }

    /// 重启恢复查询：仅 Pending / Encoding 行，Encoding 回 Pending。
    /// 与搬运前逐行一致。
    pub(crate) fn load_active_jobs(&self) -> Vec<EncodeJob> {
        let db = self.db.lock().unwrap();
        let mut stmt = match db.prepare(
            "SELECT id, input_path, output_path, config_json, status, progress,
                    input_size, estimated_output_size, output_size, vmaf_score, vmaf_detail, created_at, started_at, completed_at, error
             FROM jobs WHERE status IN ('Pending', 'Encoding')
             ORDER BY created_at ASC"
        ) {
            Ok(s) => s,
            Err(e) => {
                let top = e.top_line();
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

    pub(crate) fn save_job(&self, job: &EncodeJob) {
        let db = self.db.lock().unwrap();
        let _ = db.execute(
            "INSERT OR REPLACE INTO jobs (id, input_path, output_path, config_json, status, progress, input_size, estimated_output_size, output_size, vmaf_score, vmaf_detail, created_at, started_at, completed_at, error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            rusqlite::params![
                job.id, job.input_path, job.output_path,
                serde_json::to_string(&job.config).unwrap_or_default(),
                job.status.as_str(), job.progress,
                job.input_size,
                job.estimated_output_size,
                job.output_size,
                job.vmaf_score, job.vmaf_detail,
                job.created_at, job.started_at, job.completed_at, job.error,
            ],
        );
    }

    fn delete_job(&self, id: &str) {
        let _ = self.db.lock().unwrap().execute("DELETE FROM jobs WHERE id = ?1", rusqlite::params![id]);
    }

    /// 留存规则的唯一真相源：已结束（Completed / Failed / Cancelled）的
    /// DB 行属于历史页，队列页操作绝不能删；Pending / Encoding 行随队列走。
    pub(crate) fn is_finished(status: &JobStatus) -> bool {
        matches!(
            status,
            JobStatus::Completed | JobStatus::Failed | JobStatus::Cancelled
        )
    }

    /// 留存语义三者分工（行为与收拢前逐行一致，只收拢位置）：
    /// - `clear_completed`（manager 侧）：仅内存 retain，绝不碰 DB；
    ///   队列页“清除已完成”只把已结束任务移出内存，DB 行保留，历史页仍可读。
    /// - `clear_history_rows`（本模块，下方）：SQL DELETE 全部已结束行；
    ///   历史页“清空”连库一起删，但 Pending 行不动。
    /// - `remove_from_queue`（本模块，下方）：按 ID + 内存状态逐行处理，
    ///   Pending / Encoding 行删库，Finished 行留库；未知 ID（`status=None`，
    ///   仅历史残留）按 `unwrap_or(true)` 视为已结束、绝不删库。
    ///
    /// 队列页移除的 DB 侧：Pending / Encoding 行删除，Finished 行保留，
    /// 未知 ID（仅历史残留）视为已结束、绝不删库。与搬运前
    /// `remove_jobs` 的 `unwrap_or(true)` 语义一致。
    pub(crate) fn remove_from_queue(&self, id: &str, status: Option<&JobStatus>) {
        let is_finished = status.map(Self::is_finished).unwrap_or(true);
        if !is_finished {
            self.delete_job(id);
        }
    }

    /// 历史页单删的 DB 侧：按 ID 无条件删除。
    pub(crate) fn delete_history_rows(&self, ids: &[String]) {
        for id in ids {
            self.delete_job(id);
        }
    }

    /// 历史页清空的 DB 侧：删除全部已结束行（Pending 不动）。
    pub(crate) fn clear_history_rows(&self) {
        let db = self.db.lock().unwrap();
        let _ = db.execute(
            "DELETE FROM jobs WHERE status IN ('Completed', 'Failed', 'Cancelled')",
            [],
        );
    }

    /// VMAF 的 DB 回退读：jobs 活得比内存久时的纯 DB 路径（私有，
    /// 对外经 `resolve_job_paths` 统一口径）。
    fn job_paths_row(&self, job_id: &str) -> Option<(String, String)> {
        let db = self.db.lock().unwrap();
        db.query_row(
            "SELECT input_path, output_path FROM jobs WHERE id = ?1",
            rusqlite::params![job_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .ok()
    }

    /// VMAF 的 DB 回退写：已移出内存的历史任务直接 UPDATE，避免静默丢结果
    ///（私有，对外经 `write_vmaf_score` 统一口径）。
    fn update_vmaf_row(&self, job_id: &str, score: f64, detail_json: Option<String>) {
        let db = self.db.lock().unwrap();
        let _ = db.execute(
            "UPDATE jobs SET vmaf_score = ?1, vmaf_detail = ?2 WHERE id = ?3",
            rusqlite::params![score, detail_json, job_id],
        );
    }

    /// 路径解析的唯一真相源：内存优先→DB 回退的分支原在 `history.rs`，
    /// 现下沉到此。`memory_hit` 是调用方在内存队列中的预查结果
    ///（Some=命中直接返回；None=未命中，回退查 DB 历史行）。
    /// 调用方只做内存预查，不再手写回退分支。
    pub(crate) fn resolve_job_paths(
        &self,
        memory_hit: Option<(String, String)>,
        job_id: &str,
    ) -> Option<(String, String)> {
        if let Some(paths) = memory_hit {
            return Some(paths);
        }
        self.job_paths_row(job_id)
    }

    /// VMAF 写分支的唯一真相源：原在 `history.rs` 的内存/DB 双写分支下沉到此。
    /// `memory_job` 是调用方持锁查到的可变借用（Some=在内存：更新字段并经
    /// save_job 持久化；None=已移出内存：直接 UPDATE DB 行）。
    pub(crate) fn write_vmaf_score(
        &self,
        memory_job: Option<&mut EncodeJob>,
        job_id: &str,
        score: f64,
        detail_json: Option<String>,
    ) {
        if let Some(job) = memory_job {
            job.vmaf_score = Some(score);
            job.vmaf_detail = detail_json;
            self.save_job(job);
            return;
        }
        self.update_vmaf_row(job_id, score, detail_json);
    }

    /// 直接从数据库加载历史条目（已完成 / 失败 / 已取消）。
    /// 与内存队列（启动时仅恢复活动任务）不同，数据库会持久保留已结束任务，
    /// 使历史记录在应用重启后依然有效。
    pub(crate) fn history(&self) -> Vec<JobSnapshot> {
        self.history_filtered(None, 0, None, None).0
    }

    /// 带筛选/搜索/分页的历史查询。`status` 过滤单个状态；`search` 对
    /// input_path 做 LIKE 匹配（%/_ 转义）；`limit == None` 表示不分页。
    /// 返回 (当前页条目, 匹配总数)，总数供前端计算页数。
    pub(crate) fn history_filtered(
        &self,
        limit: Option<usize>,
        offset: usize,
        status: Option<&str>,
        search: Option<&str>,
    ) -> (Vec<JobSnapshot>, usize) {
        let db = self.db.lock().unwrap();

        // 动态拼 WHERE，参数按 ?N 顺序追加
        let mut where_clauses = vec!["status IN ('Completed', 'Failed', 'Cancelled')".to_string()];
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        if let Some(st) = status {
            params.push(Box::new(st.to_string()));
            where_clauses.push(format!("status = ?{}", params.len()));
        }
        if let Some(q) = search.filter(|s| !s.trim().is_empty()) {
            // LIKE 通配符转义，用户输入按字面量匹配
            let like = format!("%{}%", q.trim().replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"));
            params.push(Box::new(like));
            where_clauses.push(format!("input_path LIKE ?{} ESCAPE '\\'", params.len()));
        }

        let where_sql = where_clauses.join(" AND ");

        // 匹配总数（分页前）
        let total: usize = match db.prepare(&format!("SELECT COUNT(*) FROM jobs WHERE {where_sql}")) {
            Ok(mut stmt) => stmt
                .query_row(
                    rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())),
                    |row| row.get::<_, i64>(0),
                )
                .unwrap_or(0)
                .max(0) as usize,
            Err(_) => 0,
        };

        let mut sql = format!(
            "SELECT id, input_path, output_path, status, progress,
                    input_size, estimated_output_size, output_size, vmaf_score, vmaf_detail,
                    created_at, started_at, completed_at, error
             FROM jobs WHERE {where_sql}
             ORDER BY completed_at DESC, created_at DESC"
        );
        if let Some(limit_val) = limit {
            sql.push_str(" LIMIT ? OFFSET ?");
            params.push(Box::new(limit_val as i64));
            params.push(Box::new(offset as i64));
        }

        let mut stmt = match db.prepare(&sql) {
            Ok(s) => s,
            Err(_) => return (vec![], total),
        };

        let entries = stmt
            .query_map(rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())), |row| {
                let input_path: String = row.get(1)?;
                Ok(JobSnapshot {
                    id: row.get(0)?,
                    input_path: input_path.clone(),
                    output_path: row.get(2)?,
                    file_name: crate::encoder::file_name_from_path(&input_path),
                    status: row.get(3)?,
                    progress: row.get(4)?,
                    input_size: row.get(5)?,
                    estimated_output_size: row.get(6)?,
                    output_size: row.get(7)?,
                    vmaf_score: row.get(8)?,
                    vmaf_detail: row.get(9)?,
                    created_at: row.get(10)?,
                    started_at: row.get(11)?,
                    completed_at: row.get(12)?,
                    error: row.get(13)?,
                })
            })
            .ok()
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default();

        (entries, total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoder::codec::EncodeConfig;

    fn sample_config() -> EncodeConfig {
        serde_json::from_str(
            r#"{"videoCodec":"H264","videoSettings":{"rateControl":{"type":"CRF","value":23},"encoderPreset":"medium","resolution":null,"frameRate":null,"pixelFormat":null,"profile":null,"additionalParams":[]},"audioSettings":{"codec":"AAC","bitrateKbps":192,"channels":2,"sampleRate":48000},"containerFormat":"MP4","hwAccel":null}"#,
        )
        .unwrap()
    }

    /// 轻量 store：内存库，无整 manager、无临时目录。
    fn mem_store() -> JobStore {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(QUEUE_INIT_SQL).unwrap();
        JobStore { db: StdMutex::new(db) }
    }

    fn finished_job(status: JobStatus) -> EncodeJob {
        let mut j = EncodeJob::new("C:\\in\\a.mp4".into(), "C:\\in\\a_encoded.mp4".into(), sample_config());
        j.status = status;
        j.completed_at = Some(chrono::Utc::now().to_rfc3339());
        j
    }

    /// 留存矩阵：remove × pending/finished —— Pending/Encoding 行删、Finished 行留、未知 ID 不删库。
    #[test]
    fn retention_remove_pending_deletes_finished_keeps() {
        let store = mem_store();
        let mut pending = finished_job(JobStatus::Pending);
        pending.input_path = "C:\\in\\pending.mp4".into();
        let mut encoding = finished_job(JobStatus::Encoding);
        encoding.input_path = "C:\\in\\encoding.mp4".into();
        let completed = finished_job(JobStatus::Completed);
        let failed = finished_job(JobStatus::Failed);
        let cancelled = finished_job(JobStatus::Cancelled);
        for j in [&pending, &encoding, &completed, &failed, &cancelled] {
            store.save_job(j);
        }

        store.remove_from_queue(&pending.id, Some(&JobStatus::Pending));
        store.remove_from_queue(&encoding.id, Some(&JobStatus::Encoding));
        store.remove_from_queue(&completed.id, Some(&JobStatus::Completed));
        store.remove_from_queue(&failed.id, Some(&JobStatus::Failed));
        store.remove_from_queue(&cancelled.id, Some(&JobStatus::Cancelled));
        // 未知 ID（仅历史残留）：绝不删库
        store.remove_from_queue("no-such-id", None);

        assert!(store.job_paths_row(&pending.id).is_none(), "pending row must be deleted");
        assert!(store.job_paths_row(&encoding.id).is_none(), "encoding row must be deleted");
        assert!(store.job_paths_row(&completed.id).is_some(), "completed row must survive queue-remove");
        assert!(store.job_paths_row(&failed.id).is_some());
        assert!(store.job_paths_row(&cancelled.id).is_some());
        // 历史只剩 3 条已结束
        assert_eq!(store.history().len(), 3);
        // 活动恢复为空（pending/encoding 已删，finished 本就不加载）
        assert!(store.load_active_jobs().is_empty());
    }

    /// 留存矩阵：clear × pending/finished —— 已结束全清、Pending 不动。
    #[test]
    fn retention_clear_history_keeps_pending() {
        let store = mem_store();
        let pending = finished_job(JobStatus::Pending);
        let completed = finished_job(JobStatus::Completed);
        let failed = finished_job(JobStatus::Failed);
        store.save_job(&pending);
        store.save_job(&completed);
        store.save_job(&failed);

        store.clear_history_rows();

        assert!(store.job_paths_row(&pending.id).is_some(), "pending must survive clear_history");
        assert!(store.job_paths_row(&completed.id).is_none());
        assert!(store.job_paths_row(&failed.id).is_none());
        assert!(store.history().is_empty());
        // 活动恢复仍见 pending
        assert_eq!(store.load_active_jobs().len(), 1);
    }

    /// 留存矩阵：delete × ids —— 按 ID 无条件删除（历史页单删语义）。
    #[test]
    fn retention_delete_history_removes_given_ids() {
        let store = mem_store();
        let a = finished_job(JobStatus::Completed);
        let b = finished_job(JobStatus::Failed);
        let c = finished_job(JobStatus::Completed);
        for j in [&a, &b, &c] {
            store.save_job(j);
        }
        assert_eq!(store.history().len(), 3);

        store.delete_history_rows(&[b.id.clone()]);
        let hist = store.history();
        assert_eq!(hist.len(), 2);
        assert!(hist.iter().all(|h| h.id != b.id));
        assert!(store.job_paths_row(&b.id).is_none());
    }

    /// 留存矩阵：内存/重启恢复 —— 活动（Pending/Encoding）进内存，Finished 只进历史。
    #[test]
    fn retention_restart_active_vs_history_split() {
        let dir = std::env::temp_dir().join(format!("z-ffmpeg_storetest_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("queue.db").to_string_lossy().to_string();

        let store = JobStore::open(&db_path).unwrap();
        let pending = finished_job(JobStatus::Pending);
        let mut encoding = finished_job(JobStatus::Encoding);
        encoding.input_path = "C:\\in\\enc.mp4".into();
        let completed = finished_job(JobStatus::Completed);
        store.save_job(&pending);
        store.save_job(&encoding);
        store.save_job(&completed);
        drop(store);

        let reopened = JobStore::open(&db_path).unwrap();
        let active = reopened.load_active_jobs();
        assert_eq!(active.len(), 2, "only pending/encoding reload into memory");
        assert!(active.iter().all(|j| j.status == JobStatus::Pending), "encoding resets to pending");
        let hist = reopened.history();
        assert_eq!(hist.len(), 1);
        assert_eq!(hist[0].status, "Completed");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// VMAF seam：DB-only 路径解析 + 分数回写（无内存参与）。
    #[test]
    fn retention_vmaf_db_fallback_paths_and_score() {
        let store = mem_store();
        let completed = finished_job(JobStatus::Completed);
        store.save_job(&completed);

        assert_eq!(
            store.job_paths_row(&completed.id),
            Some(("C:\\in\\a.mp4".into(), "C:\\in\\a_encoded.mp4".into()))
        );
        assert!(store.job_paths_row("missing").is_none());

        store.update_vmaf_row(&completed.id, 91.25, Some(r#"{"scores":[90.1,92.4]}"#.into()));
        let hist = store.history();
        let entry = hist.iter().find(|h| h.id == completed.id).unwrap();
        assert_eq!(entry.vmaf_score, Some(91.25));
        assert!(entry.vmaf_detail.is_some());
    }

    /// 历史查询仍归 store：状态过滤 + LIKE 转义 + 分页（轻量版，不起整 manager）。
    #[test]
    fn retention_history_filter_search_pagination() {
        let store = mem_store();
        for name in ["alpha", "beta", "gamma", "delta"] {
            let mut j = finished_job(JobStatus::Completed);
            j.input_path = format!("C:\\in\\{name}.mp4");
            store.save_job(&j);
        }
        let mut f = finished_job(JobStatus::Failed);
        f.input_path = "C:\\in\\epsilon.mp4".into();
        store.save_job(&f);

        let (all, total) = store.history_filtered(None, 0, None, None);
        assert_eq!((all.len(), total), (5, 5));
        let (failed, total) = store.history_filtered(None, 0, Some("Failed"), None);
        assert_eq!((failed.len(), total), (1, 1));
        let (hits, total) = store.history_filtered(None, 0, None, Some("beta"));
        assert_eq!(total, 1);
        assert_eq!(hits[0].file_name, "beta.mp4");
        let (_, wildcard_total) = store.history_filtered(None, 0, None, Some("%"));
        assert_eq!(wildcard_total, 0, "% 应被转义为字面量");
        let (p0, total) = store.history_filtered(Some(2), 0, None, None);
        let (p1, _) = store.history_filtered(Some(2), 2, None, None);
        assert_eq!((p0.len(), total), (2, 5));
        assert_eq!(p1.len(), 2);
    }

    /// settings KV（原 `settings.rs` 并入）：缺省 None、写入覆盖、解析语义不变。
    #[test]
    fn settings_roundtrip_and_default() {
        let store = mem_store();
        assert_eq!(store.load_setting_usize(SETTINGS_KEY_MAX_CONCURRENT), None);
        store.save_setting_usize(SETTINGS_KEY_MAX_CONCURRENT, 4);
        assert_eq!(store.load_setting_usize(SETTINGS_KEY_MAX_CONCURRENT), Some(4));
        // 覆盖写
        store.save_setting_usize(SETTINGS_KEY_MAX_CONCURRENT, 8);
        assert_eq!(store.load_setting_usize(SETTINGS_KEY_MAX_CONCURRENT), Some(8));
        // 脏值解析失败按缺失处理
        store.save_setting_string(SETTINGS_KEY_VMAF_SEGMENTS, "not-a-number");
        assert_eq!(store.load_setting_usize(SETTINGS_KEY_VMAF_SEGMENTS), None);
        // 字符串口径 roundtrip
        store.save_setting_string(SETTINGS_KEY_VMAF_SEGMENTS, "6");
        assert_eq!(store.load_setting_string(SETTINGS_KEY_VMAF_SEGMENTS).as_deref(), Some("6"));
        assert_eq!(store.load_setting_usize(SETTINGS_KEY_VMAF_SEGMENTS), Some(6));
    }
}
