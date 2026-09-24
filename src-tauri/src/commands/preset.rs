use tauri::State;
use crate::error::{AppError, AppResult};
use crate::preset::Preset;

fn builtin_presets() -> Vec<Preset> {
    vec![
        // --- H.264 软编 ---
        p("builtin-h264-fast", "H.264 快速", "ultrafast, CRF 23 — 最快编码",
            "H264", "ultrafast", "CRF", 23, "MP4", "AAC"),
        p("builtin-h264-balanced", "H.264 平衡", "medium, CRF 23 — 通用编码",
            "H264", "medium", "CRF", 23, "MP4", "AAC"),
        p("builtin-h264-hq", "H.264 高质量", "slow, CRF 18, high profile — 高画质存档",
            "H264", "slow", "CRF", 18, "MP4", "AAC"),
        p("builtin-h264-archive", "H.264 无损存档", "veryslow, CRF 0 — 最大画质",
            "H264", "veryslow", "CRF", 0, "MKV", "Opus"),

        // --- H.265 软编 ---
        p("builtin-h265-fast", "H.265 快速", "fast, CRF 28 — HEVC 快速",
            "H265", "fast", "CRF", 28, "MKV", "AAC"),
        p("builtin-h265-balanced", "H.265 平衡", "medium, CRF 24 — HEVC 通用",
            "H265", "medium", "CRF", 24, "MKV", "AAC"),
        p("builtin-h265-hq", "H.265 高质量", "slower, CRF 20, main10 — HEVC 高画质",
            "H265", "slower", "CRF", 20, "MKV", "Opus"),

        // --- AV1 软编 ---
        p("builtin-av1", "AV1 通用", "preset 6, CRF 30 — SVT-AV1",
            "AV1", "medium", "CRF", 30, "MKV", "Opus"),

        // --- VP9 软编 ---
        p("builtin-vp9", "VP9 Web", "CRF 30 — Web 优化",
            "VP9", "medium", "CRF", 30, "WebM", "Opus"),

        // --- NVENC 硬编 ---
        hw_p("builtin-nvenc-h264", "NVENC H.264", "h264_nvenc — NVIDIA GPU 加速",
            "H264", "p4", "NVENC", 23),
        hw_p("builtin-nvenc-h265", "NVENC H.265", "hevc_nvenc — NVIDIA GPU 加速",
            "H265", "p4", "NVENC", 28),
        hw_p("builtin-nvenc-av1", "NVENC AV1", "av1_nvenc — NVIDIA RTX 40+",
            "AV1", "p4", "NVENC", 32),

        // --- QSV 硬编 ---
        hw_p("builtin-qsv-h264", "QSV H.264", "h264_qsv — Intel GPU 加速",
            "H264", "medium", "QSV", 23),
        hw_p("builtin-qsv-h265", "QSV H.265", "hevc_qsv — Intel GPU 加速",
            "H265", "medium", "QSV", 26),

        // --- AMF 硬编 ---
        hw_p("builtin-amf-h264", "AMF H.264", "h264_amf — AMD GPU 加速",
            "H264", "balanced", "AMF", 23),
        hw_p("builtin-amf-h265", "AMF H.265", "hevc_amf — AMD GPU 加速",
            "H265", "balanced", "AMF", 26),

        // --- VideoToolbox 硬编 (macOS) ---
        hw_p("builtin-vt-h264", "VideoToolbox H.264", "h264_videotoolbox — Apple 硬件加速",
            "H264", "medium", "VideoToolbox", 23),
        hw_p("builtin-vt-h265", "VideoToolbox H.265", "hevc_videotoolbox — Apple 硬件加速",
            "H265", "medium", "VideoToolbox", 26),
    ]
}

/// 预设默认音频配置
fn audio_json(codec: &str) -> serde_json::Value {
    serde_json::json!({
        "codec": codec,
        "bitrateKbps": 192,
        "channels": 2,
        "sampleRate": 48000
    })
}

/// 软编预设构造辅助函数
#[allow(clippy::too_many_arguments)]
fn p(
    id: &str, name: &str, desc: &str,
    codec: &str, preset: &str, rc: &str, value: u32,
    container: &str, audio: &str,
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
            "audioSettings": audio_json(audio),
            "containerFormat": container,
            "hwAccel": null
        }),
        is_builtin: true,
        created_at: String::new(),
        updated_at: String::new(),
    }
}

/// 硬件加速预设构造辅助函数
fn hw_p(id: &str, name: &str, desc: &str, codec: &str, preset: &str, device: &str, value: u32) -> Preset {
    Preset {
        id: id.into(),
        name: name.into(),
        description: desc.into(),
        config: serde_json::json!({
            "videoCodec": codec,
            "videoSettings": {
                "rateControl": { "type": "CRF", "value": value },
                "encoderPreset": preset,
                "resolution": null,
                "frameRate": null,
                "pixelFormat": null,
                "profile": null,
                "additionalParams": []
            },
            "audioSettings": audio_json("AAC"),
            "containerFormat": if codec == "AV1" { "MKV" } else { "MP4" },
            "hwAccel": { "device": device, "deviceIndex": null }
        }),
        is_builtin: true,
        created_at: String::new(),
        updated_at: String::new(),
    }
}

