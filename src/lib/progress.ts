import type { EncodeProgress } from "@/types";

// ============================================================
// 进度契约统一模块：encode://progress 事件载荷的唯一归一出口
//
// 消费方（ProgressBar / GlobalEncodingDock / QueueItem / queueStore）
// 只允许经由本模块读取进度，禁止各自手写字段访问分支。
// ============================================================

/** 进度载荷输入：实时进度对象或空 */
export type ProgressInput = EncodeProgress | null | undefined;

/**
 * 归一后的统一进度形状：调用方只读这些字段。
 * timeRemaining 为展示用时间原文（time || elapsed），
 * 不是 ETA 外推值；ETA 仍由调用方按 elapsed + percent 自行计算。
 */
export interface NormalizedProgress {
  /** 0-100 的百分比（非法缺失统一归 0） */
  percent: number;
  /** 是否为合法实时进度对象（true）还是空/非法载荷（false） */
  isLive: boolean;
  fps?: number;
  speed?: number;
  /** 码率（kbps） */
  bitrate?: number;
  /** 已用时长原文（HH:MM:SS） */
  elapsed?: string;
  /** out_time 原文 */
  time?: string;
  /** 展示用时间（time || elapsed） */
  timeRemaining?: string | null;
  /** 实时进度携带的文件名 */
  fileName?: string;
}

/** 百分比限幅到 [0, 100]，非有限数值归 0 */
function clampPercent(v: number): number {
  if (!Number.isFinite(v)) return 0;
  return Math.min(100, Math.max(0, v));
}

/**
 * 严格守卫：必须是对象且携带数值型 percentage / percent / progress 之一，
 * 防止脏数据污染进度条。
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
 * 统一取百分比：读 percentage（兼容 percent / progress 别名），
 * null / 非法载荷归 0。
 */
export function progressToPercent(p: ProgressInput): number {
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

/** 归一：把进度载荷收敛为统一内部形状，供全部消费方使用 */
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
  return { percent: 0, isLive: false };
}
