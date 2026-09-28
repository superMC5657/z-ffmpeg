import { create } from "zustand";
import { formatError } from "@/lib/utils";

export type ToastType = "success" | "error" | "info";

export interface ToastItem {
  id: number;
  message: string;
  type: ToastType;
}

interface ToastState {
  toasts: ToastItem[];
  showToast: (message: string, type?: ToastType) => void;
  dismissToast: (id: number) => void;
}

let nextId = 1;

export const useToastStore = create<ToastState>((set) => ({
  toasts: [],
  showToast: (message, type = "info") => {
    const id = nextId++;
    set((s) => ({ toasts: [...s.toasts, { id, message, type }] }));
    setTimeout(() => {
      set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) }));
    }, 3200);
  },
  dismissToast: (id) =>
    set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),
}));

/**
 * 统一错误提示：`showErrorToast("删除失败", err)` 会弹出「删除失败: <err 消息>」。
 * 约 26 处调用方原先各自手写 `${err instanceof Error ? err.message : String(err)}`，收敛到此。
 * 传入空 action 时仅展示错误消息。
 */
export function showErrorToast(action: string, error: unknown): void {
  const message = formatError(error);
  useToastStore
    .getState()
    .showToast(action ? `${action}: ${message}` : message, "error");
}
