import { useEffect } from "react";
import { useQueueStore } from "@/store/queueStore";
import {
  onEncodeProgress,
  onEncodeComplete,
  onEncodeError,
  onQueueUpdated,
} from "@/lib/tauri";
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

    const unlistenError = onEncodeError(({ jobId, error }) => {
      updateJobStatus(jobId, "Failed", error);
    });

    const unlistenQueue = onQueueUpdated((status) => {
      useQueueStore.setState({ paused: status.paused });
      useQueueStore.getState().setJobs(status.jobs);
    });

    return () => {
      unlistenProgress.then((fn) => fn());
      unlistenComplete.then((fn) => fn());
      unlistenError.then((fn) => fn());
      unlistenQueue.then((fn) => fn());
    };
  }, [updateProgress, updateJobStatus]);
}
