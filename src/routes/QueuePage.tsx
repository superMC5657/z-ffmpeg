import { useEffect } from "react";
import QueuePanel from "@/components/queue/QueuePanel";
import PageHeader from "@/components/layout/PageHeader";
import { useQueueStore } from "@/store/queueStore";
import { onQueueUpdated } from "@/lib/tauri";
import { isTauriRuntime } from "@/lib/utils";

export default function QueuePage() {
  const refreshQueue = useQueueStore((s) => s.refreshQueue);
  const setJobs = useQueueStore((s) => s.setJobs);
  const fetchMaxConcurrent = useQueueStore((s) => s.fetchMaxConcurrent);
  const fetchVmafSegments = useQueueStore((s) => s.fetchVmafSegments);

  useEffect(() => {
    // Initial load
    refreshQueue();
    fetchMaxConcurrent();
    fetchVmafSegments();

    // Tauri-only: listen for backend queue updates
    if (!isTauriRuntime()) return;

    // Listen for queue updates from backend
    const unlisten = onQueueUpdated((status) => {
      // 同步队列暂停状态（pause/resume/cancel 等操作都会触发该事件）
      useQueueStore.setState({ paused: status.paused });
      setJobs(status.jobs);
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  return (
    <div>
      <PageHeader
        title="编码队列"
        description="管理批量编码任务，查看实时进度"
      />
      <QueuePanel />
    </div>
  );
}
