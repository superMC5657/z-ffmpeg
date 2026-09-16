use serde::{Deserialize, Serialize};
use crate::ffmpeg;

/// Hardware accelerator types and their encoder prefixes and candidate codecs
const HW_ENCODERS: &[(&str, &str, &[&str])] = &[
    ("NVENC", "nvenc", &["h264", "hevc", "av1"]),
    ("AMF", "amf", &["h264", "hevc", "av1"]),
    ("QSV", "qsv", &["h264", "hevc", "av1"]),
    ("VAAPI", "vaapi", &["h264", "hevc", "av1"]),
    ("VideoToolbox", "videotoolbox", &["h264", "hevc", "av1"]),
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

#[derive(Debug, Clone, PartialEq)]
pub enum GpuVendor {
    Nvidia,
    Amd,
    Intel,
    Apple,
    Other,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveredGpu {
    pub name: String,
    pub device_id: String,
}

impl DiscoveredGpu {
    pub fn new(name: impl Into<String>, device_id: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            device_id: device_id.into(),
        }
    }

    pub fn vendor(&self) -> GpuVendor {
        let name_l = self.name.to_lowercase();
        let id_l = self.device_id.to_lowercase();
        if name_l.contains("nvidia") || name_l.contains("geforce") || id_l.contains("ven_10de") {
            GpuVendor::Nvidia
        } else if name_l.contains("amd") || name_l.contains("radeon") || id_l.contains("ven_1002") {
            GpuVendor::Amd
        } else if name_l.contains("intel") || name_l.contains("arc") || id_l.contains("ven_8086") {
            GpuVendor::Intel
        } else if name_l.contains("apple") {
            GpuVendor::Apple
        } else {
            GpuVendor::Other
        }
    }

    /// Whether this specific GPU generation supports hardware AV1 encode
    pub fn supports_av1(&self) -> bool {
        let name_l = self.name.to_lowercase();
        match self.vendor() {
            GpuVendor::Nvidia => {
                // Ada Lovelace (RTX 40-series, RTX 4000/4500/5000/6000 Ada, L4/L40) and Blackwell (RTX 50-series)
                name_l.contains("rtx 40")
                    || name_l.contains("rtx 50")
                    || name_l.contains("ada")
                    || name_l.contains("rtx 4000")
                    || name_l.contains("rtx 4500")
                    || name_l.contains("rtx 5000")
                    || name_l.contains("rtx 6000")
                    || name_l.contains("l4")
                    || name_l.contains("l40")
            }
            GpuVendor::Amd => {
                // RDNA 3 / 3.5 / 4: RX 7000-series, RX 8000, Radeon 780M, 880M, 890M, Radeon Pro W7000
                name_l.contains("rx 7")
                    || name_l.contains("rx 8")
                    || name_l.contains("780m")
                    || name_l.contains("880m")
                    || name_l.contains("890m")
                    || name_l.contains("w7")
            }
            GpuVendor::Intel => {
                // Intel Arc Alchemist / Battlemage, Core Ultra (Meteor Lake, Lunar Lake, Arrow Lake)
                name_l.contains("arc")
                    || (name_l.contains("core") && name_l.contains("ultra"))
                    || name_l.contains("battlemage")
            }
            GpuVendor::Apple => {
                // Apple M3, M4
                name_l.contains("m3") || name_l.contains("m4")
            }
            GpuVendor::Other => false,
        }
    }
}

/// Detect system GPUs across platforms
pub fn detect_system_gpus() -> Vec<DiscoveredGpu> {
    #[cfg(target_os = "windows")]
    {
        use winreg::enums::*;
        use winreg::RegKey;

        let mut gpus = Vec::new();
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let class_path = r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";
        if let Ok(class_key) = hklm.open_subkey(class_path) {
            for subkey_name in class_key.enum_keys().filter_map(|r| r.ok()) {
                if subkey_name.starts_with("0") {
                    if let Ok(sub) = class_key.open_subkey(&subkey_name) {
                        let desc: String = sub.get_value("DriverDesc").unwrap_or_default();
                        let dev_id: String = sub.get_value("MatchingDeviceId").unwrap_or_default();
                        if desc.is_empty() {
                            continue;
                        }
                        let desc_lower = desc.to_lowercase();
                        if desc_lower.contains("virtual")
                            || desc_lower.contains("parsec")
                            || desc_lower.contains("basic display")
                            || desc_lower.contains("remote display")
                            || desc_lower.contains("iddsample")
                            || desc_lower.contains("spacedesk")
                            || desc_lower.contains("vnc")
                        {
                            continue;
                        }
                        gpus.push(DiscoveredGpu::new(desc, dev_id));
                    }
                }
            }
        }
        gpus
    }

    #[cfg(target_os = "linux")]
    {
        let mut gpus = Vec::new();
        if let Ok(entries) = std::fs::read_dir("/sys/class/drm") {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                let file_name = entry.file_name().to_string_lossy().to_string();
                if file_name.starts_with("card") && !file_name.contains('-') {
                    let vendor_path = path.join("device/vendor");
                    if let Ok(vendor_hex) = std::fs::read_to_string(&vendor_path) {
                        let vendor_trimmed = vendor_hex.trim().to_lowercase();
                        let (name, dev_id) = match vendor_trimmed.as_str() {
                            "0x10de" => ("NVIDIA GPU", "pci\\ven_10de"),
                            "0x1002" => ("AMD Radeon GPU", "pci\\ven_1002"),
                            "0x8086" => ("Intel Graphics", "pci\\ven_8086"),
                            _ => continue,
                        };
                        gpus.push(DiscoveredGpu::new(name, dev_id));
                    }
                }
            }
        }
        gpus
    }

    #[cfg(target_os = "macos")]
    {
        let mut gpus = Vec::new();
        if let Ok(output) = std::process::Command::new("sysctl")
            .args(["-n", "machdep.cpu.brand_string"])
            .output()
        {
            let brand = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !brand.is_empty() {
                gpus.push(DiscoveredGpu::new(format!("Apple {}", brand), "apple_silicon"));
            } else {
                gpus.push(DiscoveredGpu::new("Apple Silicon", "apple_silicon"));
            }
        } else {
            gpus.push(DiscoveredGpu::new("Apple Silicon", "apple_silicon"));
        }
        gpus
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        Vec::new()
    }
}

