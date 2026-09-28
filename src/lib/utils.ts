import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

/** 判断应用是否在 Tauri WebView 运行时中运行（而非普通浏览器）。 */
export function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export function formatFileSize(bytes: number): string {
  if (bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  return `${(bytes / Math.pow(1024, i)).toFixed(1)} ${units[i]}`;
}

/** 紧凑体积格式：无空格、整数去小数（20MB / 41.8MB / 4.2KB） */
export function formatFileSizeCompact(bytes: number): string {
  if (bytes === 0) return "0B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  const v = bytes / Math.pow(1024, i);
  const s = v.toFixed(1).replace(/\.0$/, "");
  return `${s}${units[i]}`;
}

export function formatDuration(seconds: number): string {
  if (!seconds || !isFinite(seconds)) return "--:--:--";
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = Math.floor(seconds % 60);
  if (h > 0) {
    return `${h}:${m.toString().padStart(2, "0")}:${s.toString().padStart(2, "0")}`;
  }
  return `${m}:${s.toString().padStart(2, "0")}`;
}

export function formatBitrate(kbps: number): string {
  if (kbps >= 1000000) return `${(kbps / 1000000).toFixed(1)} Gbps`;
  if (kbps >= 1000) return `${(kbps / 1000).toFixed(1)} Mbps`;
  return `${kbps.toFixed(0)} kbps`;
}

export function formatFps(fps: number): string {
  return `${fps.toFixed(1)} fps`;
}

export function formatSpeed(speed: number, precision = 2): string {
  return `${speed.toFixed(precision)}x`;
}

/** 压缩率结果：text 为可直接展示的完整文本，ratio 为百分比（正 = 体积变小） */
export interface CompressionRatio {
  /** 形如 "↓30.1% 20MB"；无有效输入体积时为输出体积文本，输出体积为空时为空串 */
  text: string;
  /** 压缩率百分比（正 = 体积变小，负 = 体积变大）；输入体积无效时为 null */
  ratio: number | null;
  /** 输出体积大于输入（UI 展示警示色用） */
  enlarged: boolean;
}

/**
 * 统一压缩率计算：ratio = (1 - out / in) × 100。
 * 输入体积有效时输出体积显示为压缩率 + 实际体积；否则退化为仅输出体积文本。
 */
export function formatCompressionRatio(
  inBytes: number | null | undefined,
  outBytes: number | null | undefined
): CompressionRatio {
  if (outBytes == null) return { text: "", ratio: null, enlarged: false };
  if (inBytes == null || inBytes <= 0) {
    return { text: formatFileSizeCompact(outBytes), ratio: null, enlarged: false };
  }
  const ratio = (1 - outBytes / inBytes) * 100;
  const arrow = ratio >= 0 ? "↓" : "↑";
  return {
    text: `${arrow}${Math.abs(ratio).toFixed(1)}% ${formatFileSizeCompact(outBytes)}`,
    ratio,
    enlarged: ratio < 0,
  };
}

/** 从完整路径中提取文件名（兼容 Windows 反斜杠与 POSIX 斜杠） */
export function getFileName(path: string): string {
  return path.split(/[/\\]/).pop() || path;
}

export function formatPercentage(value: number): string {
  return `${value.toFixed(1)}%`;
}

/** 解析 "HH:MM:SS" / "MM:SS" 为秒；解析失败返回 0 */
export function parseElapsedSeconds(elapsed: string): number {
  const parts = elapsed.split(":").map((s) => parseInt(s, 10));
  if (parts.some((n) => Number.isNaN(n))) return 0;
  if (parts.length === 3) return parts[0] * 3600 + parts[1] * 60 + parts[2];
  if (parts.length === 2) return parts[0] * 60 + parts[1];
  return parts[0] || 0;
}

/**
 * 按当前进度线性外推剩余时长（秒）：elapsed / pct × (100 - pct)。
 * 进度过小（<0.5%）或已用时长不可解析时返回 null（此时 ETA 不可信）。
 */
export function estimateRemainingSeconds(elapsed: string, percentage: number): number | null {
  const elapsedSec = parseElapsedSeconds(elapsed);
  if (elapsedSec <= 0 || percentage <= 0.5) return null;
  return (elapsedSec / percentage) * (100 - percentage);
}

/** 统一错误信息提取：Error 取 message，其余类型转字符串 */
export function formatError(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
