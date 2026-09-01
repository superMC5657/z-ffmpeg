mod commands;
mod encoder;
mod queue;
mod preset;
mod ffmpeg;
mod license;
mod analytics;
mod util;
mod error;

use std::sync::Arc;
use parking_lot::Mutex;
use tauri::Manager;
use crate::ffmpeg::library::FfmpegStatus;
use crate::license::LicenseManager;
use crate::preset::manager::PresetManager;
use crate::queue::QueueManager;

/// Application state shared across all Tauri commands
pub struct AppState {
    pub ffmpeg_status: Mutex<FfmpegStatus>,
    pub queue_manager: Option<Arc<QueueManager>>,
    pub preset_manager: Option<Arc<PresetManager>>,
    pub license: Arc<LicenseManager>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 日志走 tauri-plugin-log：stdout + 系统日志目录（app_log_dir，跟随
    // tauri.conf.json 的 identifier）双目标，前端也可通过 @tauri-apps/plugin-log
    // 写入同一份日志文件。注意必须在 Builder 上先挂 log 插件，因此 FFmpeg/
    // 队列/预设的初始化移到了 setup 内，保证启动阶段的关键日志也能落盘。
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir {
                        file_name: Some("z-ffmpeg".into()),
                    }),
                ])
                .level(log::LevelFilter::Info)
                // 日志时间戳用本地时间（默认 UTC）
                .timezone_strategy(tauri_plugin_log::TimezoneStrategy::UseLocal)
                // 单文件上限 5MB，保留最近 7 个日志文件
                .max_file_size(5 * 1024 * 1024)
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepSome(7))
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            log::info!("z-ffmpeg v{} starting...", app.package_info().version);

            // 所有落盘数据的根目录：Tauri app_data_dir（跟随 tauri.conf.json
            // 的 identifier，Windows = %APPDATA%\{identifier}）
            let data_dir = get_data_dir(app.handle());

            // Initialize FFmpeg detection early
            let ffmpeg_status = ffmpeg::library::init_ffmpeg(&data_dir);
            log::info!(
                "FFmpeg status: available={}, version={:?}",
                ffmpeg_status.available,
                ffmpeg_status.version
            );

            // Determine queue database path and initialize queue manager
            let queue_db_path = data_dir.join("queue.db").to_string_lossy().into_owned();
            let queue = QueueManager::new(&queue_db_path)
                .map_err(|e| log::error!("Failed to init queue: {:?}", e))
                .ok();

            // Determine preset database path and initialize preset manager
            let preset_db_path = data_dir.join("presets.db").to_string_lossy().into_owned();
            let preset_manager = PresetManager::new(&preset_db_path)
                .map_err(|e| log::error!("Failed to init preset store: {:?}", e))
                .ok();

            // 授权管理：解析 tauri.conf.json → plugins.softcandy，
            // 加载本地凭证 + 离线验签（失败 = 免费版）
            let softcandy = license::config::SoftCandyConfig::from_tauri(app.config());
            let license = Arc::new(LicenseManager::new(softcandy, &data_dir));

            app.manage(AppState {
                ffmpeg_status: Mutex::new(ffmpeg_status),
                queue_manager: queue,
                preset_manager,
                license: license.clone(),
            });

            // 启动后异步在线续验一次，之后每 24h 周期续验（网络失败走离线宽限期）
            license.spawn_periodic_verify();

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::encode::probe_file,
            commands::encode::start_encode,
            commands::encode::cancel_encode,
            commands::encode::build_ffmpeg_commands,
            commands::encode::save_command_to_file,
            commands::encode::estimate_output_sizes,
            commands::license::get_license_status,
            commands::license::activate_license,
            commands::license::deactivate_license,
            commands::analytics::track_event,
            commands::analytics::get_analytics_enabled,
            commands::analytics::set_analytics_enabled,
            commands::queue::add_to_queue,
            commands::queue::start_queue,
            commands::queue::remove_from_queue,
            commands::queue::cancel_job,
            commands::queue::get_queue_status,
            commands::queue::pause_queue,
            commands::queue::resume_queue,
            commands::queue::clear_completed,
            commands::queue::retry_job,
            commands::queue::get_max_concurrent,
            commands::queue::set_max_concurrent,
            commands::preset::load_presets,
            commands::preset::delete_preset,
            commands::preset::export_preset,
            commands::preset::export_preset_to_file,
            commands::preset::import_preset,
            commands::preset::get_builtin_presets,
            commands::system::detect_hw_accel,
            commands::system::get_system_info,
            commands::system::check_ffmpeg_status,
            commands::system::download_ffmpeg,
            commands::history::get_history,
            commands::history::delete_history,
            commands::history::clear_history,
            commands::vmaf::compute_vmaf,
            commands::vmaf::get_vmaf_segments,
            commands::vmaf::set_vmaf_segments,
        ])
        .build(tauri::generate_context!())
        .expect("error while building z-ffmpeg")
        .run(|app_handle, event| {
            // 正常退出时一次性上报会话聚合埋点（失败静默，最多等 3s）
            if let tauri::RunEvent::ExitRequested { .. } = event {
                analytics::report::report_on_exit(app_handle);
            }
        });
}

/// Tauri app_data_dir（跟随 tauri.conf.json 的 identifier，Windows =
/// %APPDATA%\{identifier}）。队列库、预设库、device.id/license.json、
/// FFmpeg 本地安装目录都挂在它下面。
pub(crate) fn get_data_dir(app: &tauri::AppHandle) -> std::path::PathBuf {
    let data_dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."));

    std::fs::create_dir_all(&data_dir).ok();
    data_dir
}
