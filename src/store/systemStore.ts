import { create } from "zustand";
import type { HwAccelDevice, HwAccelInfo } from "@/types";
import { detectHwAccel } from "@/lib/tauri";

/** 模块级共享的进行中检测 Promise,保证并发调用只检测一次 */
let fetchPromise: Promise<void> | null = null;

interface SystemState {
  hwAccels: HwAccelInfo[];
  loading: boolean;
  /** 已尝试过检测(成功或失败),避免重复调用 ffmpeg -encoders */
  loaded: boolean;

  fetchHwAccels: (force?: boolean) => Promise<void>;
  /** 硬件加速设备是否可用;null/undefined(软件编码)始终可用 */
  isHwAccelAvailable: (device: HwAccelDevice | null | undefined) => boolean;
}

export const useSystemStore = create<SystemState>((set, get) => ({
  hwAccels: [],
  // 初始为 true,与旧的局部 state 行为一致:首次渲染即显示骨架屏
  loading: true,
  loaded: false,

  fetchHwAccels: (force = false) => {
    // force=true 绕过 loaded 缓存,用于 FFmpeg 刚安装/更新后的重新检测
    if (!force && get().loaded) return Promise.resolve();
    // 进行中(或刚完成)的检测直接复用,避免并发重复跑 ffmpeg -encoders
    if (fetchPromise) return fetchPromise;

    set({ loading: true });
    fetchPromise = (async () => {
      try {
        const info = await detectHwAccel();
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
}));
