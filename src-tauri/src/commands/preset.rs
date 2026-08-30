use tauri::State;
use crate::error::{AppError, AppResult};
use crate::preset::Preset;

fn builtin_presets() -> Vec<Preset> {
    vec![
        // --- H.264 Software ---
        p("builtin-h264-fast", "H.264 快速", "ultrafast, CRF 23 — 最快编码",
            "H264", "ultrafast", "CRF", 23, "MP4", "AAC", false),
        p("builtin-h264-balanced", "H.264 平衡", "medium, CRF 23 — 通用编码",
            "H264", "medium", "CRF", 23, "MP4", "AAC", false),
        p("builtin-h264-hq", "H.264 高质量", "slow, CRF 18, high profile — 高画质存档",
            "H264", "slow", "CRF", 18, "MP4", "AAC", false),
        p("builtin-h264-archive", "H.264 无损存档", "veryslow, CRF 0 — 最大画质",
            "H264", "veryslow", "CRF", 0, "MKV", "Opus", false),

        // --- H.265 Software ---
        p("builtin-h265-fast", "H.265 快速", "fast, CRF 28 — HEVC 快速",
            "H265", "fast", "CRF", 28, "MKV", "AAC", false),
        p("builtin-h265-balanced", "H.265 平衡", "medium, CRF 24 — HEVC 通用",
            "H265", "medium", "CRF", 24, "MKV", "AAC", false),
        p("builtin-h265-hq", "H.265 高质量", "slower, CRF 20, main10 — HEVC 高画质",
            "H265", "slower", "CRF", 20, "MKV", "Opus", false),

        // --- AV1 ---
        p("builtin-av1", "AV1 通用", "preset 6, CRF 30 — SVT-AV1",
            "AV1", "medium", "CRF", 30, "MKV", "Opus", false),

        // --- VP9 ---
        p("builtin-vp9", "VP9 Web", "CRF 30 — Web 优化",
            "VP9", "medium", "CRF", 30, "WebM", "Opus", false),

        // --- NVENC ---
        hw_p("builtin-nvenc-h264", "NVENC H.264", "h264_nvenc — NVIDIA GPU 加速",
            "H264", "p4", "NVENC"),
        hw_p("builtin-nvenc-h265", "NVENC H.265", "hevc_nvenc — NVIDIA GPU 加速",
            "H265", "p4", "NVENC"),
        hw_p("builtin-nvenc-av1", "NVENC AV1", "av1_nvenc — NVIDIA RTX 40+",
            "AV1", "p4", "NVENC"),

        // --- QSV ---
        hw_p("builtin-qsv-h264", "QSV H.264", "h264_qsv — Intel GPU 加速",
            "H264", "medium", "QSV"),
        hw_p("builtin-qsv-h265", "QSV H.265", "hevc_qsv — Intel GPU 加速",
            "H265", "medium", "QSV"),

        // --- AMF ---
        hw_p("builtin-amf-h264", "AMF H.264", "h264_amf — AMD GPU 加速",
            "H264", "balanced", "AMF"),
        hw_p("builtin-amf-h265", "AMF H.265", "hevc_amf — AMD GPU 加速",
            "H265", "balanced", "AMF"),

        // --- VideoToolbox (macOS) ---
        hw_p("builtin-vt-h264", "VideoToolbox H.264", "h264_videotoolbox — Apple 硬件加速",
            "H264", "medium", "VideoToolbox"),
        hw_p("builtin-vt-h265", "VideoToolbox H.265", "hevc_videotoolbox — Apple 硬件加速",
            "H265", "medium", "VideoToolbox"),
    ]
}

/// Helper for software presets
fn p(
    id: &str, name: &str, desc: &str,
    codec: &str, preset: &str, rc: &str, value: u32,
    container: &str, audio: &str, _hq: bool,
) -> Preset {
    Preset {
        id: id.into(),
        name: name.into(),
        description: desc.into(),
        config: serde_json::json!({
            "videoCodec": codec,
            "videoSettings": {
                "rateControl": { "type": rc, "value": value },
                "encoderPreset": preset,
                "resolution": null,
                "frameRate": null,
                "pixelFormat": null,
                "profile": null,
                "additionalParams": []
            },
            "audioSettings": {
                "codec": audio,
                "bitrateKbps": 192,
                "channels": 2,
                "sampleRate": 48000
            },
            "containerFormat": container,
            "hwAccel": null
        }),
        is_builtin: true,
        created_at: String::new(),
        updated_at: String::new(),
    }
}

/// Helper for HW-accelerated presets
fn hw_p(id: &str, name: &str, desc: &str, codec: &str, preset: &str, device: &str) -> Preset {
    Preset {
        id: id.into(),
        name: name.into(),
        description: desc.into(),
        config: serde_json::json!({
            "videoCodec": codec,
            "videoSettings": {
                "rateControl": { "type": "CQP", "value": 23 },
                "encoderPreset": preset,
                "resolution": null,
                "frameRate": null,
                "pixelFormat": null,
                "profile": null,
                "additionalParams": []
            },
            "audioSettings": {
                "codec": "AAC",
                "bitrateKbps": 192,
                "channels": 2,
                "sampleRate": 48000
            },
            "containerFormat": if codec == "AV1" { "MKV" } else { "MP4" },
            "hwAccel": { "device": device, "deviceIndex": null }
        }),
        is_builtin: true,
        created_at: String::new(),
        updated_at: String::new(),
    }
}

