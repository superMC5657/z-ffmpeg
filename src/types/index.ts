// ============================================================
// Codec types
// ============================================================

export type VideoCodec = "H264" | "H265" | "AV1" | "VP9";

export type AudioCodec = "AAC" | "Opus" | "Copy" | "None";

export type ContainerFormat = "MP4" | "MKV" | "WebM" | "MOV";

export type HwAccelDevice = "NVENC" | "AMF" | "QSV" | "VideoToolbox" | "VAAPI";

export type EncoderPreset =
  | "ultrafast"
  | "superfast"
  | "veryfast"
  | "faster"
  | "fast"
  | "medium"
  | "slow"
  | "slower"
  | "veryslow";

export type RateControl =
  | { type: "CRF"; value: number }
  | { type: "CQP"; value: number }
  | { type: "ABR"; bitrateKbps: number; maxBitrateKbps?: number };

export interface VideoSettings {
  rateControl: RateControl;
  encoderPreset: EncoderPreset;
  resolution: { width: number; height: number } | null;
  frameRate: number | null;
  pixelFormat: string | null;
  profile: string | null;
  additionalParams: string[];
}

export interface AudioSettings {
  codec: AudioCodec;
  bitrateKbps: number;
  channels: number;
  sampleRate: number;
}

export interface HwAccelConfig {
  device: HwAccelDevice;
  deviceIndex: number | null;
}

export interface CodecConfig {
  videoCodec: VideoCodec;
  audioSettings: AudioSettings;
  videoSettings: VideoSettings;
  containerFormat: ContainerFormat;
  hwAccel: HwAccelConfig | null;
}

// ============================================================
// File info
// ============================================================

export interface FileInfo {
  path: string;
  fileName: string;
  fileSize: number;
  duration: number | null;
  videoCodec: string | null;
  audioCodec: string | null;
  width: number | null;
  height: number | null;
  frameRate: number | null;
  bitrate: number | null;
  /** 源音频流码率（bps）；音频 Copy 时预估输出体积用 */
  audioBitrate: number | null;
  pixelFormat: string | null;
  /** 前端专用:正在 ffprobe 探测中(占位项) */
  probing?: boolean;
  /** 前端专用:探测失败时的降级标记 */
  probeError?: boolean;
}

// ============================================================
// Encoding progress
// ============================================================

export type EncodeStage =
  | "encoding"
  | "complete"
  | "error";

/**
 * 与后端 `src-tauri/src/encoder/progress.rs` 的 `EncodeProgress` 载荷严格对齐
 * (serde camelCase)：`totalSizeKb` / `elapsed`(字符串) / `time`(out_time 原文)。
 */
export interface EncodeProgress {
  jobId: string;
  fileName: string;
  frame: number;
  fps: number;
  speed: number;
  bitrate: number;
  totalSizeKb: number;
  /** 预估压缩后的输出体积（KB），编码开始后才有值，否则为 null */
  estimatedSizeKb: number | null;
  elapsed: string;
  percentage: number;
  stage: EncodeStage;
  time: string;
}

export interface EncodeResult {
  jobId: string;
  fileName: string;
  success: boolean;
  outputPath: string | null;
  outputSizeBytes: number | null;
  durationSeconds: number;
  /** 结构化取消标记：后端显式区分"用户取消"与"失败" */
  cancelled: boolean;
  error: string | null;
}

// ============================================================
// Queue
// ============================================================

export type JobStatus =
  | "Pending"
  | "Encoding"
  | "Paused"
  | "Completed"
  | "Failed"
  | "Cancelled";

