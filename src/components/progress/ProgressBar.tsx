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

export default function ProgressBar({ progress, status, estimatedSizeBytes = null, outputSizeBytes = null, inputSizeBytes = null, vmafScore = null }: ProgressBarProps) {
  const isLive = typeof progress === "object" && progress !== null;
  const pct = isLive ? (progress as EncodeProgress).percentage : (progress as number) ?? 0;
  // 编码中：按已用时长与进度线性外推剩余时间（进度过小/不可解析时为 null）
  const etaSeconds = isLive
    ? estimateRemainingSeconds((progress as EncodeProgress).elapsed, (progress as EncodeProgress).percentage)
    : null;

  if (status === "Pending" || status === "Queued") {
    return (
      <div className="flex items-center gap-2 text-[13px] text-muted-foreground">
        <div className="h-1.5 flex-1 rounded-full bg-accent">
          <div className="h-full w-0 rounded-full bg-primary" />
        </div>
        {estimatedSizeBytes != null
          ? <>等待中 · 预计 {formatFileSize(estimatedSizeBytes)}</>
          : "等待中..."}
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
      <div className={`flex items-center gap-2 text-[13px] ${enlarged ? "text-warning" : "text-success"}`}>
        <div className="h-1.5 flex-1 rounded-full bg-accent">
          <div className="h-full w-full rounded-full bg-success" />
        </div>
        {parts.join(" · ")}
      </div>
    );
  }

  if (status === "Failed" || status === "Cancelled") {
    return (
      <div className="flex items-center gap-2 text-[13px] text-destructive">
        <div className="h-1.5 flex-1 rounded-full bg-accent">
          <div className="h-full rounded-full bg-destructive"
            style={{ width: `${pct}%` }} />
        </div>
        {status === "Cancelled" ? "已取消" : "失败"}
      </div>
    );
  }

  return (
    <div className="space-y-1">
      <div className="flex items-center justify-between text-[13px]">
        <span className="text-muted-foreground">
          <span className="rounded-md bg-primary/10 px-2 py-0.5 font-mono text-[13px] font-semibold text-primary">
            {pct.toFixed(1)}%
          </span>
        </span>
        {isLive && (
          <span className="text-muted-foreground">
            {formatFps((progress as EncodeProgress).fps)} · {formatSpeed((progress as EncodeProgress).speed)} · {formatBitrate((progress as EncodeProgress).bitrate)}
            {etaSeconds != null && (
              <>
                {" "}· 预计剩余 {formatDuration(etaSeconds)}
              </>
            )}
          </span>
        )}
      </div>
      <div className="h-2 w-full rounded-full bg-accent">
        <div
          className="h-full rounded-full bg-gradient-brand transition-all duration-300"
          style={{ width: `${pct}%` }}
        />
      </div>
    </div>
  );
}
