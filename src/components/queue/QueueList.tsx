import type { EncodeJob } from "@/types";
import QueueItem from "./QueueItem";

interface QueueListProps {
  jobs: EncodeJob[];
}

/** inset grouped 列表容器：圆角卡片内行间 hairline 分隔 */
export default function QueueList({ jobs }: QueueListProps) {
  return (
    <div className="overflow-hidden rounded-[14px] border border-hairline bg-surface shadow-card">
      {jobs.map((job, i) => (
        <div key={job.id} className={i > 0 ? "border-t border-hairline" : ""}>
          <QueueItem job={job} />
        </div>
      ))}
    </div>
  );
}
