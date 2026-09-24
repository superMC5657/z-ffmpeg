use serde::{Deserialize, Serialize};

/// 可复用的视频编码配置预设。
/// 内置预设在代码中定义；自定义预设持久化存储在 SQLite 中。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub config: serde_json::Value,
    pub is_builtin: bool,
    pub created_at: String,
    pub updated_at: String,
}

pub mod manager;
