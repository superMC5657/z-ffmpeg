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
