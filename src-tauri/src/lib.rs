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
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            // 所有落盘数据的根目录：Tauri app_data_dir（跟随 tauri.conf.json
            // 的 identifier，Windows = %APPDATA%\{identifier}）
            let data_dir = get_data_dir(app.handle());

            // Initialize FFmpeg detection early
            let ffmpeg_status = ffmpeg::library::init_ffmpeg(&data_dir);

            // Determine queue database path and initialize queue manager
            let queue_db_path = data_dir.join("queue.db").to_string_lossy().into_owned();
            let queue = QueueManager::new(&queue_db_path).ok();

            // Determine preset database path and initialize preset manager
            let preset_db_path = data_dir.join("presets.db").to_string_lossy().into_owned();
            let preset_manager = PresetManager::new(&preset_db_path).ok();

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
            // 正常退出时清理运行中的子进程，并一次性上报会话聚合埋点（最多等 3s）
            if let tauri::RunEvent::ExitRequested { .. } = event {
                encoder::engine::kill_all_processes();
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
