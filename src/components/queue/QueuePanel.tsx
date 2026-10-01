import { Layers } from "lucide-react";
import { useQueueStore } from "@/store/queueStore";
import EmptyState from "@/components/layout/EmptyState";
import { SkeletonList } from "@/components/layout/Skeleton";
import QueueToolbar from "./QueueToolbar";
import QueueStats from "./QueueStats";
import QueueList from "./QueueList";

export default function QueuePanel() {
  const jobs = useQueueStore((s) => s.jobs);
  const isLoading = useQueueStore((s) => s.isLoading);

  return (
    <div className="space-y-4">
      <QueueToolbar />
      <QueueStats />
      {isLoading ? (
        <SkeletonList />
      ) : jobs.length === 0 ? (
        <EmptyState
          icon={Layers}
          title="队列为空"
          description="在编码页面添加文件到队列"
        />
      ) : (
        <QueueList jobs={jobs} />
      )}
    </div>
  );
}
