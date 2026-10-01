import { X, RotateCw, Square, Gauge } from "lucide-react";
import { useState } from "react";
import { isEncodeProgress, type EncodeJob } from "@/types";
import { useQueueStore } from "@/store/queueStore";
import { useToastStore, showErrorToast } from "@/store/toastStore";
import { computeVmaf } from "@/lib/tauri";
import ProgressBar from "@/components/progress/ProgressBar";
import ProGate from "@/components/license/ProGate";
import { cn, getFileName } from "@/lib/utils";
import { StatusBadge, StatusIcon } from "@/components/layout/StatusBadge";

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
      showErrorToast("重试失败", err);
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
      showErrorToast("VMAF 计算失败", err);
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
    (isEncodeProgress(job.progress) ? job.progress.fileName : undefined) ||
    getFileName(job.inputPath);

  return (
    <div className="group flex items-center gap-3.5 px-3.5 py-3 transition-colors hover:bg-fill/40">
      {/* Status icon */}
      <StatusIcon status={job.status} />

      {/* Job info */}
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <p className="truncate text-[13px] font-medium leading-5">{fileName}</p>
          <StatusBadge status={job.status} />
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
          {/* VMAF 按钮 — Pro 功能，未激活时锁定并引导激活 */}
          {job.status === "Completed" && (
            <ProGate title="VMAF 质量对比为 Pro 功能，点击激活">
              <button
                onClick={handleComputeVmaf}
                disabled={vmafLoading}
                title={vmafButtonTitle}
                className="flex h-7 shrink-0 items-center gap-1 rounded-md px-1.5 text-[11px] text-secondary transition-colors hover:bg-fill-strong hover:text-foreground disabled:opacity-50"
              >
                <Gauge className={cn("h-3.5 w-3.5", vmafLoading && "animate-spin")} />
                {vmafLoading ? "计算中" : job.vmafScore != null ? "重算 VMAF" : "VMAF"}
              </button>
            </ProGate>
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
