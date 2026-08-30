import { useEffect } from "react";
import { Check, Cpu } from "lucide-react";
import type { HwAccelConfig } from "@/types";
import { useEncoderStore } from "@/store/encoderStore";
import { useSystemStore } from "@/store/systemStore";
import { useLicenseStore } from "@/store/licenseStore";
import { ProBadge } from "@/components/license/ProGate";
import { cn } from "@/lib/utils";

/** 硬件加速选择（卡片外壳由父级 Card 提供，此处只输出内容） */
export default function HwAccelSelector() {
  const hwAccel = useEncoderStore((s) => s.hwAccel);
  const setHwAccel = useEncoderStore((s) => s.setHwAccel);
  const hwList = useSystemStore((s) => s.hwAccels);
  const loading = useSystemStore((s) => s.loading);
  const fetchHwAccels = useSystemStore((s) => s.fetchHwAccels);
  const isPro = useLicenseStore((s) => s.status?.pro === true);
  const setActivationOpen = useLicenseStore((s) => s.setActivationOpen);

  useEffect(() => {
    fetchHwAccels();
  }, []);

  if (loading) {
    return (
      <div className="animate-pulse space-y-2.5">
        <div className="h-10 w-32 rounded-[10px] bg-fill" />
        <div className="h-10 w-40 rounded-[10px] bg-fill" />
      </div>
    );
  }

  const available = hwList.filter((h) => h.available);

  const optionCls = (selected: boolean) =>
    cn(
      "flex min-w-36 items-start gap-2 rounded-[10px] border px-3 py-2 text-left transition-all",
      selected
        ? "border-accent bg-accent/[0.06]"
        : "border-hairline bg-fill/40 hover:bg-fill"
    );

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap gap-2.5">
        {/* 软件编码：不使用硬件加速器 */}
        <button
          onClick={() => setHwAccel(null)}
          aria-pressed={hwAccel === null}
          className={optionCls(hwAccel === null)}
        >
          <div
            className={cn(
              "mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center rounded-full border transition-colors",
              hwAccel === null
                ? "border-accent bg-accent"
                : "border-tertiary bg-transparent"
            )}
          >
            {hwAccel === null && (
              <Check className="h-3 w-3 text-on-accent" strokeWidth={3.5} />
            )}
          </div>
          <div className="min-w-0">
            <div className="flex items-center gap-1.5 text-[13px] font-medium">
              <Cpu className="h-3.5 w-3.5" />
              软件编码
            </div>
            <div className="mt-0.5 text-[11px] leading-4 text-secondary">
              兼容性最好，速度取决于 CPU
            </div>
          </div>
        </button>

        {available.map((hw) => {
          const selected = hwAccel?.device === hw.device;
          return (
            <button
              key={hw.device}
              aria-pressed={selected}
              // Pro 门控：未激活时点击硬件加速项拉起激活对话框（后端命令层同步强制）
              onClick={() => {
                if (!isPro) {
                  setActivationOpen(true);
                  return;
                }
                setHwAccel(
                  selected
                    ? null
                    : {
                        device: hw.device as HwAccelConfig["device"],
                        deviceIndex: null,
                      }
                );
              }}
              className={optionCls(selected)}
            >
              <div
                className={cn(
                  "mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center rounded-full border transition-colors",
                  selected
                    ? "border-accent bg-accent"
                    : "border-tertiary bg-transparent"
                )}
              >
                {selected && (
                  <Check className="h-3 w-3 text-on-accent" strokeWidth={3.5} />
                )}
              </div>
              <div className="min-w-0">
                <div className="flex items-center gap-1.5 text-[13px] font-medium">
                  <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-success" />
                  {hw.device}
                  {!isPro && <ProBadge />}
                </div>
                <div className="mt-0.5 truncate text-[11px] leading-4 text-secondary" title={hw.deviceName || undefined}>
                  {hw.deviceName ||
                    hw.supportedCodecs
                      ?.map((c: { codec: string }) => c.codec.toUpperCase())
                      .join(" ") ||
                    "可用"}
                </div>
              </div>
            </button>
          );
        })}
      </div>

      {available.length === 0 && (
        <p className="text-[12px] text-secondary">
          未检测到可用硬件加速器，将使用软件编码
        </p>
      )}
    </div>
  );
}
