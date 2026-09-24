use serde::{Deserialize, Serialize};
use crate::ffmpeg;

/// 硬件加速器类型及其编码器前缀和候选编解码器
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
    pub codec: String,       // "h264", "hevc", "av1"（编解码器名称）
    pub encoder: String,     // "h264_nvenc", "hevc_nvenc" 等（FFmpeg 编码器）
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

    /// 判断特定 GPU 代次是否支持硬件 AV1 编码
    pub fn supports_av1(&self) -> bool {
        let name_l = self.name.to_lowercase();
        match self.vendor() {
            GpuVendor::Nvidia => {
                // Ada Lovelace（RTX 40 系列、RTX 4000/4500/5000/6000 Ada、L4/L40）和 Blackwell（RTX 50 系列）
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
                // RDNA 3 / 3.5 / 4：RX 7000 系列、RX 8000、Radeon 780M、880M、890M、Radeon Pro W7000
                name_l.contains("rx 7")
                    || name_l.contains("rx 8")
                    || name_l.contains("780m")
                    || name_l.contains("880m")
                    || name_l.contains("890m")
                    || name_l.contains("w7")
            }
            GpuVendor::Intel => {
                // Intel Arc Alchemist / Battlemage、Core Ultra（Meteor Lake、Lunar Lake、Arrow Lake）
                name_l.contains("arc")
                    || (name_l.contains("core") && name_l.contains("ultra"))
                    || name_l.contains("battlemage")
            }
            GpuVendor::Apple => {
                // Apple M3、M4
                name_l.contains("m3") || name_l.contains("m4")
            }
            GpuVendor::Other => false,
        }
    }
}

/// 跨平台检测系统 GPU
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

/// 通过查询 ffmpeg 并结合物理检测到的 GPU 及平台能力，
/// 检测所有可用的硬件加速器。
pub fn detect_all(cpu_brand: &str, platform: &str) -> Vec<HwAccelInfo> {
    let gpus = detect_system_gpus();
    detect_all_with_gpus(cpu_brand, platform, &gpus)
}

