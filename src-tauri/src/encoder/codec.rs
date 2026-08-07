use serde::{Deserialize, Serialize};

/// Video encoder configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncodeConfig {
    pub video_codec: VideoCodec,
    pub video_settings: VideoSettings,
    pub audio_settings: AudioSettings,
    pub container_format: ContainerFormat,
    pub hw_accel: Option<HwAccelConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum VideoCodec {
    #[serde(rename = "H264")]
    H264,
    #[serde(rename = "H265")]
    H265,
    #[serde(rename = "AV1")]
    AV1,
    #[serde(rename = "VP9")]
    VP9,
    #[serde(rename = "Copy")]
    Copy,
}

impl VideoCodec {
    /// Map to FFmpeg encoder name
    pub fn encoder_name(&self, hw: Option<&HwAccelConfig>) -> &'static str {
        match hw {
            Some(HwAccelConfig { device: HwAccelDevice::NVENC, .. }) => match self {
                VideoCodec::H264 => "h264_nvenc",
                VideoCodec::H265 => "hevc_nvenc",
                VideoCodec::AV1 => "av1_nvenc",
                _ => self.software_encoder(),
            },
            Some(HwAccelConfig { device: HwAccelDevice::QSV, .. }) => match self {
                VideoCodec::H264 => "h264_qsv",
                VideoCodec::H265 => "hevc_qsv",
                VideoCodec::AV1 => "av1_qsv",
                _ => self.software_encoder(),
            },
            Some(HwAccelConfig { device: HwAccelDevice::AMF, .. }) => match self {
                VideoCodec::H264 => "h264_amf",
                VideoCodec::H265 => "hevc_amf",
                _ => self.software_encoder(),
            },
            Some(HwAccelConfig { device: HwAccelDevice::VideoToolbox, .. }) => match self {
                VideoCodec::H264 => "h264_videotoolbox",
                VideoCodec::H265 => "hevc_videotoolbox",
                _ => self.software_encoder(),
            },
            Some(HwAccelConfig { device: HwAccelDevice::VAAPI, .. }) => match self {
                VideoCodec::H264 => "h264_vaapi",
                VideoCodec::H265 => "hevc_vaapi",
                VideoCodec::AV1 => "av1_vaapi",
                _ => self.software_encoder(),
            },
            None => self.software_encoder(),
        }
    }

    pub fn software_encoder(&self) -> &'static str {
        match self {
            VideoCodec::H264 => "libx264",
            VideoCodec::H265 => "libx265",
            VideoCodec::AV1 => "libsvtav1",
            VideoCodec::VP9 => "libvpx-vp9",
            VideoCodec::Copy => "copy",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoSettings {
    pub rate_control: RateControl,
    pub encoder_preset: String,
    pub resolution: Option<Resolution>,
    pub frame_rate: Option<f64>,
    pub pixel_format: Option<String>,
    pub profile: Option<String>,
    pub additional_params: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum RateControl {
    #[serde(rename = "CRF")]
    Crf { value: u8 },
    #[serde(rename = "CQP")]
    Cqp { value: u32 },
    #[serde(rename = "ABR")]
    Abr {
        bitrate_kbps: u32,
        max_bitrate_kbps: Option<u32>,
    },
}

impl RateControl {
    /// Build FFmpeg CLI arguments for rate control
    pub fn to_args(&self) -> Vec<String> {
        match self {
            RateControl::Crf { value } => vec!["-crf".into(), value.to_string()],
            RateControl::Cqp { value } => vec!["-qp".into(), value.to_string()],
            RateControl::Abr {
                bitrate_kbps,
                max_bitrate_kbps,
            } => {
                let mut args = vec!["-b:v".into(), format!("{}k", bitrate_kbps)];
                if let Some(max) = max_bitrate_kbps {
                    args.push("-maxrate".into());
                    args.push(format!("{}k", max));
                    args.push("-bufsize".into());
                    args.push(format!("{}k", max * 2));
                }
                args
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioSettings {
    pub codec: String, // "AAC", "Opus", "Copy", "None"
    pub bitrate_kbps: u32,
    pub channels: u32,
    pub sample_rate: u32,
}

impl AudioSettings {
    pub fn to_args(&self) -> Vec<String> {
        match self.codec.as_str() {
            "Copy" => vec!["-c:a".into(), "copy".into()],
            "None" => vec!["-an".into()],
            _ => {
                let codec = match self.codec.as_str() {
                    "AAC" => "aac",
                    "Opus" => "libopus",
                    other => other,
                };
                vec![
                    "-c:a".into(),
                    codec.into(),
                    "-b:a".into(),
                    format!("{}k", self.bitrate_kbps),
                    "-ac".into(),
                    self.channels.to_string(),
                    "-ar".into(),
                    self.sample_rate.to_string(),
                ]
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ContainerFormat {
    #[serde(rename = "MP4")]
    MP4,
    #[serde(rename = "MKV")]
    MKV,
    #[serde(rename = "WebM")]
    WebM,
    #[serde(rename = "MOV")]
    MOV,
}

impl ContainerFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            ContainerFormat::MP4 => "mp4",
            ContainerFormat::MKV => "mkv",
            ContainerFormat::WebM => "webm",
            ContainerFormat::MOV => "mov",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HwAccelConfig {
    pub device: HwAccelDevice,
    pub device_index: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum HwAccelDevice {
    #[serde(rename = "NVENC")]
    NVENC,
    #[serde(rename = "AMF")]
    AMF,
    #[serde(rename = "QSV")]
    QSV,
    #[serde(rename = "VideoToolbox")]
    VideoToolbox,
    #[serde(rename = "VAAPI")]
    VAAPI,
}
