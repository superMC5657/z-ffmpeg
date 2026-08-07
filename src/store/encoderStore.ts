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
import { probeFile, startEncode, cancelEncode } from "@/lib/tauri";

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
  },

  removeFile: (index: number) =>
    set((s) => ({ inputFiles: s.inputFiles.filter((_, i) => i !== index) })),
  clearFiles: () => set({ inputFiles: [] }),

  // Codec settings
  videoCodec: "H264",
  setVideoCodec: (codec) => set({ videoCodec: codec }),

  rateControl: { type: "CRF", value: 23 },
  setRateControl: (rc) => set({ rateControl: rc }),

  encoderPreset: "medium",
  setEncoderPreset: (preset) => set({ encoderPreset: preset }),

  resolution: null,
  setResolution: (res) => set({ resolution: res }),

  frameRate: null,
  setFrameRate: (fps) => set({ frameRate: fps }),

  pixelFormat: null,
  setPixelFormat: (fmt) => set({ pixelFormat: fmt }),

  // Audio settings
  audioCodec: "AAC",
  setAudioCodec: (codec) => set({ audioCodec: codec }),
  audioBitrate: 192,
  setAudioBitrate: (br) => set({ audioBitrate: br }),

  // Output settings
  outputDir: "",
  setOutputDir: (dir) => set({ outputDir: dir }),
  containerFormat: "MP4",
  setContainerFormat: (fmt) => set({ containerFormat: fmt }),

  // HW acceleration
  hwAccel: null,
  setHwAccel: (config) => set({ hwAccel: config }),

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
}));
