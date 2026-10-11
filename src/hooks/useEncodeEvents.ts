import { useEffect } from "react";
import { useQueueStore } from "@/store/queueStore";
import {
  onEncodeProgress,
  onEncodeComplete,
  onQueueUpdated,
} from "@/lib/tauri";
import { normalizeProgress } from "@/lib/progress";
import { isTauriRuntime } from "@/lib/utils";

/**
 * 全局 Hook，监听来自 Tauri 后端的编码事件，
 * 并相应更新队列与进度 Store。
 */
export function useEncodeEvents() {
  const updateProgress = useQueueStore((s) => s.updateProgress);
  const updateJobStatus = useQueueStore((s) => s.updateJobStatus);

  useEffect(() => {
    // Tauri 事件监听器仅在 WebView 运行时中存在
    if (!isTauriRuntime()) return;

    const unlistenProgress = onEncodeProgress((progress) => {
      // 经统一契约归一后写入：缺关键百分比字段的非法载荷直接丢弃，
      // 避免脏数据污染进度条；complete 映射行为保持不变。
      const normalized = normalizeProgress(progress);
      if (!normalized.isLive) return;
      updateProgress(progress);
    });

    const unlistenComplete = onEncodeComplete((result) => {
      updateJobStatus(
        result.jobId,
        result.cancelled
          ? "Cancelled"
          : result.success
            ? "Completed"
            : "Failed",
        result.error || undefined
      );
    });

    // 注：后端从不 emit `encode://error`，失败经 `encode://complete` 上报，
    // 此处不订阅该事件（死 listener）。

    const unlistenQueue = onQueueUpdated((status) => {
      useQueueStore.setState({ paused: status.paused });
      useQueueStore.getState().setJobs(status.jobs);
    });

    return () => {
      unlistenProgress.then((fn) => fn());
      unlistenComplete.then((fn) => fn());
      unlistenQueue.then((fn) => fn());
    };
  }, [updateProgress, updateJobStatus]);
}
