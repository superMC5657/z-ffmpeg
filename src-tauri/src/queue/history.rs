//! 历史页的薄实现：jobs 表中 Completed / Failed / Cancelled 行的读取入口。
//! DB 行比内存队列活得更久（重启、clear_completed 后仍在），是 History 页的数据源。
//! 全部 SQL 与留存规则归 store::JobStore；此处仅做内存优先、DB 回退经 store 的编排，
//! 不直触 rusqlite、不手写状态分支.

use super::job::JobSnapshot;
use super::manager::QueueManager;
use super::store::JobStore;

impl QueueManager {
    /// 从数据库中删除指定的历史条目（已完成 / 失败 / 已取消），
    /// 并从内存队列中同步移除（若存在）。
    pub fn delete_history(&self, ids: &[String]) {
        self.store.delete_history_rows(ids);
        let mut jobs = self.jobs.write();
        jobs.retain(|j| !ids.contains(&j.id));
    }

    /// 从数据库和内存队列中删除所有历史记录条目（已完成 / 失败 / 已取消）。
    /// 即使在重启后内存队列为空时也能正常生效（与 clear_completed 不同）。
    pub fn clear_history(&self) {
        self.store.clear_history_rows();
        let mut jobs = self.jobs.write();
        jobs.retain(|j| !JobStore::is_finished(&j.status));
    }

    /// 取任务的输入/输出路径（VMAF 计算需要原始与压缩后的成对文件）。
    /// 先查内存队列；已完成且被移出内存的任务（clear_completed / 重启后）回退查 store，
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
        // 回退：store 里查（历史任务）
        self.store.job_paths_row(job_id)
    }

    /// 写入 VMAF 计算结果（平均分 + 各段明细 JSON）。
    /// 内存队列中的任务直接更新并经 store 持久化；已移出内存的任务（历史）直接 UPDATE store，
    /// 避免计算结果被静默丢弃。
    pub fn set_vmaf_score(&self, job_id: &str, score: f64, detail_json: Option<String>) {
        let mut jobs = self.jobs.write();
        if let Some(job) = jobs.iter_mut().find(|j| j.id == job_id) {
            job.vmaf_score = Some(score);
            job.vmaf_detail = detail_json;
            self.store.save_job(job);
            return;
        }
        drop(jobs);
        self.store.update_vmaf_row(job_id, score, detail_json);
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

        // 第一次“会话”：添加任务并完成它
        let manager = QueueManager::new(&db_path).unwrap();
        let ids = manager.add_jobs(
            vec![("C:\\in\\a.mp4".into(), "C:\\in\\a_encoded.mp4".into())],
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
