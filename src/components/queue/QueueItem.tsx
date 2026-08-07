import { X, RotateCw, Square, FileVideo, Gauge } from "lucide-react";
import { useState } from "react";
import type { EncodeJob } from "@/types";
import { useQueueStore } from "@/store/queueStore";
import { useToastStore } from "@/store/toastStore";
import { computeVmaf } from "@/lib/tauri";
import ProgressBar from "@/components/progress/ProgressBar";

interface QueueItemProps {
  job: EncodeJob;
}

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

  const statusConfig: Record<string, { label: string; color: string }> = {
    Pending: { label: "等待中", color: "bg-yellow-500/20 text-yellow-400" },
    Encoding: { label: "编码中", color: "bg-blue-500/20 text-blue-400" },
    Completed: { label: "完成", color: "bg-green-500/20 text-green-400" },
    Failed: { label: "失败", color: "bg-red-500/20 text-red-400" },
    Cancelled: { label: "已取消", color: "bg-gray-500/20 text-gray-400" },
  };

  const config = statusConfig[job.status] || { label: job.status, color: "bg-accent" };

  return (
    <div className="group flex items-center gap-3 rounded-xl border border-border bg-card p-3 shadow-sm transition-all hover:border-primary/30 hover:shadow-md">
      {/* File icon */}
      <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-primary/10 ring-1 ring-primary/15">
        <FileVideo className="h-5 w-5 text-primary" />
      </div>

      {/* Job info */}
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2">
          <p className="truncate text-sm font-medium">
            {fileName}
          </p>
          <span className={`shrink-0 rounded-full px-2.5 py-0.5 text-[13px] font-medium ${config.color}`}>
            {config.label}
          </span>
        </div>
        <div className="mt-1 flex items-center gap-2">
          <div className="flex-1 min-w-0">
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
              className="flex h-8 shrink-0 items-center gap-1.5 rounded-md px-2 text-muted-foreground hover:bg-accent hover:text-foreground disabled:opacity-50"
            >
              <Gauge className={`h-4 w-4 ${vmafLoading ? "animate-spin" : ""}`} />
              <span className="text-[13px]">{vmafLoading ? "计算中" : job.vmafScore != null ? "重算 VMAF" : "VMAF"}</span>
            </button>
          )}
        </div>
      </div>

      {/* Actions (hover 显示) */}
      <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
        {job.status === "Failed" && (
          <button
            onClick={handleRetry}
            title="重新编码"
            className="flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground"
          >
            <RotateCw className="h-4 w-4" />
          </button>
        )}
        {job.status === "Encoding" && (
          <button
            onClick={() => cancelJob(job.id)}
            title="取消编码（终止 ffmpeg 进程）"
            className="flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground hover:bg-destructive/20 hover:text-destructive"
          >
            <Square className="h-4 w-4 fill-current" />
          </button>
        )}
        {(job.status === "Pending" || job.status === "Completed" || job.status === "Failed" || job.status === "Cancelled") && (
          <button
            onClick={() => removeJobs([job.id])}
            className="flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground hover:bg-destructive/20 hover:text-destructive"
          >
            <X className="h-4 w-4" />
          </button>
        )}
      </div>
    </div>
  );
}
