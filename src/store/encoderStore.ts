import { create } from "zustand";
import type {
  FileInfo,
  CodecConfig,
  VideoCodec,
  AudioCodec,
  ContainerFormat,
  EncoderPreset,
  RateControl,
  HwAccelConfig,
} from "@/types";
import { probeFile, startEncode, cancelEncode, estimateOutputSizes } from "@/lib/tauri";

// 模块级防抖计时器：CRF slider 拖动等高频参数变化时合并为一次预估刷新
let estimateRefreshTimer: ReturnType<typeof setTimeout> | null = null;
// 预估请求序号：每次参数/文件列表变化时递增，丢弃在途的过期响应，避免旧参数覆盖新结果
let estimateRequestSeq = 0;

interface EncoderState {
  // File selection
  inputFiles: FileInfo[];
  addFiles: (paths: string[]) => Promise<void>;
  removeFile: (index: number) => void;
  clearFiles: () => void;

  // Codec settings
  videoCodec: VideoCodec;
  setVideoCodec: (codec: VideoCodec) => void;
  rateControl: RateControl;
  setRateControl: (rc: RateControl) => void;
  encoderPreset: EncoderPreset;
  setEncoderPreset: (preset: EncoderPreset) => void;
  resolution: { width: number; height: number } | null;
  setResolution: (res: { width: number; height: number } | null) => void;
  frameRate: number | null;
  setFrameRate: (fps: number | null) => void;
  pixelFormat: string | null;
  setPixelFormat: (fmt: string | null) => void;

  // Audio settings
  audioCodec: AudioCodec;
  setAudioCodec: (codec: AudioCodec) => void;
  audioBitrate: number;
  setAudioBitrate: (br: number) => void;

  // Output settings
  outputDir: string;
  setOutputDir: (dir: string) => void;
  containerFormat: ContainerFormat;
  setContainerFormat: (fmt: ContainerFormat) => void;

  // HW acceleration
  hwAccel: HwAccelConfig | null;
  setHwAccel: (config: HwAccelConfig | null) => void;

  // Actions
  isEncoding: boolean;
  setIsEncoding: (v: boolean) => void;
  buildConfig: () => CodecConfig;
  /** 把一份完整配置(如预设的 config)应用到当前表单状态 */
  applyConfig: (config: CodecConfig) => void;
  startEncode: (inputPath: string, outputPath: string) => Promise<string>;
  cancelEncode: (jobId: string) => Promise<void>;

  /** 预估输出体积（字节），key = 输入文件 path；参数变化时防抖刷新 */
  estimatedSizes: Record<string, number | null>;
  /** 立即按当前参数刷新所有已探测文件的预估体积（后端纯算术，无 I/O） */
  refreshEstimates: () => Promise<void>;
  /** 参数变化后防抖（150ms）调度刷新预估 */
  scheduleEstimateRefresh: () => void;
}

