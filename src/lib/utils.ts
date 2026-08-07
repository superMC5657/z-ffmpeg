import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

/** Whether the app is running inside the Tauri WebView (not a plain browser). */
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

export function formatSpeed(speed: number): string {
  return `${speed.toFixed(2)}x`;
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
