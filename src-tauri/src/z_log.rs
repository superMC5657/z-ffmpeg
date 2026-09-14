//! 统一日志能力：复用 tauri-plugin-log v2。
//!
//! 设计要点：
//! - release 仅落盘（`LogDir`），dev 才加 `Stdout` + `Webview`（前端 console 联调）。
//! - 编码进度循环是噪音源：`zffmpeg encoder` 模块在 release 下压到 `Info`，
//!   dev 下放开到 `Debug`；进度循环内部禁止逐帧打 log，只在开始/完成/失败
//!   打 `info`、取消打 `warn`（见 `encoder::engine`）。
//! - 不上报，只支持本地导出诊断包（`zlog_export_bundle`）。
//! - 不碰 sqlite 业务库（queue.db / presets.db），只管日志目录。

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use log::LevelFilter;
use tauri::{AppHandle, Manager};
use tauri_plugin_log::{RotationStrategy, Target, TargetKind, TimezoneStrategy};

/// 单个日志文件滚动阈值：20MB（与本地小盘策略对齐）。
const MAX_FILE_SIZE: u128 = 20 * 1024 * 1024;
/// 日志保留天数：7 天。
const RETAIN_DAYS: u64 = 7;
/// 日志目录总水位：20MB，超了按 mtime 从旧到新删。
const MAX_TOTAL_BYTES: u64 = 20 * 1024 * 1024;

/// 编码模块在 release/dev 下的阈值：避免 progress 每帧 Trace 刷盘。
fn encoder_level() -> LevelFilter {
    if cfg!(debug_assertions) {
        LevelFilter::Debug
    } else {
        LevelFilter::Info
    }
}

/// 全局阈值：release 收敛到 Info，dev 放开到 Debug。
fn root_level() -> LevelFilter {
    if cfg!(debug_assertions) {
        LevelFilter::Debug
    } else {
        LevelFilter::Info
    }
}

/// 构建日志插件：release 仅 `LogDir`，dev 额外 `Stdout` + `Webview`。
pub fn init() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    let targets: Vec<Target> = if cfg!(debug_assertions) {
        vec![
            Target::new(TargetKind::LogDir { file_name: None }),
            Target::new(TargetKind::Stdout),
            Target::new(TargetKind::Webview),
        ]
    } else {
        vec![Target::new(TargetKind::LogDir { file_name: None })]
    };

    tauri_plugin_log::Builder::new()
        .level(root_level())
        // 编码进度是高频噪音源：只允许 Info 及以上落盘（dev 放宽到 Debug，
        // 但同样禁止 Trace）。crate 名为 `zffmpeg_lib`，兼容任务描述中的
        // `zffmpeg::encoder` 前缀写法，两条都压住。
        .level_for("zffmpeg_lib::encoder", encoder_level())
        .level_for("zffmpeg::encoder", encoder_level())
        // 第三方噪音一并压住
        .level_for("reqwest", LevelFilter::Warn)
        .level_for("hyper", LevelFilter::Warn)
        .level_for("tungstenite", LevelFilter::Warn)
        .targets(targets)
        .rotation_strategy(RotationStrategy::KeepAll)
        .max_file_size(MAX_FILE_SIZE)
        .timezone_strategy(TimezoneStrategy::UseLocal)
        .build()
}

/// 安装 panic hook：把 panic 信息打到日志里（release 无 console，全靠落盘）。
pub fn install_panic_hook() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
            .unwrap_or("<non-string panic payload>");
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "<unknown location>".to_string());
        log::error!(target: "zffmpeg::panic", "panic at {}: {}", location, payload);
        prev(info);
    }));
}

/// 脱敏：去掉邮箱、疑似 token、Windows 用户名路径片段，避免诊断包泄露隐私。
/// 纯手写扫描，不引入新依赖。
pub fn redact(input: &str) -> String {
    let masked_email = mask_emails(input);
    let masked_token = mask_after_key(&masked_email, "token");
    let masked_code = mask_after_key(&masked_token, "code");
    mask_user_profile_segment(&masked_code)
}

fn is_email_local_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '.' | '_' | '-' | '+')
}

fn is_email_domain_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '.' | '-' | '_')
}

fn mask_emails(s: &str) -> String {
    // 基于游标推进：只在 ASCII '@'（恒为字符边界）处做邮箱判定，
    // 非 ASCII 内容按完整 char 拷贝。旧实现按字节步进，
    // `bytes[i] as char` 会把中文 UTF-8 多字节拆成 U+00XX 乱码，
    // 且 `s[start..end]` 可能切在字符中间导致 panic。
    // 中文 UI 日志必然走 redact（导出诊断包），此处必须字符边界安全。
    let mut out = String::with_capacity(s.len());
    let mut cursor = 0;
    let mut i = 0;
    while i < s.len() {
        let ch = s[i..].chars().next().unwrap();
        if ch != '@' {
            i += ch.len_utf8();
            continue;
        }
        // 向前找本地部分起点（按 char 回退，保证边界）
        let before = &s[..i];
        let mut taken = 0;
        for c in before.chars().rev() {
            if is_email_local_char(c) {
                taken += c.len_utf8();
            } else {
                break;
            }
        }
        let start = i - taken;
        // 向后找域名终点（按 char 前进，保证边界）
        let after = &s[i + 1..];
        let mut taken_end = 0;
        for c in after.chars() {
            if is_email_domain_char(c) {
                taken_end += c.len_utf8();
            } else {
                break;
            }
        }
        let end = i + 1 + taken_end;
        let candidate = &s[start..end];
        if candidate.contains('.') && end - start > 3 && start < i && end > i + 1 {
            out.push_str(&s[cursor..start]);
            out.push_str("[redacted-email]");
            cursor = end;
            i = end;
        } else {
            i += 1; // '@' 本身是 1 字节
        }
    }
    out.push_str(&s[cursor..]);
    out
}

