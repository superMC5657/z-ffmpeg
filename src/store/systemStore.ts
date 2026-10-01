import { create } from "zustand";
import type { HwAccelDevice, HwAccelInfo, FfmpegStatusInfo } from "@/types";
import {
  getSystemInfo,
  checkFfmpegStatus,
  downloadFfmpeg as apiDownloadFfmpeg,
  onFfmpegDownloadProgress,
  onFfmpegReady,
  onFfmpegError,
} from "@/lib/tauri";
import { isTauriRuntime, formatError } from "@/lib/utils";
import { showErrorToast } from "@/store/toastStore";

/** 模块级共享的硬件检测 Promise,保证并发调用只检测一次 */
let fetchPromise: Promise<void> | null = null;
/** 模块级共享的 FFmpeg 状态检测 Promise */
let checkFfmpegPromise: Promise<void> | null = null;
/** 模块级监听器单例标记 */
let ffmpegListenersSetup = false;

interface SystemState {
  hwAccels: HwAccelInfo[];
  loading: boolean;
  /** 已尝试过检测(成功或失败),避免重复调用 ffmpeg -encoders */
  loaded: boolean;

  fetchHwAccels: (force?: boolean) => Promise<void>;
  /** 硬件加速设备是否可用;null/undefined(软件编码)始终可用 */
  isHwAccelAvailable: (device: HwAccelDevice | null | undefined) => boolean;

  // FFmpeg 状态与下载（收拢到全局 store，避免组件分叉）
  ffmpegStatus: FfmpegStatusInfo | null;
  downloading: boolean;
  downloadProgress: number | null;
  downloadError: string | null;

  checkFfmpeg: () => Promise<void>;
  downloadFfmpeg: () => Promise<void>;
  initFfmpeg: () => void;
}

export const useSystemStore = create<SystemState>((set, get) => ({
  hwAccels: [],
  // 初始为 true，首次加载显示骨架屏
  loading: true,
  loaded: false,

  ffmpegStatus: null,
  downloading: false,
  downloadProgress: null,
  downloadError: null,

  fetchHwAccels: (force = false) => {
    // force=true 绕过 loaded 缓存,用于 FFmpeg 刚安装/更新后的重新检测
    if (!force && get().loaded) return Promise.resolve();
    // 进行中(或刚完成)的检测直接复用,避免并发重复跑 ffmpeg -encoders
    if (fetchPromise) return fetchPromise;

    set({ loading: true });
    fetchPromise = (async () => {
      try {
        const info = await getSystemInfo();
        set({ hwAccels: info.hwAccels, loaded: true });
      } catch {
        // 检测失败时视为不可用(保守处理),硬件预设将被禁用
        set({ loaded: true });
      } finally {
        set({ loading: false });
        fetchPromise = null;
      }
    })();
    return fetchPromise;
  },

  isHwAccelAvailable: (device) => {
    if (!device) return true; // 软件编码始终可用
    const found = get().hwAccels.find((h) => h.device === device);
    return found ? found.available : false;
  },

  checkFfmpeg: async () => {
    if (checkFfmpegPromise) return checkFfmpegPromise;
    checkFfmpegPromise = (async () => {
      try {
        const info = await checkFfmpegStatus();
        set({ ffmpegStatus: info });
      } catch (e) {
        set({ ffmpegStatus: null });
        if (isTauriRuntime()) {
          showErrorToast("获取 FFmpeg 状态失败", e);
        }
      } finally {
        checkFfmpegPromise = null;
      }
    })();
    return checkFfmpegPromise;
  },

  downloadFfmpeg: async () => {
    if (!isTauriRuntime()) {
      set({ downloadError: "浏览器环境不支持下载 FFmpeg" });
      return;
    }
    set({ downloading: true, downloadError: null, downloadProgress: 0 });
    try {
      const info = await apiDownloadFfmpeg();
      set({ ffmpegStatus: info, downloading: false, downloadProgress: null });
    } catch (e) {
      set({ downloading: false, downloadError: formatError(e) });
    }
  },

  initFfmpeg: () => {
    if (!ffmpegListenersSetup) {
      ffmpegListenersSetup = true;
      if (isTauriRuntime()) {
        onFfmpegDownloadProgress((p) => {
          set((state) => ({
            downloading: true,
            downloadProgress: p,
            ffmpegStatus: state.ffmpegStatus
              ? { ...state.ffmpegStatus, status: "downloading" }
              : {
                  status: "downloading",
                  version: null,
                  path: null,
                  downloadProgress: p,
                  error: null,
                },
          }));
        }).catch(() => {});

        onFfmpegReady((ready) => {
          set({
            ffmpegStatus: ready,
            downloading: false,
            downloadProgress: null,
            downloadError: null,
          });
          void get().fetchHwAccels(true);
        }).catch(() => {});

        onFfmpegError((errInfo) => {
          set({
            ffmpegStatus: errInfo,
            downloading: false,
            downloadProgress: null,
            downloadError: errInfo?.error ?? "FFmpeg 异常",
          });
          void get().checkFfmpeg();
        }).catch(() => {});
      }
    }
    void get().checkFfmpeg();
  },
}));
