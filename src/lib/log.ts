import { error as logError, info as logInfo, warn as logWarn } from "@tauri-apps/plugin-log";
import { isTauriRuntime } from "@/lib/utils";

/**
 * 前端日志：通过 tauri-plugin-log 写入后端统一的日志文件
 * （{data_dir}/zffmpeg/logs/）。浏览器预览模式下静默降级，
 * 只打 console，避免报错。
 */
export const appLog = {
  info: (message: string) => {
    if (isTauriRuntime()) logInfo(message).catch(() => {});
    else console.info(message);
  },
  warn: (message: string) => {
    if (isTauriRuntime()) logWarn(message).catch(() => {});
    else console.warn(message);
  },
  error: (message: string) => {
    if (isTauriRuntime()) logError(message).catch(() => {});
    else console.error(message);
  },
};
