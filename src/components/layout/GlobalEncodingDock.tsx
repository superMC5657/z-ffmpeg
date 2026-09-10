import { useMemo } from "react";
import { useNavigate, useLocation } from "react-router-dom";
import { Play, Pause, X, ExternalLink, Loader2 } from "lucide-react";
import { useQueueStore } from "@/store/queueStore";
import type { EncodeProgress } from "@/types";

export default function GlobalEncodingDock() {
  const navigate = useNavigate();
  const location = useLocation();
  const jobs = useQueueStore((s) => s.jobs);
  const paused = useQueueStore((s) => s.paused);
  const pauseJobs = useQueueStore((s) => s.pauseJobs);
  const resumeJobs = useQueueStore((s) => s.resumeJobs);
  const cancelJob = useQueueStore((s) => s.cancelJob);

  const activeJobs = useMemo(
    () => jobs.filter((j) => j.status === "Encoding"),
    [jobs]
  );
  const pendingCount = useMemo(
    () => jobs.filter((j) => j.status === "Pending").length,
    [jobs]
  );
  const totalCount = jobs.length;
  const completedCount = useMemo(
    () => jobs.filter((j) => j.status === "Completed").length,
    [jobs]
  );

  const currentJob = activeJobs[0];

  // 如果没有正在编码的任务，且没有等待中的任务，则不占用底部空间
  if (!currentJob && pendingCount === 0) {
    return null;
  }

  const rawProgress = currentJob?.progress;
  const isProgressObj =
    typeof rawProgress === "object" && rawProgress !== null;
  const progressObj = isProgressObj ? (rawProgress as EncodeProgress) : null;

  const percentage = Math.round(
    progressObj?.percentage ??
      (typeof rawProgress === "number" ? rawProgress : 0)
  );
  const fps = progressObj?.fps ?? 0;
  const speed = progressObj?.speed ?? 0;
  const time = progressObj?.time ?? progressObj?.elapsed ?? "";

  const fileName = currentJob
    ? currentJob.inputPath.split(/[/\\]/).pop() || currentJob.inputPath
    : "准备中…";

  const isQueuePage = location.pathname === "/queue";

  return (
    <aside
      aria-label="全局转码状态"
      className="relative z-30 flex h-13 w-full shrink-0 items-center justify-between border-t border-hairline bg-surface/90 px-4 backdrop-blur-xl shadow-lg transition-all"
    >
      {/* 顶部极细进度指示线（整体进度） */}
      <div className="absolute top-0 left-0 right-0 h-[2px] bg-fill overflow-hidden">
        <div
          className="h-full bg-accent transition-all duration-300 ease-out"
          style={{ width: `${percentage}%` }}
        />
      </div>

      {/* 左侧：状态指示与当前任务名称 */}
      <div className="flex min-w-0 items-center gap-3">
        <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg bg-accent/10 text-accent">
          {paused ? (
            <Pause className="h-3.5 w-3.5 text-warning" />
          ) : (
            <Loader2 className="h-3.5 w-3.5 animate-spin text-accent" />
          )}
        </div>
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <span className="truncate text-[13px] font-medium text-foreground max-w-[220px] sm:max-w-[320px]">
              {fileName}
            </span>
            <span className="shrink-0 rounded bg-fill px-1.5 py-0.5 text-[10px] font-medium text-secondary tabular-nums">
              {completedCount + (currentJob ? 1 : 0)} / {totalCount}
            </span>
          </div>
          <div className="flex items-center gap-2 text-[11px] text-secondary tabular-nums">
            <span className="font-semibold text-accent">{percentage}%</span>
            {fps > 0 && (
              <>
                <span className="text-tertiary">·</span>
                <span>{fps} fps</span>
              </>
            )}
            {speed > 0 && (
              <>
                <span className="text-tertiary">·</span>
                <span>{speed.toFixed(1)}x</span>
              </>
            )}
            {time && (
              <>
                <span className="text-tertiary">·</span>
                <span>{time}</span>
              </>
            )}
            {paused && (
              <span className="rounded bg-warning/15 px-1 py-0.2 text-[10px] font-medium text-warning">
                队列已暂停
              </span>
            )}
          </div>
        </div>
      </div>

      {/* 右侧：操作按钮与跳转 */}
      <div className="flex items-center gap-1.5 shrink-0 pl-3">
        <button
          onClick={() => (paused ? resumeJobs() : pauseJobs())}
          title={paused ? "恢复队列调度" : "暂停队列调度"}
          className="flex h-7 items-center gap-1 rounded-md bg-fill px-2 text-[12px] font-medium text-foreground transition-colors hover:bg-fill-strong"
        >
          {paused ? (
            <>
              <Play className="h-3 w-3 fill-current text-success" />
              <span>继续</span>
            </>
          ) : (
            <>
              <Pause className="h-3 w-3 fill-current text-secondary" />
              <span>暂停</span>
            </>
          )}
        </button>

        {currentJob && (
          <button
            onClick={() => cancelJob(currentJob.id)}
            title="终止当前任务"
            className="flex h-7 items-center gap-1 rounded-md px-2 text-[12px] font-medium text-destructive transition-colors hover:bg-destructive/10"
          >
            <X className="h-3.5 w-3.5" />
            <span className="hidden sm:inline">终止</span>
          </button>
        )}

        {!isQueuePage && (
          <button
            onClick={() => navigate("/queue")}
            className="flex h-7 items-center gap-1 rounded-md bg-accent/10 px-2.5 text-[12px] font-medium text-accent transition-colors hover:bg-accent/15"
          >
            <span>队列</span>
            <ExternalLink className="h-3 w-3" />
          </button>
        )}
      </div>
    </aside>
  );
}
