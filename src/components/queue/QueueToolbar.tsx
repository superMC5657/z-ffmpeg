import { Layers, Pause, Play, Trash2, Zap } from "lucide-react";
import { useQueueStore } from "@/store/queueStore";
import { useToastStore } from "@/store/toastStore";
import AppleSelect from "@/components/layout/AppleSelect";

export default function QueueToolbar() {
  const clearCompleted = useQueueStore((s) => s.clearCompleted);
  const startJobs = useQueueStore((s) => s.startJobs);
  const pauseJobs = useQueueStore((s) => s.pauseJobs);
  const resumeJobs = useQueueStore((s) => s.resumeJobs);
  const paused = useQueueStore((s) => s.paused);
  const jobs = useQueueStore((s) => s.jobs);
  const maxConcurrent = useQueueStore((s) => s.maxConcurrent);
  const maxConcurrentLoaded = useQueueStore((s) => s.maxConcurrentLoaded);
  const updateMaxConcurrent = useQueueStore((s) => s.updateMaxConcurrent);

  const hasPending = jobs.some((j) => j.status === "Pending");
  const hasCompleted = jobs.some(
    (j) => j.status === "Completed" || j.status === "Failed" || j.status === "Cancelled"
  );

  const handleStart = async () => {
    try {
      await startJobs();
      useToastStore.getState().showToast("队列开始执行", "success");
    } catch (err) {
      useToastStore.getState().showToast(
        `开始执行失败: ${err instanceof Error ? err.message : String(err)}`,
        "error"
      );
    }
  };

  const handleTogglePause = async () => {
    try {
      if (paused) {
        await resumeJobs();
        useToastStore.getState().showToast("队列已恢复调度", "success");
      } else {
        await pauseJobs();
        useToastStore.getState().showToast("队列已暂停：正在编码的任务继续，剩余任务暂不开始", "info");
      }
    } catch (err) {
      useToastStore.getState().showToast(
        `${paused ? "恢复" : "暂停"}失败: ${err instanceof Error ? err.message : String(err)}`,
        "error"
      );
    }
  };

  const handleConcurrentChange = async (value: number) => {
    try {
      const saved = await updateMaxConcurrent(value);
      useToastStore.getState().showToast(
        `并发数已更新为 ${saved}`,
        "success"
      );
    } catch (err) {
      useToastStore.getState().showToast(
        `更新失败: ${err instanceof Error ? err.message : String(err)}`,
        "error"
      );
    }
  };

  return (
    <div className="flex flex-wrap items-center gap-2.5 rounded-[14px] border border-hairline bg-surface p-3 shadow-card">
      <button
        onClick={handleStart}
        disabled={!hasPending || paused}
        title={paused ? "队列已暂停，先恢复调度" : undefined}
        className={`flex h-9 items-center gap-1.5 rounded-[9px] px-4 text-[13px] font-medium transition-all active:scale-[0.98] ${
          hasPending && !paused
            ? "bg-accent text-on-accent shadow-sm hover:bg-accent-hover"
            : "cursor-default bg-fill text-tertiary"
        }`}
      >
        <Zap className="h-3.5 w-3.5" />
        开始执行
      </button>

      <button
        onClick={handleTogglePause}
        disabled={!hasPending && !paused}
        className={`flex h-9 items-center gap-1.5 rounded-[9px] px-3.5 text-[13px] font-medium transition-colors disabled:opacity-40 ${
          paused
            ? "bg-accent text-on-accent shadow-sm hover:bg-accent-hover"
            : "text-secondary hover:bg-fill-strong hover:text-foreground"
        }`}
      >
        {paused ? <Play className="h-3.5 w-3.5" /> : <Pause className="h-3.5 w-3.5" />}
        {paused ? "恢复调度" : "暂停调度"}
      </button>

      <button
        onClick={clearCompleted}
        disabled={!hasCompleted}
        className="flex h-9 items-center gap-1.5 rounded-[9px] px-3.5 text-[13px] font-medium text-secondary transition-colors hover:bg-destructive/10 hover:text-destructive disabled:opacity-40 disabled:hover:bg-transparent disabled:hover:text-secondary"
      >
        <Trash2 className="h-3.5 w-3.5" />
        清除已完成
      </button>

      {/* Concurrency control — shared with Settings page */}
      <div className="ml-auto flex items-center gap-2">
        <Layers className="h-3.5 w-3.5 text-tertiary" />
        <span className="text-[12px] text-secondary">并发</span>
        <AppleSelect
          className="w-16"
          value={maxConcurrent}
          disabled={!maxConcurrentLoaded}
          onChange={(e) => handleConcurrentChange(parseInt(e.target.value))}
        >
          {Array.from({ length: 16 }, (_, i) => i + 1).map((n) => (
            <option key={n} value={n}>
              {n}
            </option>
          ))}
        </AppleSelect>
      </div>
    </div>
  );
}
