import { useQueueStore } from "@/store/queueStore";
import { cn } from "@/lib/utils";

/** 轻量统计条：圆点 + 标签 + 数字（tabular） */
export default function QueueStats() {
  const jobs = useQueueStore((s) => s.jobs);

  const pending = jobs.filter((j) => j.status === "Pending").length;
  const encoding = jobs.filter((j) => j.status === "Encoding").length;
  const completed = jobs.filter((j) => j.status === "Completed").length;
  const failed = jobs.filter((j) => j.status === "Failed").length;

  const stats = [
    { label: "等待中", value: pending, dot: "bg-warning" },
    { label: "编码中", value: encoding, dot: "bg-accent" },
    { label: "已完成", value: completed, dot: "bg-success" },
    { label: "失败", value: failed, dot: "bg-destructive" },
  ];

  if (jobs.length === 0) return null;

  return (
    <div className="flex items-center gap-6 px-1">
      {stats.map(({ label, value, dot }) => (
        <div key={label} className="flex items-center gap-1.5">
          <span className={cn("h-1.5 w-1.5 rounded-full", dot)} />
          <span className="text-[12px] text-secondary">{label}</span>
          <span className="text-[13px] font-semibold tabular-nums">{value}</span>
        </div>
      ))}
    </div>
  );
}
