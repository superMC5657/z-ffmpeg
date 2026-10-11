import type { EncodeProgress } from "@/types";

// ============================================================
// 进度契约统一模块（C5）：前后端 progress 形状的唯一归一出口
//
// 后端有两种进度形状：
// - 实时进度对象：encode://progress 事件的 EncodeProgress（percentage 0-100）
// - 快照数字：queue://updated / get_queue_status 的 0-100 数字百分比
// 消费方（ProgressBar / GlobalEncodingDock / QueueItem / queueStore）
// 只允许经由本模块读取进度，禁止各自手写 typeof === "object" 分支。
// ============================================================

/** 进度载荷输入：实时对象、快照数字百分比或空 */
export type ProgressInput = EncodeProgress | number | null | undefined;

/**
 * 归一后的统一进度形状：调用方只读这些字段。
 * timeRemaining 为展示用时间原文（time || elapsed，实时进度才有），
 * 不是 ETA 外推值；ETA 仍由调用方按 elapsed + percent 自行计算。
 */
export interface NormalizedProgress {
  /** 0-100 的百分比（快照数字/非法缺失统一归一到该字段） */
  percent: number;
  /** 是否为实时进度对象（true）还是快照数字/空（false） */
  isLive: boolean;
  fps?: number;
  speed?: number;
  /** 码率（kbps，仅实时进度有） */
  bitrate?: number;
  /** 已用时长原文（HH:MM:SS，仅实时进度有） */
  elapsed?: string;
  /** out_time 原文（仅实时进度有） */
  time?: string;
  /** 展示用时间（time || elapsed，实时进度才有） */
  timeRemaining?: string | null;
  /** 实时进度携带的文件名（快照数字无该字段） */
  fileName?: string;
}

/** 百分比限幅到 [0, 100]，非有限数值归 0 */
function clampPercent(v: number): number {
  if (!Number.isFinite(v)) return 0;
  return Math.min(100, Math.max(0, v));
}

/**
 * 严格守卫：必须是对象且携带数值型关键字段（percentage / percent / progress 之一）。
 * 任意 object 即过的旧写法（typeof p === "object"）不再允许，避免脏数据污染进度条。
 */
export function isEncodeProgressStrict(p: unknown): p is EncodeProgress {
  if (typeof p !== "object" || p === null) return false;
  const o = p as Record<string, unknown>;
  return (
    typeof o.percentage === "number" ||
    typeof o.percent === "number" ||
    typeof o.progress === "number"
  );
}

/**
 * 统一取百分比：实时对象读 percentage（兼容 percent / progress 别名），
 * 快照数字直接采用，null / 非法载荷归 0。
 */
export function progressToPercent(p: ProgressInput): number {
  if (typeof p === "number") return clampPercent(p);
  if (isEncodeProgressStrict(p)) {
    const o = p as unknown as {
      percentage?: unknown;
      percent?: unknown;
      progress?: unknown;
    };
    const v = o.percentage ?? o.percent ?? o.progress;
    return typeof v === "number" ? clampPercent(v) : 0;
  }
  return 0;
}

/** 归一：把两种后端形状收敛为一种内部形状，供全部消费方使用 */
export function normalizeProgress(p: ProgressInput): NormalizedProgress {
  if (isEncodeProgressStrict(p)) {
    return {
      percent: progressToPercent(p),
      isLive: true,
      fps: p.fps,
      speed: p.speed,
      bitrate: p.bitrate,
      elapsed: p.elapsed,
      time: p.time,
      // 展示用时间：优先 out_time 原文，缺失时退化为已用时长
      timeRemaining: p.time || p.elapsed || null,
      fileName: p.fileName,
    };
  }
  return { percent: progressToPercent(p), isLive: false };
}
