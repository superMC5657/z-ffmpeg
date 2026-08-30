//! 埋点负载组装与退出时一次性上报。
//!
//! 契约约定（接入指南 4.5 节）：失败静默（仅记本地日志）、不重试不弹窗、
//! 会话结束时一次性上报避免高频小请求（服务端限流 60 次/分钟/产品/IP）。

use std::sync::atomic::Ordering;

use serde_json::{json, Value};
use tauri::Manager;

use super::{snapshots, COUNTERS};

/// 组装会话聚合负载（JSON 对象，远小于 1MB 上限）
pub fn build_payload(device_id: &str, license_status: &str, version: &str) -> Value {
    let (codecs, events) = snapshots();
    let c = &COUNTERS;
    let num = |ctr: &std::sync::atomic::AtomicU64| ctr.load(Ordering::Relaxed);

    let mut payload = json!({
        "deviceId": device_id,
        "licenseStatus": license_status,
        "version": version,
        "os": os_name(),
        "arch": std::env::consts::ARCH,
        "sessionStart": super::session_start(),
        "sessionEnd": chrono::Utc::now().timestamp(),
        "filesAdded": num(&c.files_added),
        "jobsAdded": num(&c.jobs_added),
        "encodeCompleted": num(&c.encode_completed),
        "encodeFailed": num(&c.encode_failed),
        "encodeCancelled": num(&c.encode_cancelled),
        "retries": num(&c.retries),
        "hwAccelJobs": num(&c.hw_accel_jobs),
        "vmafRuns": num(&c.vmaf_runs),
        "presets": {
            "saved": num(&c.presets_saved),
            "imported": num(&c.presets_imported),
            "exported": num(&c.presets_exported),
        },
        "ffmpegDownloaded": num(&c.ffmpeg_downloaded),
        "commandsExported": num(&c.commands_exported),
    });

    // 可选 map：为空则不带，保持负载干净
    if !codecs.is_empty() {
        payload["codecs"] = json!(codecs.iter().cloned().collect::<std::collections::BTreeMap<_, _>>());
    }
    if !events.is_empty() {
        payload["events"] = json!(events.iter().cloned().collect::<std::collections::BTreeMap<_, _>>());
    }
    payload
}

/// 友好操作系统名（sysinfo），拿不到回退 `std::env::consts::OS`
fn os_name() -> String {
    sysinfo::System::name()
        .zip(sysinfo::System::os_version())
        .map(|(name, ver)| format!("{name} {ver}"))
        .unwrap_or_else(|| std::env::consts::OS.to_string())
}

/// 是否启用埋点（settings 表 `analytics_enabled`，默认开启）。
/// 退出钩子在队列管理器初始化失败时也应保守地视为关闭——无法读到用户偏好。
pub fn is_enabled(queue: Option<&std::sync::Arc<crate::queue::QueueManager>>) -> bool {
    queue
        .map(|q| q.get_setting_usize(crate::queue::settings::SETTINGS_KEY_ANALYTICS_ENABLED, 1) != 0)
        .unwrap_or(false)
}

/// 退出时上报：起独立线程发送，最多等 3 秒，超时/失败仅记日志。
/// 模块级守卫保证单次（ExitRequested 可能多次触发）。
pub fn report_on_exit(app: &tauri::AppHandle) {
    static REPORT_TRIGGERED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    // 单次守卫：第一次 ExitRequested 之外的触发直接跳过
    if REPORT_TRIGGERED.fetch_add(1, Ordering::SeqCst) > 0 {
        return;
    }

    let (cfg, device_id, license_status, queue) = {
        let state = app.state::<crate::AppState>();
        let license_status = if state.license.is_pro() { "pro" } else { "free" };
        (
            state.license.config().clone(),
            state.license.device_id().to_string(),
            license_status.to_string(),
            state.queue_manager.clone(),
        )
    };

    // 配置为空（未接入）时不上报
    if !cfg.online_enabled() {
        return;
    }

    if !is_enabled(queue.as_ref()) {
        log::info!("埋点上报已关闭（用户设置），跳过");
        return;
    }

    let version = app.package_info().version.to_string();
    let payload = build_payload(&device_id, &license_status, &version);

    // 独立线程发送，避免阻塞退出流程；通道 + recv_timeout 实现有界等待
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let token = cfg.analytics_token.clone();
    let url = cfg.analytics_url();
    std::thread::spawn(move || {
        let result = send_blocking(&url, &token, &payload);
        match result {
            Ok(()) => log::info!("埋点会话上报完成"),
            Err(e) => log::warn!("埋点会话上报失败（静默忽略）: {e}"),
        }
        let _ = tx.send(());
    });

    let wait = std::time::Duration::from_secs(3);
    if rx.recv_timeout(wait).is_err() {
        log::warn!("埋点上报超时（{}s），放弃等待", wait.as_secs());
    }
}

/// 阻塞 POST 上报（Bearer token 按产品配置决定是否携带）
fn send_blocking(url: &str, token: &str, payload: &Value) -> Result<(), String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;

    let mut req = client
        .post(url)
        .json(payload);
    if !token.is_empty() {
        req = req.bearer_auth(token);
    }

    let resp = req.send().map_err(|e| e.to_string())?;
    let status = resp.status();
    if status.is_success() {
        Ok(())
    } else {
        Err(format!("HTTP {status}: {}", resp.text().unwrap_or_default()))
    }
}

/// 供测试：校验负载结构（数字字段非负、无数组根等契约要求）
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_is_json_object_with_core_fields() {
        // 隔离测试进程内静态计数器从 0 开始（test 二进制每进程一次，可接受）
        let payload = build_payload("test-device", "free", "0.3.0");
        assert!(payload.is_object());
        assert_eq!(payload["deviceId"], "test-device");
        assert_eq!(payload["licenseStatus"], "free");
        assert_eq!(payload["version"], "0.3.0");
        assert!(payload["sessionStart"].is_i64());
        assert!(payload["sessionEnd"].is_i64());
        assert!(payload.get("codecs").is_none() || payload["codecs"].is_object());
        assert!(payload.get("events").is_none() || payload["events"].is_object());
        // 契约：负载必须是 JSON 对象（非数组）、<1MB
        let text = serde_json::to_string(&payload).unwrap();
        assert!(text.len() < 1024 * 1024);
    }

    #[test]
    fn pro_status_reflected() {
        let payload = build_payload("d", "pro", "1.0.0");
        assert_eq!(payload["licenseStatus"], "pro");
    }
}
