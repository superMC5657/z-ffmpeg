import { Layers } from "lucide-react";
import { useQueueStore } from "@/store/queueStore";
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
        <div className="space-y-2">
          {[0, 1, 2].map((i) => (
            <div key={i} className="h-16 animate-pulse rounded-[10px] bg-fill/70" />
          ))}
        </div>
      ) : jobs.length === 0 ? (
        <div className="flex flex-col items-center justify-center rounded-[14px] border border-dashed border-hairline py-16">
          <div className="flex h-11 w-11 items-center justify-center rounded-[12px] bg-fill">
            <Layers className="h-5 w-5 text-tertiary" />
          </div>
          <p className="mt-3 text-[13px] font-medium">队列为空</p>
          <p className="mt-0.5 text-[12px] text-secondary">
            在编码页面添加文件到队列
          </p>
        </div>
      ) : (
        <QueueList jobs={jobs} />
      )}
    </div>
  );
}
