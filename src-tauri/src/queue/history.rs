//! 已完成任务的持久化：`jobs` 表中 Completed / Failed / Cancelled 行的
//! 读写。這些 DB 行比内存队列活得更久（重启、clear_completed 后仍在），是
//! History 页的数据源；内存调度（入队/推进/取消/并发）仍在 `manager`。
//!
//! DB 写原语（`save_job` / `delete_job_db`）也归这里，`manager` 的
//! add/cancel/retry/complete 经 `pub(crate)` 调用它们。

use super::job::{EncodeJob, JobSnapshot, JobStatus};
use super::manager::QueueManager;

impl QueueManager {
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

    pub(crate) fn delete_job_db(&self, id: &str) {
        let _ = self.db.lock().unwrap().execute("DELETE FROM jobs WHERE id = ?1", rusqlite::params![id]);
    }

    /// Delete specific history entries (Completed / Failed / Cancelled) from the
    /// database, and drop them from the in-memory queue if present.
    pub fn delete_history(&self, ids: &[String]) {
        let mut jobs = self.jobs.write();
        for id in ids { self.delete_job_db(id); }
        jobs.retain(|j| !ids.contains(&j.id));
    }

    /// Remove ALL history entries (Completed / Failed / Cancelled) from the
    /// database and the in-memory queue. Works even after a restart when the
    /// in-memory queue is empty (unlike `clear_completed`).
    pub fn clear_history(&self) {
        {
            let db = self.db.lock().unwrap();
            let _ = db.execute(
                "DELETE FROM jobs WHERE status IN ('Completed', 'Failed', 'Cancelled')",
                [],
            );
        }
        let mut jobs = self.jobs.write();
        jobs.retain(|j| !matches!(
            j.status,
            JobStatus::Completed | JobStatus::Failed | JobStatus::Cancelled
        ));
    }

    /// 取任务的输入/输出路径（VMAF 计算需要原始与压缩后的成对文件）。
    /// 先查内存队列；已完成且被移出内存的任务（clear_completed / 重启后）回退查 DB，
    /// 保证历史任务仍可计算 VMAF。
    pub fn get_job_paths(&self, job_id: &str) -> Option<(String, String)> {
        if let Some(job) = self
            .jobs
            .read()
            .iter()
            .find(|j| j.id == job_id)
        {
            return Some((job.input_path.clone(), job.output_path.clone()));
        }
        // 回退：DB 里查（历史任务）
        let db = self.db.lock().unwrap();
        db.query_row(
            "SELECT input_path, output_path FROM jobs WHERE id = ?1",
            rusqlite::params![job_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .ok()
    }

    /// 写入 VMAF 计算结果（平均分 + 各段明细 JSON）。
    /// 内存队列中的任务直接更新并持久化；已移出内存的任务（历史）直接 UPDATE DB，
    /// 避免计算结果被静默丢弃。
    pub fn set_vmaf_score(&self, job_id: &str, score: f64, detail_json: Option<String>) {
        let mut jobs = self.jobs.write();
        if let Some(job) = jobs.iter_mut().find(|j| j.id == job_id) {
            job.vmaf_score = Some(score);
            job.vmaf_detail = detail_json;
            self.save_job(job);
            return;
        }
        drop(jobs);
        let db = self.db.lock().unwrap();
        let _ = db.execute(
            "UPDATE jobs SET vmaf_score = ?1, vmaf_detail = ?2 WHERE id = ?3",
            rusqlite::params![score, detail_json, job_id],
        );
    }

    /// Load history entries (Completed / Failed / Cancelled) directly from the
    /// database. Unlike the in-memory queue — which only restores active jobs on
    /// startup — the DB keeps finished jobs, so history survives app restarts.
    pub fn history(&self) -> Vec<JobSnapshot> {
        self.history_filtered(None, 0, None, None).0
    }

    /// 带筛选/搜索/分页的历史查询。`status` 过滤单个状态；`search` 对
    /// input_path 做 LIKE 匹配（%/_ 转义）；`limit == None` 表示不分页。
    /// 返回 (当前页条目, 匹配总数)，总数供前端计算页数。
    pub fn history_filtered(
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
        if limit.is_some() {
            sql.push_str(" LIMIT ? OFFSET ?");
            params.push(Box::new(limit.unwrap() as i64));
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
                    file_name: std::path::Path::new(&input_path)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
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
        ).unwrap()
    }

    #[test]
    fn history_survives_restart() {
        let dir = std::env::temp_dir().join(format!("z-ffmpeg_qtest_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("queue.db").to_string_lossy().to_string();

        // First "session": add a job and complete it
        let manager = QueueManager::new(&db_path).unwrap();
        let ids = manager.add_jobs(
            vec![("C:\\in\\a.mp4".into(), "C:\\in\\a_encoded.mp4".into())],
            sample_config(),
        );
        manager.complete_job(&ids[0], true, None);

        // Second "session": restart the manager — only the DB remains
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

        // Delete a single entry
        manager.delete_history(&[ids[1].clone()]);
        let history = manager.history();
        assert_eq!(history.len(), 2);
        assert!(history.iter().all(|h| h.id != ids[1]));

        // Clear the rest
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
                ("C:\\in\\alpha.mp4".into(), "C:\\in\\alpha_encoded.mp4".into()),
                ("C:\\in\\beta.mp4".into(), "C:\\in\\beta_encoded.mp4".into()),
                ("C:\\in\\gamma.mp4".into(), "C:\\in\\gamma_encoded.mp4".into()),
                ("C:\\in\\delta.mp4".into(), "C:\\in\\delta_encoded.mp4".into()),
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
        // c stays Pending

        // Queue's "清除已完成" must only drop finished jobs from the in-memory
        // queue — the History page reads the same DB table.
        manager.clear_completed();
        assert_eq!(manager.jobs.read().len(), 1, "only the pending job remains");
        assert_eq!(manager.jobs.read()[0].id, ids[2]);

        // History records are untouched
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
        manager.complete_job(&ids[0], true, None); // finished -> history
        // b stays Pending, c stays Pending

        // Queue page removes one finished and one pending job
        manager.remove_jobs(&[ids[0].clone(), ids[1].clone()]);

        // History still has the finished record
        let history = manager.history();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].id, ids[0]);

        // The pending job's DB row was deleted: after a restart it must not
        // resurrect, and only c remains queued.
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
