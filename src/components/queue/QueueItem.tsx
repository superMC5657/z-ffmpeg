import { X, RotateCw, Square, FileVideo, Gauge } from "lucide-react";
import { useState } from "react";
import type { EncodeJob } from "@/types";
import { useQueueStore } from "@/store/queueStore";
import { useToastStore } from "@/store/toastStore";
import { computeVmaf } from "@/lib/tauri";
import ProgressBar from "@/components/progress/ProgressBar";
import { cn } from "@/lib/utils";

interface QueueItemProps {
  job: EncodeJob;
}

const STATUS_PILL: Record<string, string> = {
  Pending: "bg-warning/15 text-warning",
  Encoding: "bg-accent/12 text-accent",
  Completed: "bg-success/15 text-success",
  Failed: "bg-destructive/12 text-destructive",
  Cancelled: "bg-fill text-secondary",
};

const STATUS_LABEL: Record<string, string> = {
  Pending: "等待中",
  Encoding: "编码中",
  Completed: "已完成",
  Failed: "失败",
  Cancelled: "已取消",
};

export default function QueueItem({ job }: QueueItemProps) {
  const removeJobs = useQueueStore((s) => s.removeJobs);
  const cancelJob = useQueueStore((s) => s.cancelJob);
  const retryJob = useQueueStore((s) => s.retryJob);
  const vmafSegmentsSetting = useQueueStore((s) => s.vmafSegments);
  const [vmafLoading, setVmafLoading] = useState(false);

  const handleRetry = async () => {
    try {
      await retryJob(job.id);
      useToastStore.getState().showToast("已重新加入队列", "success");
    } catch (err) {
      useToastStore.getState().showToast(
        `重试失败: ${err instanceof Error ? err.message : String(err)}`,
        "error"
      );
    }
  };

  const handleComputeVmaf = async () => {
    setVmafLoading(true);
    try {
      await computeVmaf(job.id, vmafSegmentsSetting);
      useToastStore.getState().showToast(
        vmafSegmentsSetting === 0 ? "VMAF 全量对比完成" : "VMAF 计算完成",
        "success"
      );
    } catch (err) {
      useToastStore.getState().showToast(
        `VMAF 计算失败: ${err instanceof Error ? err.message : String(err)}`,
        "error"
      );
    } finally {
      setVmafLoading(false);
    }
  };

  // VMAF 按钮的提示文案：按当前设置区分全量/采样
  const vmafButtonTitle =
    vmafSegmentsSetting === 0
      ? "全片逐帧计算 VMAF（耗时长，结果最精确）"
      : `均匀采样 ${vmafSegmentsSetting} 段 × 5 秒计算 VMAF 得分`;

  const fileName =
    (job.progress && typeof job.progress === "object" ? job.progress.fileName : undefined) ||
    job.inputPath.split(/[/\\]/).pop();

  return (
    <div className="group flex items-center gap-3.5 px-3.5 py-3 transition-colors hover:bg-fill/40">
      {/* File icon */}
      <div
        className={cn(
          "flex h-9 w-9 shrink-0 items-center justify-center rounded-[9px]",
          job.status === "Completed"
            ? "bg-success/12 text-success"
            : job.status === "Failed"
              ? "bg-destructive/10 text-destructive"
              : "bg-accent/10 text-accent"
        )}
      >
        <FileVideo className="h-4.5 w-4.5" />
      </div>

      {/* Job info */}
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <p className="truncate text-[13px] font-medium leading-5">{fileName}</p>
          <span
            className={cn(
              "shrink-0 rounded-full px-2 py-0.5 text-[10px] font-medium leading-4",
              STATUS_PILL[job.status] ?? "bg-fill text-secondary"
            )}
          >
            {STATUS_LABEL[job.status] ?? job.status}
          </span>
        </div>
        <div className="mt-1 flex items-center gap-2">
          <div className="min-w-0 flex-1">
            <ProgressBar
              progress={job.progress ?? null}
              status={job.status}
              estimatedSizeBytes={job.estimatedOutputSize ?? null}
              outputSizeBytes={job.outputSize ?? null}
              inputSizeBytes={job.inputSize ?? null}
              vmafScore={job.vmafScore ?? null}
            />
          </div>
          {/* VMAF 按钮 — 常显（不依赖 hover），与进度信息文字同排对齐 */}
          {job.status === "Completed" && (
            <button
              onClick={handleComputeVmaf}
              disabled={vmafLoading}
              title={vmafButtonTitle}
              className="flex h-7 shrink-0 items-center gap-1 rounded-md px-1.5 text-[11px] text-secondary transition-colors hover:bg-fill-strong hover:text-foreground disabled:opacity-50"
            >
              <Gauge className={cn("h-3.5 w-3.5", vmafLoading && "animate-spin")} />
              {vmafLoading ? "计算中" : job.vmafScore != null ? "重算 VMAF" : "VMAF"}
            </button>
          )}
        </div>
      </div>

      {/* Actions (hover 显示) */}
      <div className="flex items-center gap-0.5 opacity-0 transition-opacity group-hover:opacity-100">
        {job.status === "Failed" && (
          <button
            onClick={handleRetry}
            title="重新编码"
            className="flex h-7 w-7 items-center justify-center rounded-md text-secondary transition-colors hover:bg-fill-strong hover:text-foreground"
          >
            <RotateCw className="h-3.5 w-3.5" />
          </button>
        )}
        {job.status === "Encoding" && (
          <button
            onClick={() => cancelJob(job.id)}
            title="取消编码（终止 ffmpeg 进程）"
            className="flex h-7 w-7 items-center justify-center rounded-md text-secondary transition-colors hover:bg-destructive/10 hover:text-destructive"
          >
            <Square className="h-3.5 w-3.5 fill-current" />
          </button>
        )}
        {(job.status === "Pending" || job.status === "Completed" || job.status === "Failed" || job.status === "Cancelled") && (
          <button
            onClick={() => removeJobs([job.id])}
            title="移除任务"
            className="flex h-7 w-7 items-center justify-center rounded-md text-secondary transition-colors hover:bg-destructive/10 hover:text-destructive"
          >
            <X className="h-3.5 w-3.5" />
          </button>
        )}
      </div>
    </div>
  );
}
