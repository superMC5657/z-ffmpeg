use serde::{Deserialize, Serialize};

/// A reusable encoding configuration preset.
/// Built-in presets are constructed in code; custom presets are persisted in SQLite.
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
