import { Layers, Trash2, Zap } from "lucide-react";
import { useQueueStore } from "@/store/queueStore";
import { useToastStore } from "@/store/toastStore";

export default function QueueToolbar() {
  const clearCompleted = useQueueStore((s) => s.clearCompleted);
  const startJobs = useQueueStore((s) => s.startJobs);
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
    <div className="flex flex-wrap items-center gap-2 rounded-xl border border-border bg-card p-2 shadow-sm">
      <button
        onClick={handleStart}
        disabled={!hasPending}
        className={`flex items-center gap-1.5 rounded-md px-4 py-2.5 text-[14px] font-medium transition-colors ${
          hasPending
            ? "bg-gradient-brand text-white shadow-md shadow-primary/25 hover:brightness-110"
            : "cursor-not-allowed bg-accent/60 text-muted-foreground/50"
        }`}
      >
        <Zap className="h-4 w-4" />
        开始执行
      </button>

      <div className="h-5 w-px bg-border" />

      <button
        onClick={clearCompleted}
        disabled={!hasCompleted}
        className={`flex items-center gap-1.5 rounded-md px-4 py-2.5 text-[14px] font-medium transition-colors ${
          hasCompleted
            ? "text-muted-foreground hover:bg-destructive/20 hover:text-destructive"
            : "cursor-not-allowed text-muted-foreground/40"
        }`}
      >
        <Trash2 className="h-4 w-4" />
        清除已完成
      </button>

      {/* Concurrency control — shared with Settings page */}
      <div className="ml-auto flex items-center gap-2">
        <Layers className="h-4 w-4 text-muted-foreground" />
        <span className="text-[13px] text-muted-foreground">并发</span>
        <select
          value={maxConcurrent}
          disabled={!maxConcurrentLoaded}
          onChange={(e) => handleConcurrentChange(parseInt(e.target.value))}
          className="rounded-md border border-border bg-accent px-2.5 py-1.5 text-[14px] transition-colors hover:border-primary/40 focus:border-primary focus:outline-none disabled:cursor-not-allowed disabled:opacity-50"
        >
          {Array.from({ length: 16 }, (_, i) => i + 1).map((n) => (
            <option key={n} value={n}>
              {n}
            </option>
          ))}
        </select>
        <span className="text-xs text-muted-foreground">同时编码</span>
      </div>
    </div>
  );
}
