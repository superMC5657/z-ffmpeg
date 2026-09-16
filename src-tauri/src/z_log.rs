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

/// 单个日志文件滚动阈值：5MB（单文件封顶，避免单个日志过大拖慢导出）。
const MAX_FILE_SIZE: u128 = 5 * 1024 * 1024;
/// 日志保留天数：14 天。
const RETAIN_DAYS: u64 = 14;
/// 日志目录总水位：25MB，超了按 mtime 从旧到新删。
const MAX_TOTAL_BYTES: u64 = 25 * 1024 * 1024;
/// 滚动保留的旧日志文件数（不含当前写入文件）：KeepSome(5)。
const KEEP_ROTATED: usize = 5;

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

/// 解析单条级别覆盖值：error/warn/info/debug/trace（大小写不敏感，
/// 前后空白容忍，warn 兼容 warning）。非法值返回 None——调用方忽略，
/// 绝不因环境变量写错而炸启动。
fn parse_level_override(raw: &str) -> Option<LevelFilter> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "off" => Some(LevelFilter::Off),
        "error" => Some(LevelFilter::Error),
        "warn" | "warning" => Some(LevelFilter::Warn),
        "info" => Some(LevelFilter::Info),
        "debug" => Some(LevelFilter::Debug),
        "trace" => Some(LevelFilter::Trace),
        _ => None,
    }
}

/// 全局阈值解析优先级：ZFFMPEG_LOG > RUST_LOG > 默认。
/// 专有变量优先（避免 RUST_LOG 被其他库污染时误伤本应用阈值），
/// 两者都缺失/非法时回退到 `root_level()`。
fn resolve_level() -> LevelFilter {
    std::env::var("ZFFMPEG_LOG")
        .ok()
        .and_then(|v| parse_level_override(&v))
        .or_else(|| {
            std::env::var("RUST_LOG")
                .ok()
                .and_then(|v| parse_level_override(&v))
        })
        .unwrap_or_else(root_level)
}

/// 构建日志插件：release 仅 `LogDir`，dev 额外 `Stdout` + `Webview`。
pub fn init() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    // 全局 format：级别 + target + 本地 HH:mm:ss.SSS，写盘前二次脱敏
    //（调用方偶发直接打全路径/token，format 层统一兜底）。
    let fmt = |out: tauri_plugin_log::fern::FormatCallback,
               message: &std::fmt::Arguments,
               record: &log::Record| {
        out.finish(format_args!(
            "[{}][{}][{}] {}",
            chrono::Local::now().format("%H:%M:%S%.3f"),
            record.level(),
            record.target(),
            redact(&message.to_string())
        ))
    };
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
        .level(resolve_level())
        // 编码进度是高频噪音源：只允许 Info 及以上落盘（dev 放宽到 Debug，禁止 Trace）
        .level_for("zffmpeg_lib::encoder", encoder_level())
        // 第三方噪音一并压住
        .level_for("tao", LevelFilter::Warn)
        .level_for("wry", LevelFilter::Warn)
        .level_for("tauri", LevelFilter::Warn)
        .level_for("tracing", LevelFilter::Warn)
        .level_for("reqwest", LevelFilter::Warn)
        .level_for("hyper", LevelFilter::Warn)
        .level_for("tungstenite", LevelFilter::Warn)
        .targets(targets)
        .format(fmt)
        .rotation_strategy(RotationStrategy::KeepSome(KEEP_ROTATED))
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
    // 基于游标推进：只在 ASCII '@' 处判定邮箱，按字符边界安全拷贝，保障多字节字符不乱码。
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

