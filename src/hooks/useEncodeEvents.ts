import { useEffect } from "react";
import { useQueueStore } from "@/store/queueStore";
import { useEncoderStore } from "@/store/encoderStore";
import { onEncodeProgress, onEncodeComplete, onEncodeError } from "@/lib/tauri";
import { isTauriRuntime } from "@/lib/utils";

/**
 * Global hook that listens for encoding events from the Tauri backend
 * and updates the queue/progress stores accordingly.
 */
export function useEncodeEvents() {
  const updateProgress = useQueueStore((s) => s.updateProgress);
  const updateJobStatus = useQueueStore((s) => s.updateJobStatus);
  const setIsEncoding = useEncoderStore((s) => s.setIsEncoding);

  useEffect(() => {
    // Tauri event listeners only exist inside the WebView runtime
    if (!isTauriRuntime()) return;

    const unlistenProgress = onEncodeProgress((progress) => {
      updateProgress(progress);
    });

    const unlistenComplete = onEncodeComplete((result) => {
      setIsEncoding(false);
      // 结构化 cancelled 字段是主判断；旧版后端只发魔法字符串，
      // 保留字符串匹配兼容一个版本周期
      const isCancelled =
        result.cancelled === true || result.error === "Cancelled by user";
      updateJobStatus(
        result.jobId,
        isCancelled ? "Cancelled" : result.success ? "Completed" : "Failed",
        result.error || undefined
      );
    });

    const unlistenError = onEncodeError(({ jobId, error }) => {
      setIsEncoding(false);
      updateJobStatus(jobId, "Failed", error);
    });

    return () => {
      unlistenProgress.then((fn) => fn());
      unlistenComplete.then((fn) => fn());
      unlistenError.then((fn) => fn());
    };
  }, [updateProgress, updateJobStatus, setIsEncoding]);
}
