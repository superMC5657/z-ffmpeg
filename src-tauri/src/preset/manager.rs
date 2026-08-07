use std::sync::Mutex as StdMutex;
use rusqlite::Connection;
use crate::error::AppResult;
use crate::preset::Preset;

/// SQLite-backed persistence for custom presets.
pub struct PresetManager {
    db: StdMutex<Connection>, // std Mutex because Connection is Send but not Sync
}

impl PresetManager {
    pub fn new(db_path: &str) -> AppResult<std::sync::Arc<Self>> {
        let db = Connection::open(db_path)
            .map_err(|e| crate::error::AppError::Internal(e.to_string()))?;

        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS presets (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                config_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );"
        ).map_err(|e| crate::error::AppError::Internal(e.to_string()))?;

        Ok(std::sync::Arc::new(Self {
            db: StdMutex::new(db),
        }))
    }

    /// Load all custom presets (is_builtin is always false for persisted presets).
    pub fn load(&self) -> Vec<Preset> {
        let db = self.db.lock().unwrap();
        let mut stmt = match db.prepare(
            "SELECT id, name, description, config_json, created_at, updated_at
             FROM presets ORDER BY created_at ASC"
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };

        stmt.query_map([], |row| {
            Ok(Preset {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                config: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_default(),
                is_builtin: false,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })
        .ok()
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    /// Insert or replace a preset.
    pub fn insert(&self, preset: &Preset) -> AppResult<()> {
        let db = self.db.lock().unwrap();
        db.execute(
            "INSERT OR REPLACE INTO presets (id, name, description, config_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                preset.id,
                preset.name,
                preset.description,
                serde_json::to_string(&preset.config).unwrap_or_default(),
                preset.created_at,
                preset.updated_at,
            ],
        )
        .map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
        Ok(())
    }

    /// Fetch a single preset by id.
    pub fn get(&self, id: &str) -> Option<Preset> {
        let db = self.db.lock().unwrap();
        db.query_row(
            "SELECT id, name, description, config_json, created_at, updated_at
             FROM presets WHERE id = ?1",
            rusqlite::params![id],
            |row| {
                Ok(Preset {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    config: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_default(),
                    is_builtin: false,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            },
        )
        .ok()
    }

    /// Delete a preset by id.
    pub fn delete(&self, id: &str) -> AppResult<()> {
        let db = self.db.lock().unwrap();
        db.execute("DELETE FROM presets WHERE id = ?1", rusqlite::params![id])
            .map_err(|e| crate::error::AppError::Internal(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_preset(id: &str, name: &str) -> Preset {
        Preset {
            id: id.into(),
            name: name.into(),
            description: "test".into(),
            config: serde_json::json!({ "videoCodec": "H264" }),
            is_builtin: false,
            created_at: "now".into(),
            updated_at: "now".into(),
        }
    }

    #[test]
    fn insert_load_get_delete_roundtrip() {
        let dir = std::env::temp_dir().join(format!("zffmpeg_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("presets.db").to_string_lossy().to_string();

        let manager = PresetManager::new(&db_path).unwrap();

        // Empty initially
        assert!(manager.load().is_empty());

        // Insert two presets
        manager.insert(&sample_preset("p1", "预设一")).unwrap();
        manager.insert(&sample_preset("p2", "预设二")).unwrap();

        let loaded = manager.load();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].name, "预设一");
        assert!(!loaded[0].is_builtin);
        assert_eq!(loaded[0].config["videoCodec"], "H264");

        // Get single
        let got = manager.get("p2").unwrap();
        assert_eq!(got.name, "预设二");

        // Delete
        manager.delete("p1").unwrap();
        let loaded = manager.load();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "p2");
        assert!(manager.get("p1").is_none());

        // Persistence: reopen the same database
        drop(manager);
        let reopened = PresetManager::new(&db_path).unwrap();
        let loaded = reopened.load();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "预设二");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
