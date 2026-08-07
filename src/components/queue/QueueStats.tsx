import { useQueueStore } from "@/store/queueStore";
import { CheckCircle2, Clock, Loader2, XCircle } from "lucide-react";

export default function QueueStats() {
  const jobs = useQueueStore((s) => s.jobs);

  const pending = jobs.filter((j) => j.status === "Pending").length;
  const encoding = jobs.filter((j) => j.status === "Encoding").length;
  const completed = jobs.filter((j) => j.status === "Completed").length;
  const failed = jobs.filter((j) => j.status === "Failed").length;

  const stats = [
    { label: "等待中", value: pending, icon: Clock, cls: "text-yellow-400 bg-yellow-500/10" },
    { label: "编码中", value: encoding, icon: Loader2, cls: "text-blue-400 bg-blue-500/10" },
    { label: "已完成", value: completed, icon: CheckCircle2, cls: "text-green-400 bg-green-500/10" },
    { label: "失败", value: failed, icon: XCircle, cls: "text-red-400 bg-red-500/10" },
  ];

  return (
    <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
      {stats.map(({ label, value, icon: Icon, cls }) => (
        <div
          key={label}
          className="flex items-center gap-3 rounded-xl border border-border bg-card p-4 shadow-sm"
        >
          <div className={`flex h-10 w-10 shrink-0 items-center justify-center rounded-lg ${cls}`}>
            <Icon className="h-5 w-5" />
          </div>
          <div>
            <div className="text-2xl font-bold leading-none">{value}</div>
            <div className="mt-1.5 text-[13px] text-muted-foreground">{label}</div>
          </div>
        </div>
      ))}
    </div>
  );
}