/// 内部检测逻辑，允许注入模拟 GPU 和编码器以进行单元测试
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
        // 系统 GPU 探测成功时查找匹配的 GPU
        let matched_gpu = gpus.iter().find(|g| match device_name {
            "NVENC" => g.vendor() == GpuVendor::Nvidia,
            "AMF" => g.vendor() == GpuVendor::Amd,
            "QSV" => g.vendor() == GpuVendor::Intel,
            "VideoToolbox" => g.vendor() == GpuVendor::Apple || platform == "macos",
            _ => false,
        });

        // 判定可用性
        let is_available = if !gpus.is_empty() {
            // 高精度 GPU 硬件检测
            match device_name {
                "NVENC" => matched_gpu.is_some() && (platform == "windows" || platform == "linux"),
                "AMF" => matched_gpu.is_some() && (platform == "windows" || platform == "linux"),
                "QSV" => matched_gpu.is_some() && (platform == "windows" || platform == "linux"),
                "VideoToolbox" => platform == "macos",
                "VAAPI" => platform == "linux",
                _ => false,
            }
        } else {
            // 若 GPU 注册表/sysfs 查询为空，回退到基于 CPU/平台的启发式检测
            platform_supports_device_fallback(device_name, cpu_brand, platform)
        };

        // 结合 FFmpeg 编译支持与物理 GPU 代次共同过滤支持的编解码器
        let supported: Vec<HwCodecInfo> = if is_available {
            codecs
                .iter()
                .filter_map(|&codec| {
                    // 检查物理 GPU 是否支持 AV1（例如 RTX 30 系列无 av1_nvenc 硬件支持）
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

/// 当系统 GPU 探测不可用时的回退启发式检测
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

/// 查询 ffmpeg -encoders 并解析编码器列表
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
        .filter(|l| l.starts_with(" V"))  // 仅保留视频编码器
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

/// 设备类型回退时的通用易读 GPU 名称
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
        // NVIDIA：RTX 40/50 系列支持 AV1；RTX 30 / 20 / 10 系列不支持
        assert!(DiscoveredGpu::new("NVIDIA GeForce RTX 4090", "").supports_av1());
        assert!(DiscoveredGpu::new("NVIDIA GeForce RTX 4060 Ti", "").supports_av1());
        assert!(DiscoveredGpu::new("NVIDIA RTX 4000 Ada Generation", "").supports_av1());
        assert!(DiscoveredGpu::new("NVIDIA GeForce RTX 5080", "").supports_av1());
        assert!(!DiscoveredGpu::new("NVIDIA GeForce RTX 3080", "").supports_av1());
        assert!(!DiscoveredGpu::new("NVIDIA GeForce RTX 3060", "").supports_av1());
        assert!(!DiscoveredGpu::new("NVIDIA GeForce RTX 2070 Super", "").supports_av1());
        assert!(!DiscoveredGpu::new("NVIDIA GeForce GTX 1660 Super", "").supports_av1());

        // AMD：RX 7000 / 8000 及 780M/880M/890M 支持 AV1；RX 6000 / 5000 不支持
        assert!(DiscoveredGpu::new("AMD Radeon RX 7900 XTX", "").supports_av1());
        assert!(DiscoveredGpu::new("AMD Radeon RX 7600", "").supports_av1());
        assert!(DiscoveredGpu::new("AMD Radeon 780M Graphics", "").supports_av1());
        assert!(!DiscoveredGpu::new("AMD Radeon RX 6800 XT", "").supports_av1());
        assert!(!DiscoveredGpu::new("AMD Radeon RX 6700 XT", "").supports_av1());
        assert!(!DiscoveredGpu::new("AMD Radeon RX 5700 XT", "").supports_av1());

        // Intel：Arc 和 Core Ultra 支持 AV1；UHD 770 不支持
        assert!(DiscoveredGpu::new("Intel(R) Arc(TM) A770 Graphics", "").supports_av1());
        assert!(DiscoveredGpu::new("Intel(R) Arc(TM) A380 Graphics", "").supports_av1());
        assert!(DiscoveredGpu::new("Intel(R) Core(TM) Ultra 7 155H", "").supports_av1());
        assert!(!DiscoveredGpu::new("Intel(R) UHD Graphics 770", "").supports_av1());

        // Apple：M3 / M4 支持 AV1；M1 / M2 不支持
        assert!(DiscoveredGpu::new("Apple M3 Pro", "").supports_av1());
        assert!(DiscoveredGpu::new("Apple M4 Max", "").supports_av1());
        assert!(!DiscoveredGpu::new("Apple M1 Max", "").supports_av1());
        assert!(!DiscoveredGpu::new("Apple M2 Pro", "").supports_av1());
    }

    #[test]
    fn test_detect_all_filters_codecs_and_device_availability() {
        // 仅配备 RTX 3080 的设备（Windows）
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
        // 即使 ffmpeg 包含 av1_nvenc，RTX 3080 也绝不能在支持列表中包含 AV1
        let codecs: Vec<&str> = nvenc.supported_codecs.iter().map(|c| c.codec.as_str()).collect();
        assert!(!codecs.contains(&"av1"), "RTX 3080 should not support AV1: {codecs:?}");
        assert!(codecs.contains(&"h264"));
        assert!(codecs.contains(&"hevc"));

        // 即使 CPU 是 AMD Ryzen，因无 AMD GPU，AMF 也必须不可用
        let amf = results.iter().find(|r| r.device == "AMF").unwrap();
        assert!(!amf.available, "AMF must be unavailable when no AMD GPU is installed");

        // QSV 必须不可用
        let qsv = results.iter().find(|r| r.device == "QSV").unwrap();
        assert!(!qsv.available);
    }

    #[test]
    fn test_real_system_gpu_detection_runs_without_panic() {
        let gpus = detect_system_gpus();
        // 仅断言不打印：测试输出禁 println!（日志收敛）
        // 在 Windows 或配备 GPU 的系统上，检测 GPU 应正常运行且不崩溃
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
