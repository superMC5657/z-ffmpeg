import type { EncodeProgress } from "@/types";
import { normalizeProgress } from "@/lib/progress";
import {
  formatFps,
  formatSpeed,
  formatBitrate,
  formatFileSize,
  formatCompressionRatio,
  formatDuration,
  estimateRemainingSeconds,
} from "@/lib/utils";
import { cn } from "@/lib/utils";
import LinearProgress from "@/components/layout/LinearProgress";

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

export default function ProgressBar({
  progress,
  status,
  estimatedSizeBytes = null,
  outputSizeBytes = null,
  inputSizeBytes = null,
  vmafScore = null,
}: ProgressBarProps) {
  // 经统一契约归一读取：实时对象与快照数字收敛为同一形状
  const normalized = normalizeProgress(progress);
  const isLive = normalized.isLive;
  const pct = normalized.percent;
  // 编码中：按已用时长与进度线性外推剩余时间（进度过小/不可解析时为 null）
  const etaSeconds = isLive
    ? estimateRemainingSeconds(normalized.elapsed ?? "", pct)
    : null;

  if (status === "Pending" || status === "Queued") {
    return (
      <div className="flex items-center gap-2.5 text-[11px] text-secondary">
        <LinearProgress value={0} variant="accent" className="flex-1" />
        <span className="shrink-0">
          {estimatedSizeBytes != null ? (
            <>等待中 · 约 {formatFileSize(estimatedSizeBytes)}</>
          ) : (
            "等待中"
          )}
        </span>
      </div>
    );
  }

  if (status === "Completed") {
    const parts = ["完成"];
    // 压缩率 + 实际体积合并为一行：↓30.1% 20MB（增大时 ↑20.0%）
    const compression = formatCompressionRatio(inputSizeBytes, outputSizeBytes);
    if (compression.text) parts.push(compression.text);
    if (vmafScore != null) parts.push(`VMAF ${vmafScore.toFixed(1)}`);
    // 输出变大时整体用警示色
    return (
      <div
        className={cn(
          "flex items-center gap-2.5 text-[11px]",
          compression.enlarged ? "text-warning" : "text-success"
        )}
      >
        <LinearProgress value={100} variant="success" className="flex-1" />
        <span className="shrink-0 font-medium tabular-nums">
          {parts.join(" · ")}
        </span>
      </div>
    );
  }

  if (status === "Failed" || status === "Cancelled") {
    return (
      <div className="flex items-center gap-2.5 text-[11px] text-destructive">
        <LinearProgress value={pct} variant="destructive" className="flex-1" />
        <span className="shrink-0">
          {status === "Cancelled" ? "已取消" : "失败"}
        </span>
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
            {formatFps(normalized.fps ?? 0)} · {formatSpeed(normalized.speed ?? 0)} ·{" "}
            {formatBitrate(normalized.bitrate ?? 0)}
            {etaSeconds != null && <> · 剩余 {formatDuration(etaSeconds)}</>}
          </span>
        )}
      </div>
      <LinearProgress value={pct} variant="accent" />
    </div>
  );
}