/// 修剪策略：删 14 天前文件；总量超 25MB 按 mtime 从旧到新删。只管 *.log。
pub fn prune(dir: &Path) {
    let now = SystemTime::now();
    let mut removed: u32 = 0;
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

    // 1) 超 14 天直接删
    for entry in &entries {
        let path = entry.path();
        let old_enough = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .map(|t| is_expired(t, now))
            .unwrap_or(false);
        if old_enough && std::fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }

    // 2) 总量水位：超 25MB 按 mtime 从旧到新删
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
            removed += 1;
        }
    }
    // 修剪只记删除计数：不记文件名（可能含用户目录片段）
    if removed > 0 {
        log::debug!("zlog pruned count={removed}");
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
    fn parse_level_override_accepts_known_levels() {
        assert_eq!(parse_level_override("error"), Some(LevelFilter::Error));
        assert_eq!(parse_level_override("WARN"), Some(LevelFilter::Warn));
        assert_eq!(parse_level_override("warning"), Some(LevelFilter::Warn));
        assert_eq!(parse_level_override("  info  "), Some(LevelFilter::Info));
        assert_eq!(parse_level_override("Debug"), Some(LevelFilter::Debug));
        assert_eq!(parse_level_override("trace"), Some(LevelFilter::Trace));
        assert_eq!(parse_level_override("off"), Some(LevelFilter::Off));
        assert_eq!(parse_level_override("verbose"), None);
        assert_eq!(parse_level_override(""), None);
        assert_eq!(parse_level_override("info2"), None);
    }

    #[test]
    fn resolve_level_prefers_zffmpeg_log_over_rust_log() {
        // 单测内串行操作进程级环境变量：一个用例走完完整优先级链，
        // 避免多用例并行读写互相干扰；结尾恢复现场。
        let old_z = std::env::var("ZFFMPEG_LOG").ok();
        let old_r = std::env::var("RUST_LOG").ok();
        let restore = || {
            match &old_z {
                Some(v) => std::env::set_var("ZFFMPEG_LOG", v),
                None => std::env::remove_var("ZFFMPEG_LOG"),
            }
            match &old_r {
                Some(v) => std::env::set_var("RUST_LOG", v),
                None => std::env::remove_var("RUST_LOG"),
            }
        };

        std::env::remove_var("ZFFMPEG_LOG");
        std::env::remove_var("RUST_LOG");
        assert_eq!(resolve_level(), root_level());

        // RUST_LOG 生效
        std::env::set_var("RUST_LOG", "warn");
        assert_eq!(resolve_level(), LevelFilter::Warn);

        // ZFFMPEG_LOG 压过 RUST_LOG
        std::env::set_var("ZFFMPEG_LOG", "error");
        assert_eq!(resolve_level(), LevelFilter::Error);

        // ZFFMPEG_LOG 非法时回退到 RUST_LOG（而非直接默认）
        std::env::set_var("ZFFMPEG_LOG", "verbose");
        assert_eq!(resolve_level(), LevelFilter::Warn);

        // 两者都非法时回退默认
        std::env::set_var("RUST_LOG", "nope");
        assert_eq!(resolve_level(), root_level());

        restore();
    }

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
    fn redact_masks_code_value_and_home_dirs() {
        // 激活码/验证码类 `code[:=] value` 必须脱敏
        let out = redact("verify code=ABCD-1234-EFGH-5678 done");
        assert!(!out.contains("ABCD-1234-EFGH-5678"), "code leaked: {out}");
        assert!(out.contains("[redacted]"), "code not masked: {out}");
        let out2 = redact("license code: SECRETVALUE ok");
        assert!(!out2.contains("SECRETVALUE"), "code leaked: {out2}");
        // Unix /home/ 与 macOS /Users/ 用户名片段必须脱敏
        let nix = redact("open /home/alice/video/a.mp4 failed");
        assert!(!nix.contains("/home/alice"), "username leaked: {nix}");
        assert!(nix.contains("/home/[user]"), "home not masked: {nix}");
        let mac = redact("open /Users/Bob/a.mp4 failed");
        assert!(!mac.contains("/Users/Bob"), "username leaked: {mac}");
        assert!(mac.contains("/Users/[user]"), "Users not masked: {mac}");
        // Windows 小写 users 同样处理
        let win = redact("open C:\\users\\eve\\a.mp4 failed");
        assert!(!win.contains("\\eve\\"), "username leaked: {win}");
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
    fn is_expired_flags_files_older_than_14_days() {
        const DAY: u64 = 24 * 60 * 60;
        let now = SystemTime::now();
        // 边界：恰 14 天未过期，14 天 + 1 秒过期（is_expired 用 `>` 比较）
        let exactly_14d = now - std::time::Duration::from_secs(14 * DAY);
        let just_over_14d = now - std::time::Duration::from_secs(14 * DAY + 1);
        let just_under_14d = now - std::time::Duration::from_secs(14 * DAY - 1);
        let fifteen_days = now - std::time::Duration::from_secs(15 * DAY);
        let one_hour = now - std::time::Duration::from_secs(3600);
        assert!(is_expired(fifteen_days, now));
        assert!(is_expired(just_over_14d, now));
        assert!(!is_expired(exactly_14d, now));
        assert!(!is_expired(just_under_14d, now));
        assert!(!is_expired(one_hour, now));
        // 未来 mtime（时钟回拨）不过期
        let future = now + std::time::Duration::from_secs(3600);
        assert!(!is_expired(future, now));
    }
}
