import { create } from "zustand";
import type { LicenseStatus } from "@/types";
import { getLicenseStatus, activateLicense, deactivateLicense } from "@/lib/tauri";

/** 并发数选择上限（与后端 set_max_concurrent 的 1..=16 范围对齐，免费版/Pro 一致） */
export const FREE_MAX_CONCURRENT = 16;

interface LicenseState {
  status: LicenseStatus | null;
  /** 首次加载中 */
  loading: boolean;
  /** 激活/注销请求进行中 */
  working: boolean;
  /** 激活对话框是否打开（全局单例，门控点击处拉起） */
  activationOpen: boolean;

  fetchStatus: () => Promise<void>;
  activate: (code: string, email: string) => Promise<LicenseStatus>;
  deactivate: () => Promise<LicenseStatus>;
  setActivationOpen: (open: boolean) => void;
}

export const useLicenseStore = create<LicenseState>((set) => ({
  status: null,
  loading: true,
  working: false,
  activationOpen: false,

  fetchStatus: async () => {
    try {
      const status = await getLicenseStatus();
      set({ status, loading: false });
    } catch {
      // 读取失败保守按免费版处理（后端不可用时门控依旧生效）
      set({ loading: false });
    }
  },

  activate: async (code, email) => {
    set({ working: true });
    try {
      const status = await activateLicense(code, email);
      set({ status, activationOpen: false });
      return status;
    } finally {
      set({ working: false });
    }
  },

  deactivate: async () => {
    set({ working: true });
    try {
      const status = await deactivateLicense();
      set({ status });
      return status;
    } finally {
      set({ working: false });
    }
  },

  setActivationOpen: (open) => set({ activationOpen: open }),
}));
