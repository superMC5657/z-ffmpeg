import { Layers } from "lucide-react";
import { useQueueStore } from "@/store/queueStore";
import QueueToolbar from "./QueueToolbar";
import QueueList from "./QueueList";
import QueueStats from "./QueueStats";

export default function QueuePanel() {
  const jobs = useQueueStore((s) => s.jobs);
  const isLoading = useQueueStore((s) => s.isLoading);

  return (
    <div className="space-y-6">
      <QueueStats />
      <QueueToolbar />
      {isLoading ? (
        <div className="flex items-center justify-center py-16">
          <p className="text-sm text-muted-foreground">加载中...</p>
        </div>
      ) : jobs.length === 0 ? (
        <div className="flex flex-col items-center justify-center rounded-xl border border-dashed border-border bg-card/40 py-20">
          <div className="mb-3 flex h-14 w-14 items-center justify-center rounded-2xl bg-accent">
            <Layers className="h-7 w-7 text-muted-foreground/50" />
          </div>
          <p className="text-sm font-medium text-foreground/80">队列为空</p>
          <p className="mt-1 text-[13px] text-muted-foreground">
            在编码页面添加文件到队列
          </p>
        </div>
      ) : (
        <QueueList jobs={jobs} />
      )}
    </div>
  );
}
