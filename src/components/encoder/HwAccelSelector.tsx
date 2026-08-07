import { useEffect } from "react";
import type { HwAccelConfig } from "@/types";
import { useEncoderStore } from "@/store/encoderStore";
import { useSystemStore } from "@/store/systemStore";
import { cn } from "@/lib/utils";

export default function HwAccelSelector() {
  const hwAccel = useEncoderStore((s) => s.hwAccel);
  const setHwAccel = useEncoderStore((s) => s.setHwAccel);
  const hwList = useSystemStore((s) => s.hwAccels);
  const loading = useSystemStore((s) => s.loading);
  const fetchHwAccels = useSystemStore((s) => s.fetchHwAccels);

  useEffect(() => {
    fetchHwAccels();
  }, []);

  const available = hwList.filter((h) => h.available);

  if (loading) {
    return (
      <div className="rounded-xl border border-border bg-card p-5 shadow-sm">
        <div className="h-3 w-28 animate-pulse rounded bg-accent" />
        <div className="mt-3 flex gap-2">
          <div className="h-9 w-24 animate-pulse rounded-md bg-accent" />
          <div className="h-9 w-28 animate-pulse rounded-md bg-accent" />
        </div>
      </div>
    );
  }

  return (
    <div className="rounded-xl border border-border bg-card p-5 shadow-sm">
      <div className="mb-3 flex items-center justify-between">
        <h3 className="text-[15px] font-semibold">硬件加速</h3>
        <button
          onClick={() => setHwAccel(null)}
          className={`text-[14px] font-medium transition-colors ${
            hwAccel === null
              ? "text-primary"
              : "text-muted-foreground hover:text-foreground"
          }`}
        >
          软件编码
        </button>
      </div>

      <div className="flex flex-wrap gap-2">
        {available.length === 0 && (
          <p className="text-[13px] text-muted-foreground">
            未检测到可用硬件加速器，将使用软件编码
          </p>
        )}
        {available.map((hw) => (
          <button
            key={hw.device}
            // 再点一次已选中的项 → 取消选中(回到软件编码)
            onClick={() =>
              setHwAccel(
                hwAccel?.device === hw.device
                  ? null
                  : {
                      device: hw.device as HwAccelConfig["device"],
                      deviceIndex: null,
                    }
              )
            }
            className={cn(
              "rounded-md border px-3.5 py-2 text-[14px] font-medium transition-all",
              hwAccel?.device === hw.device
                ? "border-primary bg-primary/10 text-primary ring-1 ring-primary/50"
                : "border-border bg-accent/50 text-muted-foreground hover:border-primary/40 hover:text-foreground"
            )}
          >
            <span className="flex items-center gap-1.5">
              <span className="h-1.5 w-1.5 rounded-full bg-success" />
              {hw.device}
            </span>
            <span className="mt-0.5 block text-[13px] opacity-60">
              {hw.deviceName ||
                hw.supportedCodecs
                  ?.map((c: { codec: string }) => c.codec.toUpperCase())
                  .join(" ") ||
                "可用"}
            </span>
          </button>
        ))}
      </div>
    </div>
  );
}
