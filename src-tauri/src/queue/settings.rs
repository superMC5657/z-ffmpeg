//! 队列设置存储：settings 表（key-value）的读写，被 QueueManager 的
//! 并发数、VMAF 段数等持久化设置共用。

use rusqlite::Connection;

/// settings 表中已知的 key
pub const SETTINGS_KEY_MAX_CONCURRENT: &str = "max_concurrent";
pub const SETTINGS_KEY_VMAF_SEGMENTS: &str = "vmaf_segments";

/// Read a usize setting from the settings table.
pub fn load_usize(db: &Connection, key: &str) -> Option<usize> {
    let value: Option<String> = db
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            rusqlite::params![key],
            |row| row.get(0),
        )
        .ok();
    value?.trim().parse().ok()
}

/// Persist a usize setting (insert or replace).
pub fn save_usize(db: &Connection, key: &str, value: usize) {
    let _ = db.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
        rusqlite::params![key, value.to_string()],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem_db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);")
            .unwrap();
        db
    }

    #[test]
    fn settings_roundtrip_and_default() {
        let db = mem_db();
        assert_eq!(load_usize(&db, SETTINGS_KEY_MAX_CONCURRENT), None);
        save_usize(&db, SETTINGS_KEY_MAX_CONCURRENT, 4);
        assert_eq!(load_usize(&db, SETTINGS_KEY_MAX_CONCURRENT), Some(4));
        // 覆盖写
        save_usize(&db, SETTINGS_KEY_MAX_CONCURRENT, 8);
        assert_eq!(load_usize(&db, SETTINGS_KEY_MAX_CONCURRENT), Some(8));
    }
}
