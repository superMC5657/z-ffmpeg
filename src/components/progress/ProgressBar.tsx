import type { EncodeProgress } from "@/types";
import {
  formatFps,
  formatSpeed,
  formatBitrate,
  formatFileSize,
  formatFileSizeCompact,
  formatDuration,
  estimateRemainingSeconds,
} from "@/lib/utils";
import { cn } from "@/lib/utils";

interface ProgressBarProps {
  /** 实时进度对象或后端快照中的数字百分比（0-100） */
  progress: EncodeProgress | number | null;
  status: string;
  /** 编码开始前（Pending）的预估输出体积（字节） */
  estimatedSizeBytes?: number | null;
  /** 编码完成后（Completed）的实际输出体积（字节） */
  outputSizeBytes?: number | null;
  /** 原始文件体积（字节），用于完成时计算压缩率 */
  inputSizeBytes?: number | null;
  /** VMAF 平均得分（0-100），计算完成后显示 */
  vmafScore?: number | null;
}

const TRACK = "h-1.5 flex-1 overflow-hidden rounded-full bg-fill-strong";

export default function ProgressBar({ progress, status, estimatedSizeBytes = null, outputSizeBytes = null, inputSizeBytes = null, vmafScore = null }: ProgressBarProps) {
  const isLive = typeof progress === "object" && progress !== null;
  const pct = isLive ? (progress as EncodeProgress).percentage : (progress as number) ?? 0;
  // 编码中：按已用时长与进度线性外推剩余时间（进度过小/不可解析时为 null）
  const etaSeconds = isLive
    ? estimateRemainingSeconds((progress as EncodeProgress).elapsed, (progress as EncodeProgress).percentage)
    : null;

  if (status === "Pending" || status === "Queued") {
    return (
      <div className="flex items-center gap-2.5 text-[11px] text-secondary">
        <div className={TRACK}>
          <div className="h-full w-0 rounded-full bg-accent" />
        </div>
        <span className="shrink-0">
          {estimatedSizeBytes != null
            ? <>等待中 · 预计 {formatFileSize(estimatedSizeBytes)}</>
            : "等待中"}
        </span>
      </div>
    );
  }

  if (status === "Completed") {
    const parts = ["完成"];
    // 压缩率 + 实际体积合并为一行：↓30.1% 20MB（增大时 ↑20.0%）
    if (outputSizeBytes != null && inputSizeBytes != null && inputSizeBytes > 0) {
      const ratio = (1 - outputSizeBytes / inputSizeBytes) * 100;
      const arrow = ratio >= 0 ? "↓" : "↑";
      parts.push(`${arrow}${Math.abs(ratio).toFixed(1)}% ${formatFileSizeCompact(outputSizeBytes)}`);
    } else if (outputSizeBytes != null) {
      parts.push(formatFileSizeCompact(outputSizeBytes));
    }
    if (vmafScore != null) parts.push(`VMAF ${vmafScore.toFixed(1)}`);
    // 输出变大时整体用警示色
    const enlarged =
      outputSizeBytes != null && inputSizeBytes != null && inputSizeBytes > 0 && outputSizeBytes > inputSizeBytes;
    return (
      <div className={cn("flex items-center gap-2.5 text-[11px]", enlarged ? "text-warning" : "text-success")}>
        <div className={TRACK}>
          <div className="h-full w-full rounded-full bg-success" />
        </div>
        <span className="shrink-0 font-medium tabular-nums">{parts.join(" · ")}</span>
      </div>
    );
  }

  if (status === "Failed" || status === "Cancelled") {
    return (
      <div className="flex items-center gap-2.5 text-[11px] text-destructive">
        <div className={TRACK}>
          <div
            className="h-full rounded-full bg-destructive"
            style={{ width: `${pct}%` }}
          />
        </div>
        <span className="shrink-0">{status === "Cancelled" ? "已取消" : "失败"}</span>
      </div>
    );
  }

  // Encoding（实时）
  return (
    <div className="space-y-1">
      <div className="flex items-center justify-between gap-2 text-[11px] text-secondary tabular-nums">
        <span className="font-semibold text-foreground">
          {pct.toFixed(1)}%
        </span>
        {isLive && (
          <span className="truncate">
            {formatFps((progress as EncodeProgress).fps)} · {formatSpeed((progress as EncodeProgress).speed)} · {formatBitrate((progress as EncodeProgress).bitrate)}
            {etaSeconds != null && <> · 剩余 {formatDuration(etaSeconds)}</>}
          </span>
        )}
      </div>
      <div className={TRACK}>
        <div
          className="h-full rounded-full bg-accent transition-[width] duration-300"
          style={{ width: `${pct}%` }}
        />
      </div>
    </div>
  );
}