/// 把 `token[:=] <value>` / `code[:=] <value>` 后面的值替换掉。
fn mask_after_key(s: &str, key: &str) -> String {
    // 用 to_ascii_lowercase 而非 to_lowercase：前者只映射 ASCII，
    // 与原串等长，lower 中的字节下标可直接用于 s；
    // 后者（如土耳其语 İ）会改变字节长度导致下标错位。
    let lower = s.to_ascii_lowercase();
    let mut out = String::with_capacity(s.len());
    let mut cursor = 0;
    while let Some(rel) = lower[cursor..].find(key) {
        let key_start = cursor + rel;
        let after_key = key_start + key.len();
        let rest = &s[after_key..];
        let trimmed = rest.trim_start();
        let sep_len = rest.len() - trimmed.len();
        let mut chars = trimmed.chars();
        match chars.next() {
            Some('=' | ':') => {
                let sep = trimmed.chars().next().unwrap();
                let after_sep = &trimmed[sep.len_utf8()..];
                let ws = after_sep.len() - after_sep.trim_start().len();
                let value_start = after_key + sep_len + sep.len_utf8() + ws;
                let mut value_end = value_start;
                for (idx, c) in s[value_start..].char_indices() {
                    if c.is_whitespace() || matches!(c, '"' | '\'' | ',' | ';' | '}') {
                        break;
                    }
                    value_end = value_start + idx + c.len_utf8();
                }
                if value_end > value_start {
                    out.push_str(&s[cursor..value_start]);
                    out.push_str("[redacted]");
                    cursor = value_end;
                    continue;
                }
                out.push_str(&s[cursor..value_end]);
                cursor = value_end;
            }
            _ => {
                out.push_str(&s[cursor..after_key]);
                cursor = after_key;
            }
        }
    }
    out.push_str(&s[cursor..]);
    out
}

/// `C:\Users\<name>\` / `/home/<name>/` 中的用户名替换掉。
fn mask_user_profile_segment(s: &str) -> String {
    let mut out = s.to_string();
    for marker in ["\\Users\\", "\\users\\", "/home/", "/Users/"] {
        let mut result = String::with_capacity(out.len());
        let mut cursor = 0;
        while let Some(rel) = out[cursor..].find(marker) {
            let name_start = cursor + rel + marker.len();
            let mut name_end = name_start;
            while name_end < out.len() {
                let c = out[name_end..].chars().next().unwrap();
                if matches!(c, '/' | '\\') {
                    break;
                }
                name_end += c.len_utf8();
            }
            result.push_str(&out[cursor..name_start]);
            result.push_str("[user]");
            cursor = name_end;
        }
        result.push_str(&out[cursor..]);
        out = result;
    }
    out
}

/// 日志目录：优先 Tauri app_log_dir，失败回退到 app_data_dir/logs。
pub fn log_dir(app: &AppHandle) -> PathBuf {
    if let Ok(dir) = app.path().app_log_dir() {
        std::fs::create_dir_all(&dir).ok();
        prune(&dir);
        return dir;
    }
    let fallback = app
        .path()
        .app_data_dir()
        .map(|d| d.join("logs"))
        .unwrap_or_else(|_| PathBuf::from("logs"));
    std::fs::create_dir_all(&fallback).ok();
    prune(&fallback);
    fallback
}

/// 日志文件是否过期（mtime 距 now 超过保留天数），抽出以便单测。
fn is_expired(modified: SystemTime, now: SystemTime) -> bool {
    const DAY: u64 = 24 * 60 * 60;
    now.duration_since(modified)
        .map(|d| d.as_secs() > RETAIN_DAYS * DAY)
        .unwrap_or(false)
}

