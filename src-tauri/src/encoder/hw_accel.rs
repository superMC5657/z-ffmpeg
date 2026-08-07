use serde::{Deserialize, Serialize};
use crate::ffmpeg;

/// Hardware accelerator types and their encoder prefixes
const HW_ENCODERS: &[(&str, &str, &[&str])] = &[
    ("NVENC", "nvenc", &["h264", "hevc", "av1"]),
    ("AMF", "amf", &["h264", "hevc", "av1"]),
    ("QSV", "qsv", &["h264", "hevc", "av1", "mpeg2", "vp9"]),
    ("VAAPI", "vaapi", &["h264", "hevc", "av1", "mpeg2", "vp8", "vp9"]),
    ("VideoToolbox", "videotoolbox", &["h264", "hevc", "prores"]),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HwAccelInfo {
    pub device: String,
    pub available: bool,
    pub device_name: String,
    pub supported_codecs: Vec<HwCodecInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HwCodecInfo {
    pub codec: String,       // "h264", "hevc", "av1"
    pub encoder: String,     // "h264_nvenc", "hevc_nvenc", etc.
}

/// Detect all available hardware accelerators by querying ffmpeg,
/// then apply platform / CPU constraints (e.g. QSV needs an Intel CPU,
/// VideoToolbox only exists on macOS, NVENC/AMF not on macOS).
pub fn detect_all(cpu_brand: &str, platform: &str) -> Vec<HwAccelInfo> {
    let ffmpeg_path = ffmpeg::get_ffmpeg_path()
        .or_else(|| ffmpeg::get_ffprobe_path());

    let available_encoders = match ffmpeg_path {
        Some(path) => query_encoders(&path),
        None => return vec![],
    };

    let mut results = Vec::new();

    for &(device_name, suffix, codecs) in HW_ENCODERS {
        let supported: Vec<HwCodecInfo> = codecs.iter()
            .filter_map(|&codec| {
                let encoder_name = format!("{}_{}", codec, suffix);
                if available_encoders.contains(&encoder_name) {
                    Some(HwCodecInfo {
                        codec: codec.to_string(),
                        encoder: encoder_name,
                    })
                } else {
                    None
                }
            })
            .collect();

        let ffmpeg_supported = !supported.is_empty();
        // ffmpeg 列出的编码器只是「编译支持」,不代表当前硬件/平台能用
        let available = ffmpeg_supported && platform_supports_device(device_name, cpu_brand, platform);

        // Get GPU name for the device
        let gpu_name = get_gpu_name_for_device(device_name);

        results.push(HwAccelInfo {
            device: device_name.to_string(),
            available,
            device_name: if available { gpu_name } else { String::new() },
            supported_codecs: supported,
        });
    }

    results
}

/// Whether the current platform/CPU can actually run this hardware encoder.
fn platform_supports_device(device: &str, cpu_brand: &str, platform: &str) -> bool {
    let cpu = cpu_brand.to_lowercase();
    let is_intel = cpu.contains("intel");
    let is_amd = cpu.contains("amd") || cpu.contains("ryzen") || cpu.contains("athlon");
    match device {
        // QSV (Quick Sync Video) is Intel-only: an AMD CPU cannot use it,
        // even if the ffmpeg binary was compiled with qsv encoders.
        "QSV" => is_intel,
        // VideoToolbox is Apple-only.
        "VideoToolbox" => platform == "macos",
        // VAAPI is Linux-only.
        "VAAPI" => platform == "linux",
        // NVENC is NVIDIA-only and never exists on macOS.
        "NVENC" => platform == "windows" || platform == "linux",
        // AMF is AMD's encoder; approximate with CPU vendor (GPU detection is unreliable).
        "AMF" => (platform == "windows" || platform == "linux") && is_amd,
        _ => true,
    }
}

/// Query ffmpeg -encoders and parse the list
fn query_encoders(ffmpeg_path: &std::path::PathBuf) -> Vec<String> {
    let output = match ffmpeg::hidden_command(ffmpeg_path)
        .args(["-encoders"])
        .output()
    {
        Ok(o) => o,
        Err(_) => return vec![],
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .filter(|l| l.starts_with(" V"))  // Video encoders
        .filter_map(|l| {
            let parts: Vec<&str> = l.split_whitespace().collect();
            if parts.len() >= 3 {
                Some(parts[1].to_string())
            } else {
                None
            }
        })
        .collect()
}

/// Get human-readable GPU name for a device type
fn get_gpu_name_for_device(device: &str) -> String {
    match device {
        "NVENC" => {
            // Try to get NVIDIA GPU name via DXGI or similar
            // For now return a generic name
            "NVIDIA GPU (NVENC)".into()
        }
        "AMF" => "AMD GPU (AMF)".into(),
        "QSV" => "Intel GPU (Quick Sync)".into(),
        "VAAPI" => "VAAPI Device".into(),
        "VideoToolbox" => "Apple VideoToolbox".into(),
        _ => device.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::platform_supports_device;

    #[test]
    fn qsv_requires_intel_cpu() {
        // AMD CPU: QSV must NOT be available even if ffmpeg lists qsv encoders
        assert!(!platform_supports_device("QSV", "AMD Ryzen 7 5800X", "windows"));
        assert!(!platform_supports_device("QSV", "AMD Ryzen 7 5800X", "linux"));
        // Intel CPU: QSV available on windows/linux
        assert!(platform_supports_device("QSV", "Intel(R) Core(TM) i7-12700K", "windows"));
        assert!(platform_supports_device("QSV", "Intel(R) Core(TM) i7-12700K", "linux"));
        // Unknown CPU: conservatively not available
        assert!(!platform_supports_device("QSV", "Unknown CPU", "windows"));
    }

    #[test]
    fn videotoolbox_is_macos_only() {
        assert!(platform_supports_device("VideoToolbox", "Apple M1", "macos"));
        assert!(!platform_supports_device("VideoToolbox", "Apple M1", "windows"));
        assert!(!platform_supports_device("VideoToolbox", "Intel(R) Core(TM) i7", "windows"));
    }

    #[test]
    fn nvenc_amf_platform_and_vendor() {
        // NVENC: windows/linux ok, never macOS
        assert!(platform_supports_device("NVENC", "AMD Ryzen 7 5800X", "windows"));
        assert!(platform_supports_device("NVENC", "Intel(R) Core(TM) i7", "linux"));
        assert!(!platform_supports_device("NVENC", "AMD Ryzen 7 5800X", "macos"));
        // AMF: AMD CPU + windows/linux
        assert!(platform_supports_device("AMF", "AMD Ryzen 7 5800X", "windows"));
        assert!(!platform_supports_device("AMF", "Intel(R) Core(TM) i7", "windows"));
        assert!(!platform_supports_device("AMF", "AMD Ryzen 7 5800X", "macos"));
    }

    #[test]
    fn vaapi_is_linux_only() {
        assert!(platform_supports_device("VAAPI", "AMD Ryzen 7 5800X", "linux"));
        assert!(!platform_supports_device("VAAPI", "AMD Ryzen 7 5800X", "windows"));
        assert!(!platform_supports_device("VAAPI", "AMD Ryzen 7 5800X", "macos"));
    }
}
