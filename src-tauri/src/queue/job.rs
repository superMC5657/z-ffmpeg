use serde::{Deserialize, Serialize};
use crate::encoder::codec::EncodeConfig;

/// Status of an encoding job in the queue
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum JobStatus {
    Pending,
    Encoding,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

impl JobStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            JobStatus::Pending => "Pending",
            JobStatus::Encoding => "Encoding",
            JobStatus::Paused => "Paused",
            JobStatus::Completed => "Completed",
            JobStatus::Failed => "Failed",
            JobStatus::Cancelled => "Cancelled",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "Pending" => JobStatus::Pending,
            "Encoding" => JobStatus::Encoding,
            "Paused" => JobStatus::Paused,
            "Completed" => JobStatus::Completed,
            "Failed" => JobStatus::Failed,
            "Cancelled" => JobStatus::Cancelled,
            _ => JobStatus::Pending,
        }
    }
}

/// One task in the encoding queue
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncodeJob {
    pub id: String,
    pub input_path: String,
    pub output_path: String,
    #[serde(skip)]
    pub config: Option<EncodeConfig>,
    pub status: JobStatus,
    pub progress: Option<f64>, // 0.0 - 100.0
    /// 原始文件体积（字节）；add_to_queue 入队时读取，用于完成时计算压缩率
    pub input_size: Option<u64>,
    /// 编码开始前的预估输出体积（字节）；add_to_queue 时由 ffprobe + 配置推算
    pub estimated_output_size: Option<u64>,
    /// 编码完成后的实际输出体积（字节）；complete_job 成功时写入
    pub output_size: Option<u64>,
    /// VMAF 平均得分（0-100），由用户点击“计算 VMAF”后写入
    pub vmaf_score: Option<f64>,
    /// VMAF 各采样段得分（JSON 数组），供明细展示
    pub vmaf_detail: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub error: Option<String>,
}

impl EncodeJob {
    pub fn new(input_path: String, output_path: String, config: EncodeConfig) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            input_path,
            output_path,
            config: Some(config),
            status: JobStatus::Pending,
            progress: None,
            input_size: None,
            estimated_output_size: None,
            output_size: None,
            vmaf_score: None,
            vmaf_detail: None,
            created_at: chrono::Utc::now().to_rfc3339(),
            started_at: None,
            completed_at: None,
            error: None,
        }
    }

    pub fn file_name(&self) -> String {
        std::path::Path::new(&self.input_path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string()
    }
}

/// Serializable snapshot for the frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobSnapshot {
    pub id: String,
    pub input_path: String,
    pub output_path: String,
    pub file_name: String,
    pub status: String,
    pub progress: Option<f64>,
    /// 原始文件体积（字节），入队时读取；用于完成时计算压缩率
    pub input_size: Option<u64>,
    /// 编码开始前的预估输出体积（字节），未探测到时为 null
    pub estimated_output_size: Option<u64>,
    /// 编码完成后的实际输出体积（字节），未完成/失败时为 null
    pub output_size: Option<u64>,
    /// VMAF 平均得分（0-100），未计算时为 null
    pub vmaf_score: Option<f64>,
    /// VMAF 各采样段得分 JSON，未计算时为 null
    pub vmaf_detail: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub error: Option<String>,
}

impl From<&EncodeJob> for JobSnapshot {
    fn from(job: &EncodeJob) -> Self {
        Self {
            id: job.id.clone(),
            input_path: job.input_path.clone(),
            output_path: job.output_path.clone(),
            file_name: job.file_name(),
            status: job.status.as_str().to_string(),
            progress: job.progress,
            input_size: job.input_size,
            estimated_output_size: job.estimated_output_size,
            output_size: job.output_size,
            vmaf_score: job.vmaf_score,
            vmaf_detail: job.vmaf_detail.clone(),
            created_at: job.created_at.clone(),
            started_at: job.started_at.clone(),
            completed_at: job.completed_at.clone(),
            error: job.error.clone(),
        }
    }
}

/// Full queue state sent to frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueStatus {
    pub jobs: Vec<JobSnapshot>,
    pub total: usize,
    pub pending: usize,
    pub encoding: usize,
    pub completed: usize,
    pub failed: usize,
    /// 队列级暂停：暂停后不再自动启动下一个任务（正在编码的任务不受影响）
    pub paused: bool,
}
