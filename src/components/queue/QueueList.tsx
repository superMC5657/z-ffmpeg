import type { EncodeJob } from "@/types";
import { GroupedList } from "@/components/layout/GroupedList";
import QueueItem from "./QueueItem";

interface QueueListProps {
  jobs: EncodeJob[];
}

/** inset grouped 列表容器：圆角卡片内行间 hairline 分隔 */
export default function QueueList({ jobs }: QueueListProps) {
  return (
    <GroupedList>
      {jobs.map((job, i) => (
        <div key={job.id} className={i > 0 ? "border-t border-hairline" : ""}>
          <QueueItem job={job} />
        </div>
      ))}
    </GroupedList>
  );
}