export const useEncoderStore = create<EncoderState>((set, get) => ({
  // File selection
  inputFiles: [],
  addFiles: async (paths: string[]) => {
    // 1) 立即插入"分析中"占位项,界面即时响应;全部探测完才渲染会造成卡顿感
    const placeholders: FileInfo[] = paths.map((path) => ({
      path,
      fileName: path.split(/[/\\]/).pop() || path,
      fileSize: 0,
      duration: null,
      videoCodec: null,
      audioCodec: null,
      width: null,
      height: null,
      frameRate: null,
      bitrate: null,
      audioBitrate: null,
      pixelFormat: null,
      probing: true,
    }));
    set((s) => ({ inputFiles: [...s.inputFiles, ...placeholders] }));

    // 2) 小并发探测(上限 4),每个完成后立即按 path 更新对应项;
    //    若期间该文件被用户删除,map 找不到 path 则自然跳过,不覆盖
    const CONCURRENCY = 4;
    let cursor = 0;
    const workers = Array.from(
      { length: Math.min(CONCURRENCY, paths.length) },
      async () => {
        while (cursor < paths.length) {
          const path = paths[cursor++];
          const fileName = path.split(/[/\\]/).pop() || path;
          let info: FileInfo;
          try {
            info = { ...(await probeFile(path)), probing: false, probeError: false };
          } catch {
            // 探测失败:保留占位信息,标记 probeError 供 UI 降级显示
            info = {
              path,
              fileName,
              fileSize: 0,
              duration: null,
              videoCodec: null,
              audioCodec: null,
              width: null,
              height: null,
              frameRate: null,
              bitrate: null,
              audioBitrate: null,
              pixelFormat: null,
              probing: false,
              probeError: true,
            };
          }
          set((s) => ({
            inputFiles: s.inputFiles.map((f) => (f.path === path ? info : f)),
          }));
        }
      }
    );
    await Promise.all(workers);
    // 探测完成后按当前参数刷新预估体积
    get().scheduleEstimateRefresh();
  },

  removeFile: (index: number) => {
    const removed = get().inputFiles[index];
    // 使在途预估请求失效，防止旧结果复活已删除文件的孤儿 key
    estimateRequestSeq++;
    set((s) => ({
      inputFiles: s.inputFiles.filter((_, i) => i !== index),
      // 同步清理该文件的预估项，避免孤儿 key 累积
      estimatedSizes: removed
        ? Object.fromEntries(Object.entries(s.estimatedSizes).filter(([p]) => p !== removed.path))
        : s.estimatedSizes,
    }));
    get().scheduleEstimateRefresh();
  },
  clearFiles: () => {
    estimateRequestSeq++;
    set({ inputFiles: [], estimatedSizes: {} });
  },

  // Codec settings
  videoCodec: "H264",
  setVideoCodec: (codec) => {
    set({ videoCodec: codec });
    get().scheduleEstimateRefresh();
  },

  rateControl: { type: "CRF", value: 23 },
  setRateControl: (rc) => {
    set({ rateControl: rc });
    get().scheduleEstimateRefresh();
  },

  encoderPreset: "medium",
  setEncoderPreset: (preset) => set({ encoderPreset: preset }),

  resolution: null,
  setResolution: (res) => {
    set({ resolution: res });
    // 分辨率影响 CRF/CQP 预估体积（像素面积缩放），需刷新
    get().scheduleEstimateRefresh();
  },

  frameRate: null,
  setFrameRate: (fps) => {
    set({ frameRate: fps });
    // 帧率影响 CRF/CQP 预估体积（帧数缩放），需刷新
    get().scheduleEstimateRefresh();
  },

  pixelFormat: null,
  setPixelFormat: (fmt) => set({ pixelFormat: fmt }),

  // Audio settings
  audioCodec: "AAC",
  setAudioCodec: (codec) => {
    set({ audioCodec: codec });
    get().scheduleEstimateRefresh();
  },
  audioBitrate: 192,
  setAudioBitrate: (br) => {
    set({ audioBitrate: br });
    get().scheduleEstimateRefresh();
  },

  // Output settings
  outputDir: "",
  setOutputDir: (dir) => set({ outputDir: dir }),
  containerFormat: "MP4",
  setContainerFormat: (fmt) => {
    set({ containerFormat: fmt });
    get().scheduleEstimateRefresh();
  },

  // HW acceleration
  hwAccel: null,
  setHwAccel: (config) => {
    set({ hwAccel: config });
    get().scheduleEstimateRefresh();
  },

  // Actions
  isEncoding: false,
  setIsEncoding: (v) => set({ isEncoding: v }),

  buildConfig: () => {
    const s = get();
    return {
      videoCodec: s.videoCodec,
      audioSettings: {
        codec: s.audioCodec,
        bitrateKbps: s.audioBitrate,
        channels: 2,
        sampleRate: 48000,
      },
      videoSettings: {
        rateControl: s.rateControl,
        encoderPreset: s.encoderPreset,
        resolution: s.resolution,
        frameRate: s.frameRate,
        pixelFormat: s.pixelFormat,
        profile: null,
        additionalParams: [],
      },
      containerFormat: s.containerFormat,
      hwAccel: s.hwAccel,
    };
  },

  applyConfig: (config) => {
    // 预设可能来自导入的 JSON,不保证 schema 完整——逐字段防护,
    // 缺失的字段不覆盖当前表单值(否则 buildConfig/addToQueue 会在 Rust 端反序列化失败)。
    const vs = config.videoSettings;
    const as_ = config.audioSettings;
    const patch: Partial<CodecConfig> & {
      videoCodec?: VideoCodec;
      rateControl?: RateControl;
      encoderPreset?: EncoderPreset;
      resolution?: { width: number; height: number } | null;
      frameRate?: number | null;
      pixelFormat?: string | null;
      audioCodec?: AudioCodec;
      audioBitrate?: number;
      containerFormat?: ContainerFormat;
      hwAccel?: HwAccelConfig | null;
    } = {};

    if (config.videoCodec) patch.videoCodec = config.videoCodec;
    if (vs) {
      if (vs.rateControl) patch.rateControl = vs.rateControl;
      if (vs.encoderPreset) patch.encoderPreset = vs.encoderPreset;
      if (vs.resolution != null) patch.resolution = vs.resolution;
      if (vs.frameRate != null) patch.frameRate = vs.frameRate;
      if (vs.pixelFormat != null) patch.pixelFormat = vs.pixelFormat;
    }
    if (as_) {
      if (as_.codec) patch.audioCodec = as_.codec;
      if (as_.bitrateKbps != null) patch.audioBitrate = as_.bitrateKbps;
    }
    if (config.containerFormat) patch.containerFormat = config.containerFormat;
    if (config.hwAccel != null) patch.hwAccel = config.hwAccel;

    set(patch);
    get().scheduleEstimateRefresh();
  },

  startEncode: async (inputPath: string, outputPath: string) => {
    const config = get().buildConfig();
    const jobId = crypto.randomUUID();
    set({ isEncoding: true });
    await startEncode(config, inputPath, outputPath, jobId);
    return jobId;
  },

  cancelEncode: async (jobId: string) => {
    await cancelEncode(jobId);
    set({ isEncoding: false });
  },

  // ---- 预估体积（编码页实时预览） ----
  estimatedSizes: {},
  refreshEstimates: async () => {
    const s = get();
    // 探测中/失败的项无有效数据，后端会返回 null，直接跳过减少无谓 IPC
    const files = s.inputFiles.filter((f) => !f.probing && !f.probeError);
    if (files.length === 0) return;
    const seq = ++estimateRequestSeq;
    try {
      const sizes = await estimateOutputSizes(s.buildConfig(), files);
      // 期间参数/文件列表又变了：丢弃这次结果，避免旧参数覆盖新预估
      if (seq !== estimateRequestSeq) return;
      const map: Record<string, number | null> = { ...get().estimatedSizes };
      files.forEach((f, i) => {
        map[f.path] = sizes[i] ?? null;
      });
      set({ estimatedSizes: map });
    } catch {
      // 预估失败不影响主流程，静默忽略（下次参数变化会重试）
    }
  },
  scheduleEstimateRefresh: () => {
    // 参数/文件列表已变化：使在途预估请求失效，防止旧结果覆盖新预估
    estimateRequestSeq++;
    if (estimateRefreshTimer) clearTimeout(estimateRefreshTimer);
    estimateRefreshTimer = setTimeout(() => {
      get().refreshEstimates();
    }, 150);
  },
}));
