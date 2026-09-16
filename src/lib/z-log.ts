import {
  attachConsole,
  debug,
  error,
  info,
  warn,
} from "@tauri-apps/plugin-log";
import { invoke } from "@tauri-apps/api/core";

// 前端日志：直接转发模式（ffmpeg 前端日志量小，无需批处理队列）。
// Rust 侧落盘，DEV 下 attachConsole 把日志同时镜像到 devtools console。

let initialized = false;

/** 应用入口调用一次：DEV 镜像 console + 全局错误上报。 */
export async function initZLog(): Promise<void> {
  if (initialized) return;
  initialized = true;

  if (import.meta.env.DEV) {
    try {
      await attachConsole();
    } catch {
      // attach 失败不阻塞启动（例如单测/jsdom 环境无 Tauri 后端）
    }
  }

  window.onerror = (message, source, lineno, colno, err) => {
    const detail =
      err instanceof Error ? err.stack ?? err.message : String(message);
    void error(
      `window.onerror: ${detail} @${source ?? "?"}:${lineno ?? 0}:${
        colno ?? 0
      }`
    );
    return false;
  };

  window.onunhandledrejection = (event) => {
    const reason = event.reason;
    const detail =
      reason instanceof Error
        ? reason.stack ?? reason.message
        : String(reason);
    void error(`unhandledrejection: ${detail}`);
  };
}

/** 安全日志输出：IPC 失败时不抛异常，在单测/非 Tauri 环境静默降级或打印到 console */
async function safeLog(
  level: "debug" | "info" | "warn" | "error",
  msg: string
): Promise<void> {
  const fn =
    level === "debug"
      ? debug
      : level === "info"
        ? info
        : level === "warn"
          ? warn
          : error;
  try {
    await fn(msg);
  } catch {
    if (import.meta.env.DEV) {
      console[level === "error" ? "error" : level === "warn" ? "warn" : "log"](msg);
    }
  }
}

const settingDebounceTimers = new Map<string, ReturnType<typeof setTimeout>>();

/**
 * 记录用户修改参数（使用 debug 级别避免日常使用过度刷屏）。
 * 内置防抖：同一 setting 在 300ms 内连续变更时只落盘最后一次稳定值，
 * 避免用户拖动滑块时产生大量无意义的 IPC 序列化与日志洪泛。
 */
function logUiSetting(setting: string, value: unknown, debounceMs = 300): Promise<void> {
  // 测试环境下不延迟，避免影响异步测试时序
  if (import.meta.env.MODE === "test" || debounceMs <= 0) {
    const valStr = typeof value === "object" ? JSON.stringify(value) : String(value);
    return safeLog("debug", `[UI] 修改参数: ${setting}=${valStr}`);
  }

  const existingTimer = settingDebounceTimers.get(setting);
  if (existingTimer) {
    clearTimeout(existingTimer);
  }

  return new Promise((resolve) => {
    const timer = setTimeout(() => {
      settingDebounceTimers.delete(setting);
      const valStr = typeof value === "object" ? JSON.stringify(value) : String(value);
      resolve(safeLog("debug", `[UI] 修改参数: ${setting}=${valStr}`));
    }, debounceMs);
    settingDebounceTimers.set(setting, timer);
  });
}

/** 前端结构化日志出口：调用即经 IPC 落到 Rust 日志文件。 */
export const zlog = {
  debug: (msg: string): Promise<void> => safeLog("debug", msg),
  info: (msg: string): Promise<void> => safeLog("info", msg),
  warn: (msg: string): Promise<void> => safeLog("warn", msg),
  error: (msg: string): Promise<void> => safeLog("error", msg),

  /** 记录路由/页面导航 */
  route: (pathname: string): Promise<void> => {
    return safeLog("info", `[UI] 导航至页面: ${pathname}`);
  },

  /** 记录用户关键业务操作 */
  uiAction: (
    action: string,
    detail?: Record<string, unknown> | string
  ): Promise<void> => {
    const detailStr = detail
      ? typeof detail === "string"
        ? detail
        : Object.entries(detail)
            .map(([k, v]) => `${k}=${typeof v === "object" ? JSON.stringify(v) : String(v)}`)
            .join(" ")
      : "";
    const msg = detailStr ? `[UI] ${action} ${detailStr}` : `[UI] ${action}`;
    return safeLog("info", msg);
  },

  /** 记录用户修改参数（使用 debug 级别避免日常使用过度刷屏） */
  uiSetting: logUiSetting,
};

/** 取后端日志目录（“打开日志目录”按钮用）。 */
export function getLogDir(): Promise<string> {
  return invoke<string>("zlog_get_dir");
}

/** 导出诊断包（zip 路径在系统 temp 下），不上报、仅本地导出。 */
export function exportLogBundle(): Promise<string> {
  return invoke<string>("zlog_export_bundle");
}
