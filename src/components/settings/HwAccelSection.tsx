import { useEffect } from "react";
import { useSystemStore } from "@/store/systemStore";
import Card from "@/components/layout/Card";
import { cn } from "@/lib/utils";

export default function HwAccelSection() {
  const hwAccels = useSystemStore((s) => s.hwAccels);
  const fetchHwAccels = useSystemStore((s) => s.fetchHwAccels);

  useEffect(() => {
    fetchHwAccels();
  }, []);

  return (
    <Card title="硬件加速器">
      <div className="grid gap-3 sm:grid-cols-2">
        {hwAccels.map((hw) => (
          <div
            key={hw.device}
            className={cn(
              "rounded-[9px] p-3.5",
              hw.available ? "bg-success/8" : "bg-fill"
            )}
          >
            <div className="flex items-center gap-2">
              <span
                className={cn(
                  "h-2 w-2 rounded-full",
                  hw.available ? "bg-success" : "bg-tertiary"
                )}
              />
              <span className="text-[13px] font-semibold">{hw.device}</span>
            </div>
            <p className="mt-1.5 text-[12px] leading-5 text-secondary">
              {hw.available ? hw.deviceName : "未检测到"}
            </p>
            {hw.supportedCodecs && hw.supportedCodecs.length > 0 && (
              <div className="mt-2 flex flex-wrap gap-1">
                {hw.supportedCodecs.map((c: {codec: string; encoder: string}) => (
                  <span
                    key={c.codec}
                    className="rounded-md bg-surface px-2 py-0.5 text-[11px] font-medium text-secondary"
                  >
                    {c.codec.toUpperCase()}
                  </span>
                ))}
              </div>
            )}
          </div>
        ))}
      </div>
    </Card>
  );
}
