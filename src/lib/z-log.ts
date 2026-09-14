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

/** 直接转发的日质量出口：调用即经 IPC 落到 Rust 日志文件。 */
export const zlog = {
  debug: (msg: string): Promise<void> => debug(msg),
  info: (msg: string): Promise<void> => info(msg),
  warn: (msg: string): Promise<void> => warn(msg),
  error: (msg: string): Promise<void> => error(msg),
};

/** 取后端日志目录（“打开日志目录”按钮用）。 */
export function getLogDir(): Promise<string> {
  return invoke<string>("zlog_get_dir");
}

/** 导出诊断包（zip 路径在系统 temp 下），不上报、仅本地导出。 */
export function exportLogBundle(): Promise<string> {
  return invoke<string>("zlog_export_bundle");
}