/// 修剪策略：删 7 天前文件；总量超 20MB 按 mtime 从旧到新删。只管 *.log。
pub fn prune(dir: &Path) {
    let now = SystemTime::now();
    let entries: Vec<_> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .filter(|e| {
                    e.path()
                        .extension()
                        .and_then(|x| x.to_str())
                        .map(|x| x.eq_ignore_ascii_case("log"))
                        .unwrap_or(false)
                })
                .collect()
        })
        .unwrap_or_default();

    // 1) 超 7 天直接删
    for entry in &entries {
        let path = entry.path();
        let old_enough = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .map(|t| is_expired(t, now))
            .unwrap_or(false);
        if old_enough {
            std::fs::remove_file(&path).ok();
        }
    }

    // 2) 总量水位：超 20MB 按 mtime 从旧到新删
    let mut files: Vec<(PathBuf, u64, SystemTime)> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .filter(|e| {
                    e.path()
                        .extension()
                        .and_then(|x| x.to_str())
                        .map(|x| x.eq_ignore_ascii_case("log"))
                        .unwrap_or(false)
                })
                .filter_map(|e| {
                    let m = e.metadata().ok()?;
                    Some((e.path(), m.len(), m.modified().unwrap_or(SystemTime::UNIX_EPOCH)))
                })
                .collect()
        })
        .unwrap_or_default();
    files.sort_by_key(|(_, _, t)| *t);
    let mut total: u64 = files.iter().map(|(_, len, _)| len).sum();
    for (path, len, _) in files {
        if total <= MAX_TOTAL_BYTES {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(len);
        }
    }
}

/// 取日志目录（前端“打开日志目录 / 导出诊断包”入口）。
#[tauri::command]
pub fn zlog_get_dir(app: AppHandle) -> Result<String, String> {
    Ok(log_dir(&app).to_string_lossy().into_owned())
}

/// 导出诊断包：把日志目录下 *.log 打包为 temp 下的 zip（文件名带时间戳），
/// 返回 zip 绝对路径。不上报、不碰 sqlite 业务库。
#[tauri::command]
pub fn zlog_export_bundle(app: AppHandle) -> Result<String, String> {
    use std::io::Write;

    let dir = log_dir(&app);
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let out_path = std::env::temp_dir().join(format!("zffmpeg-logs-{}.zip", stamp));

    let file = std::fs::File::create(&out_path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let mut entries: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| {
                    p.extension()
                        .and_then(|x| x.to_str())
                        .map(|x| x.eq_ignore_ascii_case("log"))
                        .unwrap_or(false)
                })
                .collect()
        })
        .unwrap_or_default();
    entries.sort();
    if entries.is_empty() {
        zip.start_file("README.txt", options)
            .map_err(|e| e.to_string())?;
        zip.write_all(b"no log files yet")
            .map_err(|e| e.to_string())?;
    }
    for path in entries {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("log.log")
            .to_string();
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        let redacted = redact(&content);
        zip.start_file(name, options)
            .map_err(|e| e.to_string())?;
        zip.write_all(redacted.as_bytes())
            .map_err(|e| e.to_string())?;
    }
    // 附带一行版本信息，方便定位问题
    zip.start_file("bundle-info.txt", options)
        .map_err(|e| e.to_string())?;
    zip.write_all(
        format!(
            "product=z-ffmpeg version={} exported_at={}\n",
            app.config().version.as_deref().unwrap_or("unknown"),
            chrono::Local::now().to_rfc3339(),
        )
        .as_bytes(),
    )
    .map_err(|e| e.to_string())?;
    zip.finish().map_err(|e| e.to_string())?;

    Ok(out_path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoder_level_is_info_in_release_or_debug_in_dev() {
        let expected = if cfg!(debug_assertions) {
            LevelFilter::Debug
        } else {
            LevelFilter::Info
        };
        assert_eq!(encoder_level(), expected);
    }

    #[test]
    fn redact_masks_email_token_and_username() {
        let out = redact("contact user@example.com token=abc123 path C:\\Users\\Bob\\a.mp4");
        assert!(!out.contains("user@example.com"), "email leaked: {out}");
        assert!(!out.contains("abc123"), "token leaked: {out}");
        assert!(!out.contains("\\Bob\\"), "username leaked: {out}");
    }

    #[test]
    fn redact_keeps_chinese_text_intact() {
        // 中文 UI 日志必然走 redact：多字节字符不得被拆成乱码，
        // 非邮箱的 '@'（如单独符号）不得 panic。
        let input = "编码完成 输出文件 视频转码成功 @ 用户 happy@example.com 结束";
        let out = redact(input);
        assert!(out.contains("编码完成"), "chinese mangled: {out}");
        assert!(out.contains("视频转码成功"), "chinese mangled: {out}");
        assert!(!out.contains("happy@example.com"), "email leaked: {out}");
        assert!(out.contains("[redacted-email]"), "email not masked: {out}");
        let lone = redact("通知 @ 全体：任务开始");
        assert!(lone.contains("通知"), "chinese mangled: {lone}");
    }

    #[test]
    fn prune_keeps_fresh_logs_and_ignores_db() {
        let dir = std::env::temp_dir().join(format!(
            "zlog-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let fresh = dir.join("fresh.log");
        std::fs::write(&fresh, "fresh").unwrap();
        // 非 log 文件不动
        let db = dir.join("queue.db");
        std::fs::write(&db, "db").unwrap();

        prune(&dir);
        assert!(fresh.exists());
        assert!(db.exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn is_expired_flags_files_older_than_7_days() {
        let now = SystemTime::now();
        let eight_days = now - std::time::Duration::from_secs(8 * 24 * 60 * 60);
        let one_hour = now - std::time::Duration::from_secs(3600);
        assert!(is_expired(eight_days, now));
        assert!(!is_expired(one_hour, now));
    }
}
