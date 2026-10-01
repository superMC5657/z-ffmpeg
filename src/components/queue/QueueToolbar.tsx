import { useEffect, useState } from "react";
import { Layers, Pause, Play, Trash2, Zap } from "lucide-react";
import { useQueueStore } from "@/store/queueStore";
import { useToastStore, showErrorToast } from "@/store/toastStore";
import AppleButton from "@/components/layout/AppleButton";
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
  const fetchMaxConcurrent = useQueueStore((s) => s.fetchMaxConcurrent);
  const updateMaxConcurrent = useQueueStore((s) => s.updateMaxConcurrent);
  const [savingConcurrent, setSavingConcurrent] = useState(false);

  useEffect(() => {
    fetchMaxConcurrent();
  }, [fetchMaxConcurrent]);

  // 队列并发数上限 16
  const concurrencyCap = 16;

  const hasPending = jobs.some((j) => j.status === "Pending");
  const hasCompleted = jobs.some(
    (j) => j.status === "Completed" || j.status === "Failed" || j.status === "Cancelled"
  );

  const handleStart = async () => {
    try {
      await startJobs();
      useToastStore.getState().showToast("队列开始执行", "success");
    } catch (err) {
      showErrorToast("开始执行失败", err);
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
      showErrorToast(`${paused ? "恢复" : "暂停"}失败`, err);
    }
  };

  const handleConcurrentChange = async (value: number) => {
    setSavingConcurrent(true);
    try {
      const saved = await updateMaxConcurrent(value);
      useToastStore.getState().showToast(
        `队列并发数已更新为 ${saved}`,
        "success"
      );
    } catch (err) {
      showErrorToast("保存失败", err);
    } finally {
      setSavingConcurrent(false);
    }
  };

  return (
    <div className="flex flex-wrap items-center gap-2.5 rounded-[14px] border border-hairline bg-surface p-3 shadow-card">
      <AppleButton
        variant="primary"
        disabled={!hasPending || paused}
        title={paused ? "队列已暂停，先恢复调度" : undefined}
        onClick={handleStart}
      >
        <Zap className="h-3.5 w-3.5" />
        开始执行
      </AppleButton>

      <AppleButton
        variant={paused ? "primary" : "secondary"}
        disabled={!hasPending && !paused}
        onClick={handleTogglePause}
      >
        {paused ? <Play className="h-3.5 w-3.5" /> : <Pause className="h-3.5 w-3.5" />}
        {paused ? "恢复调度" : "暂停调度"}
      </AppleButton>

      <AppleButton
        variant="secondary"
        disabled={!hasCompleted}
        onClick={clearCompleted}
        className="hover:bg-destructive/10 hover:text-destructive"
      >
        <Trash2 className="h-3.5 w-3.5" />
        清除已完成
      </AppleButton>

      {/* Concurrency control — shared with Settings page */}
      <div className="ml-auto flex items-center gap-2">
        <Layers className="h-3.5 w-3.5 text-tertiary" />
        <span className="text-[12px] text-secondary">并发</span>
        <AppleSelect
          className="w-16"
          value={Math.min(maxConcurrent, concurrencyCap)}
          disabled={!maxConcurrentLoaded || savingConcurrent}
          onChange={(e) => handleConcurrentChange(parseInt(e.target.value, 10))}
        >
          {Array.from({ length: concurrencyCap }, (_, i) => i + 1).map((n) => (
            <option key={n} value={n}>
              {n}
            </option>
          ))}
        </AppleSelect>
      </div>
    </div>
  );
}