/// 从持久化存储中加载所有自定义（已导入）预设。
#[tauri::command]
pub async fn load_presets(state: State<'_, crate::AppState>) -> AppResult<Vec<Preset>> {
    match state.preset_manager.as_ref() {
        Some(m) => Ok(m.load()),
        None => Ok(vec![]),
    }
}

/// 按 ID 删除自定义预设。
#[tauri::command]
pub async fn delete_preset(state: State<'_, crate::AppState>, id: String) -> AppResult<()> {
    match state.preset_manager.as_ref() {
        Some(m) => m.delete(&id),
        None => Ok(()),
    }
}

/// 将预设（内置或自定义）序列化为便携的 JSON 格式 { name, description, config }。
fn preset_export_json(state: &crate::AppState, id: &str) -> AppResult<String> {
    // 内置预设来自代码定义
    if let Some(p) = builtin_presets().iter().find(|p| p.id == id) {
        return Ok(serde_json::to_string_pretty(&serde_json::json!({
            "name": p.name,
            "description": p.description,
            "config": p.config,
        }))?);
    }
    // 自定义预设来自 SQLite 数据库
    if let Some(m) = state.preset_manager.as_ref() {
        if let Some(p) = m.get(id) {
            return Ok(serde_json::to_string_pretty(&serde_json::json!({
                "name": p.name,
                "description": p.description,
                "config": p.config,
            }))?);
        }
    }
    let err = AppError::Internal(format!("预设不存在: {id}"));
    let msg = err.to_string();
    let top = msg.lines().next().unwrap_or("unknown").to_string();
    log::warn!("preset export failed reason {top}");
    Err(err)
}

/// 将预设导出为 JSON 字符串（包含名称、描述和配置），便于后续重新导入。
#[tauri::command]
pub async fn export_preset(state: State<'_, crate::AppState>, id: String) -> AppResult<String> {
    crate::analytics::bump(&crate::analytics::COUNTERS.presets_exported, 1);
    preset_export_json(&state, &id)
}

/// 直接将预设导出为指定路径的 JSON 文件。
/// 文件由 Rust 后端直接写入，因此不受前端 fs 插件的作用域限制。
#[tauri::command]
pub async fn export_preset_to_file(
    state: State<'_, crate::AppState>,
    id: String,
    path: String,
) -> AppResult<String> {
    crate::analytics::bump(&crate::analytics::COUNTERS.presets_exported, 1);

    let json = preset_export_json(&state, &id)?;
    if let Err(e) = std::fs::write(&path, json) {
        // 日志只记原因，不记目标全路径
        let err = AppError::Io(e);
        let msg = err.to_string();
        let top = msg.lines().next().unwrap_or("unknown").to_string();
        log::warn!("preset export failed reason {top}");
        return Err(err);
    }
    Ok(path)
}

/// 从 JSON 导入预设并持久化保存到数据库。
/// 支持完整导出格式（{ name, description, config }）或纯编解码配置 JSON。
/// `name`（由前端传入，默认取导入文件的文件名无后缀）优先级高于 JSON 中的名称。
#[tauri::command]
pub async fn import_preset(
    state: State<'_, crate::AppState>,
    json: String,
    name: String,
) -> AppResult<Preset> {
    crate::analytics::bump(&crate::analytics::COUNTERS.presets_imported, 1);

    // 导入失败只记原因首行：不记待导入的 JSON 原文（可能很长）
    let warn_import = |err: &AppError| {
        let msg = err.to_string();
        let top = msg.lines().next().unwrap_or("unknown").to_string();
        log::warn!("preset import failed reason {top}");
    };

    let value: serde_json::Value = match serde_json::from_str(&json) {
        Ok(v) => v,
        Err(e) => {
            let err = AppError::Serialization(e);
            warn_import(&err);
            return Err(err);
        }
    };

    let (config, description) = if let Some(c) = value.get("config") {
        if !c.is_object() {
            let err = AppError::InvalidConfig("预设 config 必须是 JSON 对象".into());
            warn_import(&err);
            return Err(err);
        }
        (
            c.clone(),
            value.get("description").and_then(|d| d.as_str()).unwrap_or("").to_string(),
        )
    } else {
        if !value.is_object() {
            let err = AppError::InvalidConfig("预设 JSON 格式无效".into());
            warn_import(&err);
            return Err(err);
        }
        (value.clone(), String::new())
    };

    // 校验 config 是否符合 EncodeConfig 结构，防止损坏或不兼容的配置入库
    let _validated: crate::encoder::codec::EncodeConfig = match serde_json::from_value(config.clone()) {
        Ok(v) => v,
        Err(e) => {
            let err = AppError::InvalidConfig(format!("预设编码配置解析失败: {}", e.to_string().lines().next().unwrap_or("unknown")));
            warn_import(&err);
            return Err(err);
        }
    };

    let preset_name = if name.trim().is_empty() {
        value.get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("导入的预设")
            .to_string()
    } else {
        name.trim().to_string()
    };

    let manager = match state.preset_manager.as_ref() {
        Some(m) => m,
        None => {
            let err = AppError::Internal("Preset store not initialized".into());
            warn_import(&err);
            return Err(err);
        }
    };

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
    if let Err(e) = manager.insert(&preset) {
        warn_import(&e);
        return Err(e);
    }
    Ok(preset)
}

#[tauri::command]
pub async fn get_builtin_presets() -> AppResult<Vec<Preset>> {
    Ok(builtin_presets())
}
