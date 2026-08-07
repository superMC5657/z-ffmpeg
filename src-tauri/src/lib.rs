mod commands;
mod encoder;
mod queue;
mod preset;
mod ffmpeg;
mod util;
mod error;

use std::sync::Arc;
use std::io::Write;
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
    // env_logger 默认时间戳为 UTC,这里改用 chrono::Local 输出本地时间(东八区)
    env_logger::Builder::from_env(
        env_logger::Env::default().filter_or("RUST_LOG", "info"),
    )
    .format(|buf, record| {
        let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        writeln!(
            buf,
            "[{} {:<5} {}] {}",
            ts,
            record.level(),
            record.target(),
            record.args()
        )
    })
    .init();

    // Initialize FFmpeg detection early
    let ffmpeg_status = ffmpeg::library::init_ffmpeg();
    log::info!(
        "FFmpeg status: available={}, version={:?}",
        ffmpeg_status.available,
        ffmpeg_status.version
    );

    // Determine queue database path
    let queue_db_path = get_queue_db_path();

    // Initialize queue manager
    let queue = QueueManager::new(&queue_db_path)
        .map_err(|e| log::error!("Failed to init queue: {:?}", e))
        .ok();

    // Determine preset database path and initialize preset manager
    let preset_db_path = get_preset_db_path();
    let preset_manager = PresetManager::new(&preset_db_path)
        .map_err(|e| log::error!("Failed to init preset store: {:?}", e))
        .ok();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(move |app| {
            log::info!("zffmpeg v{} starting...", app.package_info().version);

            app.manage(AppState {
                ffmpeg_status: Mutex::new(ffmpeg_status),
                queue_manager: queue.clone(),
                preset_manager: preset_manager.clone(),
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::encode::probe_file,
            commands::encode::start_encode,
            commands::encode::cancel_encode,
            commands::encode::build_ffmpeg_commands,
            commands::encode::save_command_to_file,
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
fn get_queue_db_path() -> String {
    let data_dir = directories::BaseDirs::new()
        .map(|d| d.data_dir().join("zffmpeg"))
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    std::fs::create_dir_all(&data_dir).ok();

    data_dir
        .join("queue.db")
        .to_string_lossy()
        .to_string()
}

/// Get the path for the preset database
fn get_preset_db_path() -> String {
    let data_dir = directories::BaseDirs::new()
        .map(|d| d.data_dir().join("zffmpeg"))
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    std::fs::create_dir_all(&data_dir).ok();

    data_dir
        .join("presets.db")
        .to_string_lossy()
        .to_string()
}