/// Load all custom (imported) presets from the persistent store.
#[tauri::command]
pub async fn load_presets(state: State<'_, crate::AppState>) -> AppResult<Vec<Preset>> {
    match state.preset_manager.as_ref() {
        Some(m) => Ok(m.load()),
        None => Ok(vec![]),
    }
}

/// Delete a custom preset by id.
#[tauri::command]
pub async fn delete_preset(state: State<'_, crate::AppState>, id: String) -> AppResult<()> {
    log::info!("Delete preset: {}", id);
    match state.preset_manager.as_ref() {
        Some(m) => m.delete(&id),
        None => Ok(()),
    }
}

/// Serialize a preset (built-in or custom) to the portable JSON format
/// { name, description, config }.
fn preset_export_json(state: &crate::AppState, id: &str) -> AppResult<String> {
    // Built-in presets come from code
    if let Some(p) = builtin_presets().iter().find(|p| p.id == id) {
        return Ok(serde_json::to_string_pretty(&serde_json::json!({
            "name": p.name,
            "description": p.description,
            "config": p.config,
        }))?);
    }
    // Custom presets come from the database
    if let Some(m) = state.preset_manager.as_ref() {
        if let Some(p) = m.get(id) {
            return Ok(serde_json::to_string_pretty(&serde_json::json!({
                "name": p.name,
                "description": p.description,
                "config": p.config,
            }))?);
        }
    }
    Err(AppError::Internal(format!("预设不存在: {id}")))
}

/// Export a preset as a JSON string (name + description + config),
/// so it can be re-imported later.
/// Pro 功能：预设导入/导出。
#[tauri::command]
pub async fn export_preset(state: State<'_, crate::AppState>, id: String) -> AppResult<String> {
    state.license.ensure_pro("预设导出")?;
    crate::analytics::bump(&crate::analytics::COUNTERS.presets_exported, 1);
    preset_export_json(&state, &id)
}

/// Export a preset directly to a JSON file at the given path.
/// The file is written by the Rust backend, so it is not subject to the
/// frontend fs-plugin scope restrictions.
#[tauri::command]
pub async fn export_preset_to_file(
    state: State<'_, crate::AppState>,
    id: String,
    path: String,
) -> AppResult<String> {
    // Pro 功能：预设导入/导出
    state.license.ensure_pro("预设导出")?;
    crate::analytics::bump(&crate::analytics::COUNTERS.presets_exported, 1);

    let json = preset_export_json(&state, &id)?;
    std::fs::write(&path, json)
        .map_err(|e| AppError::Io(e))?;
    log::info!("Exported preset {} to {}", id, path);
    Ok(path)
}

/// Import a preset from JSON and persist it to the store.
/// Accepts either the full export format ({ name, description, config })
/// or a bare codec config JSON. `name` (from the frontend, defaulting to the
/// imported file name without extension) takes precedence over the JSON name.
#[tauri::command]
pub async fn import_preset(
    state: State<'_, crate::AppState>,
    json: String,
    name: String,
) -> AppResult<Preset> {
    // Pro 功能：预设导入/导出
    state.license.ensure_pro("预设导入")?;
    crate::analytics::bump(&crate::analytics::COUNTERS.presets_imported, 1);

    let value: serde_json::Value = serde_json::from_str(&json)?;

    let (config, description) = if let Some(c) = value.get("config") {
        if !c.is_object() {
            return Err(AppError::InvalidConfig("预设 config 必须是 JSON 对象".into()));
        }
        (
            c.clone(),
            value.get("description").and_then(|d| d.as_str()).unwrap_or("").to_string(),
        )
    } else {
        if !value.is_object() {
            return Err(AppError::InvalidConfig("预设 JSON 格式无效".into()));
        }
        (value.clone(), String::new())
    };

    let preset_name = if name.trim().is_empty() {
        value.get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("导入的预设")
            .to_string()
    } else {
        name.trim().to_string()
    };

    let manager = state.preset_manager.as_ref()
        .ok_or_else(|| AppError::Internal("Preset store not initialized".into()))?;

    let now = chrono::Utc::now().to_rfc3339();
    let preset = Preset {
        id: uuid::Uuid::new_v4().to_string(),
        name: preset_name,
        description,
        config,
        is_builtin: false,
        created_at: now.clone(),
        updated_at: now,
    };
    manager.insert(&preset)?;
    log::info!("Imported preset: {}", preset.name);
    Ok(preset)
}

#[tauri::command]
pub async fn get_builtin_presets() -> AppResult<Vec<Preset>> {
    Ok(builtin_presets())
}