/// Detect all available hardware accelerators by querying ffmpeg,
/// then matching against physically discovered GPUs and platform capabilities.
pub fn detect_all(cpu_brand: &str, platform: &str) -> Vec<HwAccelInfo> {
    let gpus = detect_system_gpus();
    detect_all_with_gpus(cpu_brand, platform, &gpus)
}

/// Inner detection logic allowing mock GPU and encoder injection for unit tests
pub fn detect_all_with_gpus(
    cpu_brand: &str,
    platform: &str,
    gpus: &[DiscoveredGpu],
) -> Vec<HwAccelInfo> {
    let available_encoders = match ffmpeg::get_ffmpeg_path() {
        Some(path) => query_encoders(&path),
        None => vec![],
    };
    detect_all_inner(cpu_brand, platform, gpus, &available_encoders)
}

pub fn detect_all_inner(
    cpu_brand: &str,
    platform: &str,
    gpus: &[DiscoveredGpu],
    available_encoders: &[String],
) -> Vec<HwAccelInfo> {
    let mut results = Vec::new();

    for &(device_name, suffix, codecs) in HW_ENCODERS {
        // Find matching GPU if system GPU discovery succeeded
        let matched_gpu = gpus.iter().find(|g| match device_name {
            "NVENC" => g.vendor() == GpuVendor::Nvidia,
            "AMF" => g.vendor() == GpuVendor::Amd,
            "QSV" => g.vendor() == GpuVendor::Intel,
            "VideoToolbox" => g.vendor() == GpuVendor::Apple || platform == "macos",
            _ => false,
        });

        // Determine availability
        let is_available = if !gpus.is_empty() {
            // High-precision GPU hardware detection
            match device_name {
                "NVENC" => matched_gpu.is_some() && (platform == "windows" || platform == "linux"),
                "AMF" => matched_gpu.is_some() && (platform == "windows" || platform == "linux"),
                "QSV" => matched_gpu.is_some() && (platform == "windows" || platform == "linux"),
                "VideoToolbox" => platform == "macos",
                "VAAPI" => platform == "linux",
                _ => false,
            }
        } else {
            // Fallback to CPU/platform heuristic if GPU registry/sysfs query was empty
            platform_supports_device_fallback(device_name, cpu_brand, platform)
        };

        // Filter supported codecs based on both FFmpeg compilation AND physical GPU generation
        let supported: Vec<HwCodecInfo> = if is_available {
            codecs
                .iter()
                .filter_map(|&codec| {
                    // Check if physical GPU supports AV1 (e.g. RTX 30-series has NO av1_nvenc)
                    if codec == "av1" {
                        if let Some(gpu) = matched_gpu {
                            if !gpu.supports_av1() {
                                return None;
                            }
                        }
                    }

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
                .collect()
        } else {
            Vec::new()
        };

        let final_available = is_available && !supported.is_empty();

        let display_name = if final_available {
            matched_gpu
                .map(|g| g.name.clone())
                .unwrap_or_else(|| get_default_gpu_name(device_name))
        } else {
            String::new()
        };

        results.push(HwAccelInfo {
            device: device_name.to_string(),
            available: final_available,
            device_name: display_name,
            supported_codecs: supported,
        });
    }

    results
}

/// Fallback heuristic when system GPU discovery is unavailable
fn platform_supports_device_fallback(device: &str, cpu_brand: &str, platform: &str) -> bool {
    let cpu = cpu_brand.to_lowercase();
    let is_intel = cpu.contains("intel");
    let is_amd = cpu.contains("amd") || cpu.contains("ryzen") || cpu.contains("athlon");
    match device {
        "QSV" => (platform == "windows" || platform == "linux") && is_intel,
        "VideoToolbox" => platform == "macos",
        "VAAPI" => platform == "linux",
        "NVENC" => platform == "windows" || platform == "linux",
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

/// Generic human-readable GPU name for a device type fallback
fn get_default_gpu_name(device: &str) -> String {
    match device {
        "NVENC" => "NVIDIA GPU (NVENC)".into(),
        "AMF" => "AMD GPU (AMF)".into(),
        "QSV" => "Intel GPU (Quick Sync)".into(),
        "VAAPI" => "VAAPI Device".into(),
        "VideoToolbox" => "Apple VideoToolbox".into(),
        _ => device.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_vendor_detection() {
        let nvidia = DiscoveredGpu::new("NVIDIA GeForce RTX 4090", "pci\\ven_10de&dev_2684");
        assert_eq!(nvidia.vendor(), GpuVendor::Nvidia);

        let amd = DiscoveredGpu::new("AMD Radeon RX 7900 XTX", "pci\\ven_1002&dev_744c");
        assert_eq!(amd.vendor(), GpuVendor::Amd);

        let intel = DiscoveredGpu::new("Intel(R) Arc(TM) A770 Graphics", "pci\\ven_8086&dev_56a0");
        assert_eq!(intel.vendor(), GpuVendor::Intel);

        let apple = DiscoveredGpu::new("Apple M3 Max", "apple_silicon");
        assert_eq!(apple.vendor(), GpuVendor::Apple);
    }

    #[test]
    fn gpu_av1_support_matrix() {
        // NVIDIA: RTX 40/50 series support AV1; RTX 30 / 20 / 10 series do NOT
        assert!(DiscoveredGpu::new("NVIDIA GeForce RTX 4090", "").supports_av1());
        assert!(DiscoveredGpu::new("NVIDIA GeForce RTX 4060 Ti", "").supports_av1());
        assert!(DiscoveredGpu::new("NVIDIA RTX 4000 Ada Generation", "").supports_av1());
        assert!(DiscoveredGpu::new("NVIDIA GeForce RTX 5080", "").supports_av1());
        assert!(!DiscoveredGpu::new("NVIDIA GeForce RTX 3080", "").supports_av1());
        assert!(!DiscoveredGpu::new("NVIDIA GeForce RTX 3060", "").supports_av1());
        assert!(!DiscoveredGpu::new("NVIDIA GeForce RTX 2070 Super", "").supports_av1());
        assert!(!DiscoveredGpu::new("NVIDIA GeForce GTX 1660 Super", "").supports_av1());

        // AMD: RX 7000 / 8000 and 780M/880M/890M support AV1; RX 6000 / 5000 do NOT
        assert!(DiscoveredGpu::new("AMD Radeon RX 7900 XTX", "").supports_av1());
        assert!(DiscoveredGpu::new("AMD Radeon RX 7600", "").supports_av1());
        assert!(DiscoveredGpu::new("AMD Radeon 780M Graphics", "").supports_av1());
        assert!(!DiscoveredGpu::new("AMD Radeon RX 6800 XT", "").supports_av1());
        assert!(!DiscoveredGpu::new("AMD Radeon RX 6700 XT", "").supports_av1());
        assert!(!DiscoveredGpu::new("AMD Radeon RX 5700 XT", "").supports_av1());

        // Intel: Arc and Core Ultra support AV1; UHD 770 does NOT
        assert!(DiscoveredGpu::new("Intel(R) Arc(TM) A770 Graphics", "").supports_av1());
        assert!(DiscoveredGpu::new("Intel(R) Arc(TM) A380 Graphics", "").supports_av1());
        assert!(DiscoveredGpu::new("Intel(R) Core(TM) Ultra 7 155H", "").supports_av1());
        assert!(!DiscoveredGpu::new("Intel(R) UHD Graphics 770", "").supports_av1());

        // Apple: M3 / M4 support AV1; M1 / M2 do NOT
        assert!(DiscoveredGpu::new("Apple M3 Pro", "").supports_av1());
        assert!(DiscoveredGpu::new("Apple M4 Max", "").supports_av1());
        assert!(!DiscoveredGpu::new("Apple M1 Max", "").supports_av1());
        assert!(!DiscoveredGpu::new("Apple M2 Pro", "").supports_av1());
    }

    #[test]
    fn test_detect_all_filters_codecs_and_device_availability() {
        // Machine with only an RTX 3080 (Windows)
        let rtx3080 = vec![DiscoveredGpu::new(
            "NVIDIA GeForce RTX 3080",
            "pci\\ven_10de&dev_2206",
        )];
        let mock_encoders = vec![
            "h264_nvenc".into(),
            "hevc_nvenc".into(),
            "av1_nvenc".into(),
            "h264_amf".into(),
            "hevc_amf".into(),
            "h264_qsv".into(),
        ];
        let results = detect_all_inner("AMD Ryzen 7 5800X", "windows", &rtx3080, &mock_encoders);
        let nvenc = results.iter().find(|r| r.device == "NVENC").unwrap();
        assert!(nvenc.available);
        assert_eq!(nvenc.device_name, "NVIDIA GeForce RTX 3080");
        // RTX 3080 must NOT have AV1 in supported codecs even if ffmpeg has av1_nvenc
        let codecs: Vec<&str> = nvenc.supported_codecs.iter().map(|c| c.codec.as_str()).collect();
        assert!(!codecs.contains(&"av1"), "RTX 3080 should not support AV1: {codecs:?}");
        assert!(codecs.contains(&"h264"));
        assert!(codecs.contains(&"hevc"));

        // AMF must be unavailable even though CPU is AMD Ryzen
        let amf = results.iter().find(|r| r.device == "AMF").unwrap();
        assert!(!amf.available, "AMF must be unavailable when no AMD GPU is installed");

        // QSV must be unavailable
        let qsv = results.iter().find(|r| r.device == "QSV").unwrap();
        assert!(!qsv.available);
    }

    #[test]
    fn test_real_system_gpu_detection_runs_without_panic() {
        let gpus = detect_system_gpus();
        // 仅断言不打印：测试输出禁 println!（日志收敛）
        // On Windows or systems with GPUs, it should find GPUs without crashing
        for gpu in &gpus {
            assert!(!gpu.name.is_empty());
        }
    }

    #[test]
    fn fallback_platform_heuristics() {
        assert!(!platform_supports_device_fallback("QSV", "AMD Ryzen 7 5800X", "windows"));
        assert!(platform_supports_device_fallback("QSV", "Intel(R) Core(TM) i7-12700K", "windows"));
        assert!(platform_supports_device_fallback("VideoToolbox", "Apple M1", "macos"));
        assert!(!platform_supports_device_fallback("VideoToolbox", "Apple M1", "windows"));
        assert!(platform_supports_device_fallback("NVENC", "AMD Ryzen 7 5800X", "windows"));
        assert!(!platform_supports_device_fallback("NVENC", "AMD Ryzen 7 5800X", "macos"));
        assert!(platform_supports_device_fallback("VAAPI", "AMD Ryzen 7 5800X", "linux"));
        assert!(!platform_supports_device_fallback("VAAPI", "AMD Ryzen 7 5800X", "windows"));
    }
}