export interface EncodeJob {
  id: string;
  inputPath: string;
  outputPath: string;
  codecConfig: CodecConfig;
  status: JobStatus;
  /** 原始文件体积（字节），入队时读取；用于完成时计算压缩率 */
  inputSize: number | null;
  /** 编码开始前的预估输出体积（字节），来自后端 add_to_queue 时的 ffprobe 推算 */
  estimatedOutputSize: number | null;
  /** 编码完成后的实际输出体积（字节），完成后由后端写入 */
  outputSize: number | null;
  /** VMAF 平均得分（0-100），点击「计算 VMAF」后写入，未计算时为 null */
  vmafScore: number | null;
  /** VMAF 明细 JSON（{mode:"full"|"sampled", scores:number[]}），未计算时为 null */
  vmafDetail: string | null;
  /**
   * 实时编码中为 EncodeProgress 对象（来自 encode://progress 事件）；
   * 后端队列快照（queue://updated / get_queue_status）携带的是 0-100 的数字百分比。
   */
  progress: EncodeProgress | number | null;
  createdAt: string;
  startedAt: string | null;
  completedAt: string | null;
  error: string | null;
}

export interface QueueStatus {
  jobs: EncodeJob[];
  total: number;
  pending: number;
  encoding: number;
  completed: number;
  failed: number;
  /** 队列级暂停：暂停后不再自动启动下一个任务（正在编码的不受影响） */
  paused: boolean;
}

/** compute_vmaf 命令的返回（4 段 × 5 秒采样 VMAF 结果） */
export interface VmafResult {
  averageScore: number;
  segmentScores: number[];
  segmentCount: number;
}

// ============================================================
// History
// ============================================================

export interface HistoryEntry {
  id: string;
  inputPath: string;
  outputPath: string;
  fileName: string;
  status: string;
  /** VMAF 平均得分（0-100），计算过才有值 */
  vmafScore: number | null;
  /** 实际输出体积（字节），完成的任务才有值 */
  outputSize: number | null;
  /** 原始文件体积（字节），入队时读取，用于计算压缩率 */
  inputSize: number | null;
  createdAt: string;
  completedAt: string | null;
  error: string | null;
}

// ============================================================
// Presets
// ============================================================

export interface Preset {
  id: string;
  name: string;
  description: string;
  config: CodecConfig;
  isBuiltin: boolean;
  createdAt: string;
  updatedAt: string;
}

// ============================================================
// Hardware acceleration
// ============================================================

export interface HwCodecInfo {
  codec: string;
  encoder: string;
}

export interface HwAccelInfo {
  device: HwAccelDevice;
  available: boolean;
  deviceName: string;
  supportedCodecs: HwCodecInfo[];
}

export interface SystemInfo {
  hwAccels: HwAccelInfo[];
  ffmpegVersion: string | null;
  ffmpegPath: string | null;
  cpuName: string;
  cpuCores: number;
  totalMemoryGb: number;
  platform: string;
}

// ============================================================
// FFmpeg status
// ============================================================

export type FfmpegStatus =
  | "checking"
  | "installed"
  | "not-installed"
  | "downloading"
  | "error";

export interface FfmpegStatusInfo {
  status: FfmpegStatus;
  version: string | null;
  path: string | null;
  downloadProgress: number | null;
  error: string | null;
}

// ============================================================
// License（软糖铺授权，对齐后端 license/manager.rs 的 LicenseStatus）
// ============================================================

/** 授权状态：Free / Pro + 到期时间 + 离线宽限期标记 */
export interface LicenseStatus {
  pro: boolean;
  /** 等级显示名（如"专业版"），未激活为 null */
  levelLabel: string | null;
  /** 购买邮箱，未激活为 null */
  email: string | null;
  /** 令牌到期时间（RFC3339），未激活为 null */
  expiresAt: string | null;
  /** 该等级功能特性列表 */
  features: string[];
  /** 是否处于离线宽限期（最近一次在线续验未成功） */
  offline: boolean;
  /** 已绑定的激活码（激活对话框回显），未激活为 null */
  code: string | null;
  /** 购买页链接（激活对话框「购买激活码」入口），未配置为 null */
  buyUrl: string | null;
}
