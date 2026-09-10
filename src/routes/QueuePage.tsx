import { useEffect } from "react";
import QueuePanel from "@/components/queue/QueuePanel";
import PageHeader from "@/components/layout/PageHeader";
import { useQueueStore } from "@/store/queueStore";

export default function QueuePage() {
  const refreshQueue = useQueueStore((s) => s.refreshQueue);
  const fetchMaxConcurrent = useQueueStore((s) => s.fetchMaxConcurrent);
  const fetchVmafSegments = useQueueStore((s) => s.fetchVmafSegments);

  useEffect(() => {
    // Initial load
    refreshQueue();
    fetchMaxConcurrent();
    fetchVmafSegments();
  }, [refreshQueue, fetchMaxConcurrent, fetchVmafSegments]);

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
