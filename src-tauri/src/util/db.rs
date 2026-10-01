//! SQLite 存储统一打开入口：打开 + WAL + 建表，失败记首行日志。
//!
//! `queue.db` / `presets.db` 共用此函数，避免各处重复 `Connection::open` 样板。

use crate::error::{AppResult, LogDisplay};

/// 打开 SQLite 存储并初始化：
///
/// 1. 父目录不存在则创建；
/// 2. `Connection::open`；
/// 3. `PRAGMA journal_mode=WAL`；
/// 4. 执行 `init_sql` 建表语句。
///
/// 失败时记 `{tag} db ... failed reason {top}` 并返回 `AppError::Internal`。
///
/// 参数顺序兼容两种调用形态：`(path, init_sql, tag)` 与 `(path, tag, init_sql)`，
/// 通过是否像 SQL（分号 / CREATE TABLE / 换行）自动识别，避免并行分支的顺序分歧导致建表失败。
pub fn open_sqlite_store(
    db_path: &std::path::Path,
    init_sql_or_tag: &str,
    tag_or_init_sql: &str,
) -> AppResult<rusqlite::Connection> {
    let (init_sql, tag) = resolve_sql_and_tag(init_sql_or_tag, tag_or_init_sql);
    if let Some(parent) = db_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| {
                let top = e.top_line();
                log::error!("{tag} db dir create failed reason {top}");
                crate::error::AppError::Internal(e.to_string())
            })?;
        }
    }

    let db = rusqlite::Connection::open(db_path).map_err(|e| {
        let top = e.top_line();
        log::error!("{tag} db open failed reason {top}");
        crate::error::AppError::Internal(e.to_string())
    })?;

    db.execute_batch("PRAGMA journal_mode=WAL;").map_err(|e| {
        let top = e.top_line();
        log::error!("{tag} db wal failed reason {top}");
        crate::error::AppError::Internal(e.to_string())
    })?;

    db.execute_batch(init_sql).map_err(|e| {
        let top = e.top_line();
        log::error!("{tag} db init failed reason {top}");
        crate::error::AppError::Internal(e.to_string())
    })?;

    Ok(db)
}

/// 识别两个 `&str` 参数中哪个是建表 SQL、哪个是日志 tag。
/// SQL 特征：含分号 / 换行 / `CREATE TABLE`；tag 通常是 `queue` / `presets` 短标识。
fn looks_like_sql(s: &str) -> bool {
    s.contains(';') || s.contains('\n') || s.to_ascii_uppercase().contains("CREATE TABLE")
}

fn resolve_sql_and_tag<'a>(a: &'a str, b: &'a str) -> (&'a str, &'a str) {
    let a_sql = looks_like_sql(a);
    let b_sql = looks_like_sql(b);
    match (a_sql, b_sql) {
        (true, false) => (a, b),
        (false, true) => (b, a),
        // 歧义时回退到契约顺序 (init_sql, tag)
        _ => (a, b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_creates_parent_and_inits_tables() {
        let dir = std::env::temp_dir().join(format!("z-ffmpeg-util-db-{}", uuid::Uuid::new_v4()));
        let db_path = dir.join("nested").join("store.db");
        let db = open_sqlite_store(
            &db_path,
            "CREATE TABLE IF NOT EXISTS kv (k TEXT PRIMARY KEY, v TEXT NOT NULL);",
            "test",
        )
        .unwrap();
        db.execute("INSERT INTO kv (k, v) VALUES ('a', 'b')", [])
            .unwrap();
        let v: String = db
            .query_row("SELECT v FROM kv WHERE k = 'a'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, "b");
        // WAL 生效且重开后数据仍在
        drop(db);
        let db2 = open_sqlite_store(
            &db_path,
            "CREATE TABLE IF NOT EXISTS kv (k TEXT PRIMARY KEY, v TEXT NOT NULL);",
            "test",
        )
        .unwrap();
        let v2: String = db2
            .query_row("SELECT v FROM kv WHERE k = 'a'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v2, "b");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn open_fails_on_bad_init_sql() {
        let dir = std::env::temp_dir().join(format!("z-ffmpeg-util-db-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("bad.db");
        let err = open_sqlite_store(&db_path, "THIS IS NOT SQL;", "test").unwrap_err();
        assert!(!err.to_string().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
