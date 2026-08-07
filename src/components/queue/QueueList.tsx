import type { EncodeJob } from "@/types";
import QueueItem from "./QueueItem";

interface QueueListProps {
  jobs: EncodeJob[];
}

export default function QueueList({ jobs }: QueueListProps) {
  return (
    <div className="space-y-2">
      {jobs.map((job) => (
        <QueueItem key={job.id} job={job} />
      ))}
    </div>
  );
}
