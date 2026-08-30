mod commands;
mod encoder;
mod queue;
mod preset;
mod ffmpeg;
mod util;
mod error;

use std::sync::Arc;
use parking_lot::Mutex;
use tauri::Manager;
use crate::ffmpeg::library::FfmpegStatus;
use crate::preset::manager::PresetManager;
use crate::queue::QueueManager;

/// Application state shared across all Tauri commands
pub struct AppState {
    pub ffmpeg_status: Mutex<FfmpegStatus>,
    pub queue_manager: Option<Arc<QueueManager>>,
    pub preset_manager: Option<Arc<PresetManager>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 日志走 tauri-plugin-log：stdout + {data_dir}/zffmpeg/logs/ 双目标，
    // 前端也可通过 @tauri-apps/plugin-log 写入同一份日志文件。
    // 注意必须在 Builder 上先挂 log 插件，因此 FFmpeg/队列/预设的初始化
    // 移到了 setup 内，保证启动阶段的关键日志也能落盘。
    let log_dir = get_log_dir();

    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Folder {
                        path: log_dir,
                        file_name: Some("zffmpeg".into()),
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
        .setup(move |app| {
            log::info!("zffmpeg v{} starting...", app.package_info().version);

            // Initialize FFmpeg detection early
            let ffmpeg_status = ffmpeg::library::init_ffmpeg();
            log::info!(
                "FFmpeg status: available={}, version={:?}",
                ffmpeg_status.available,
                ffmpeg_status.version
            );

            // Determine queue database path and initialize queue manager
            let queue_db_path = get_queue_db_path();
            let queue = QueueManager::new(&queue_db_path)
                .map_err(|e| log::error!("Failed to init queue: {:?}", e))
                .ok();

            // Determine preset database path and initialize preset manager
            let preset_db_path = get_preset_db_path();
            let preset_manager = PresetManager::new(&preset_db_path)
                .map_err(|e| log::error!("Failed to init preset store: {:?}", e))
                .ok();

            app.manage(AppState {
                ffmpeg_status: Mutex::new(ffmpeg_status),
                queue_manager: queue,
                preset_manager,
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::encode::probe_file,
            commands::encode::start_encode,
            commands::encode::cancel_encode,
            commands::encode::build_ffmpeg_commands,
            commands::encode::save_command_to_file,
            commands::encode::estimate_output_sizes,
            commands::queue::add_to_queue,
            commands::queue::start_queue,
            commands::queue::remove_from_queue,
            commands::queue::cancel_job,
            commands::queue::get_queue_status,
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
        .run(tauri::generate_context!())
        .expect("error while running zffmpeg");
}

/// Get the path for the queue database
fn get_data_dir() -> std::path::PathBuf {
    let data_dir = directories::BaseDirs::new()
        .map(|d| d.data_dir().join("zffmpeg"))
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    std::fs::create_dir_all(&data_dir).ok();
    data_dir
}

/// Get the path for the queue database
fn get_queue_db_path() -> String {
    get_data_dir()
        .join("queue.db")
        .to_string_lossy()
        .to_string()
}

/// Get the path for the preset database
fn get_preset_db_path() -> String {
    get_data_dir()
        .join("presets.db")
        .to_string_lossy()
        .to_string()
}

/// Get the directory for log files ({data_dir}/zffmpeg/logs)
fn get_log_dir() -> std::path::PathBuf {
    let dir = get_data_dir().join("logs");
    std::fs::create_dir_all(&dir).ok();
    dir
}
