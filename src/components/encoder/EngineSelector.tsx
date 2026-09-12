import { useEffect } from "react";
import { Cpu, Zap, Film, Sparkles, Clapperboard, Globe } from "lucide-react";
import type { VideoCodec, ContainerFormat, HwAccelDevice } from "@/types";
import { useEncoderStore } from "@/store/encoderStore";
import { useSystemStore } from "@/store/systemStore";
import SegmentedControl from "@/components/layout/SegmentedControl";
import { cn } from "@/lib/utils";

const CODEC_ITEMS: {
  value: VideoCodec;
  label: string;
  sub: string;
  icon: typeof Film;
}[] = [
  { value: "H264", label: "H.264", sub: "全设备最高兼容", icon: Film },
  { value: "H265", label: "H.265 / HEVC", sub: "高压缩 · 小体积", icon: Clapperboard },
  { value: "AV1", label: "AV1", sub: "下一代开源前沿", icon: Sparkles },
  { value: "VP9", label: "VP9", sub: "Web 优化免专利", icon: Globe },
];

const CONTAINERS: { value: ContainerFormat; label: string }[] = [
  { value: "MP4", label: "MP4" },
  { value: "MKV", label: "MKV" },
  { value: "WebM", label: "WebM" },
  { value: "MOV", label: "MOV" },
];

export default function EngineSelector() {
  const videoCodec = useEncoderStore((s) => s.videoCodec);
  const setVideoCodec = useEncoderStore((s) => s.setVideoCodec);
  const containerFormat = useEncoderStore((s) => s.containerFormat);
  const setContainerFormat = useEncoderStore((s) => s.setContainerFormat);
  const hwAccel = useEncoderStore((s) => s.hwAccel);
  const setHwAccel = useEncoderStore((s) => s.setHwAccel);

  const hwList = useSystemStore((s) => s.hwAccels);
  const fetchHwAccels = useSystemStore((s) => s.fetchHwAccels);

  useEffect(() => {
    fetchHwAccels();
  }, [fetchHwAccels]);

  const availableHw = hwList.filter((h) => h.available);

  const handleSelectHw = (device: HwAccelDevice | null) => {
    if (device === null) {
      setHwAccel(null);
      return;
    }
    setHwAccel({ device, deviceIndex: null });
  };

  return (
    <div className="space-y-4">
      {/* 1. 硬件加速方案 */}
      <div>
        <div className="mb-2 flex items-center justify-between">
          <label className="text-[13px] font-semibold text-secondary">
            硬件与加速引擎
          </label>
          <span className="text-[12px] text-tertiary">
            {hwAccel ? `已启用 ${hwAccel.device} 硬件加速` : "使用 CPU 软件编码"}
          </span>
        </div>

        <div className="grid grid-cols-2 gap-2.5">
          {/* CPU 软件编码卡片 */}
          <button
            type="button"
            onClick={() => handleSelectHw(null)}
            className={cn(
              "flex items-center gap-3 rounded-xl border p-3 text-left transition-all",
              hwAccel === null
                ? "border-accent bg-accent/10 shadow-xs ring-1 ring-accent/20"
                : "border-hairline bg-fill/40 hover:bg-fill hover:border-hairline/80"
            )}
          >
            <div
              className={cn(
                "flex h-8.5 w-8.5 shrink-0 items-center justify-center rounded-lg transition-colors",
                hwAccel === null
                  ? "bg-accent text-on-accent"
                  : "bg-fill text-secondary"
              )}
            >
              <Cpu className="h-4.5 w-4.5" />
            </div>
            <div className="min-w-0">
              <div className="text-[13px] font-bold text-foreground">CPU 软件编码</div>
              <div className="truncate text-[11px] text-secondary">
                画质最高 · 依赖 CPU
              </div>
            </div>
          </button>

          {/* 可用的硬件加速卡片 */}
          {availableHw.map((hw) => {
            const selected = hwAccel?.device === hw.device;
            return (
              <button
                key={hw.device}
                type="button"
                onClick={() => handleSelectHw(hw.device as HwAccelDevice)}
                className={cn(
                  "flex items-center gap-3 rounded-xl border p-3 text-left transition-all",
                  selected
                    ? "border-accent bg-accent/10 shadow-xs ring-1 ring-accent/20"
                    : "border-hairline bg-fill/40 hover:bg-fill hover:border-hairline/80"
                )}
              >
                <div
                  className={cn(
                    "flex h-8.5 w-8.5 shrink-0 items-center justify-center rounded-lg transition-colors",
                    selected
                      ? "bg-accent text-on-accent"
                      : "bg-fill text-secondary"
                  )}
                >
                  <Zap className="h-4.5 w-4.5 fill-current" />
                </div>
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-1.5 text-[13px] font-bold text-foreground">
                    <span className="truncate">{hw.device}</span>
                  </div>
                  <div className="truncate text-[11px] text-secondary">
                    GPU 极速硬件加速
                  </div>
                </div>
              </button>
            );
          })}
        </div>
      </div>

      {/* 2. 视频编码格式选择 */}
      <div>
        <label className="mb-2 block text-[13px] font-semibold text-secondary">
          视频编码格式
        </label>
        <div className="grid grid-cols-2 gap-2">
          {CODEC_ITEMS.map(({ value, label, sub, icon: Icon }) => {
            const selected = videoCodec === value;
            return (
              <button
                key={value}
                type="button"
                onClick={() => setVideoCodec(value)}
                className={cn(
                  "flex items-center gap-2.5 rounded-xl border p-2.5 text-left transition-all",
                  selected
                    ? "border-accent bg-accent/10 shadow-xs ring-1 ring-accent/20"
                    : "border-hairline bg-fill/30 hover:bg-fill/70"
                )}
              >
                <div
                  className={cn(
                    "flex h-7.5 w-7.5 shrink-0 items-center justify-center rounded-lg",
                    selected
                      ? "bg-accent/20 text-accent font-bold"
                      : "bg-fill text-secondary"
                  )}
                >
                  <Icon className="h-4 w-4" />
                </div>
                <div className="min-w-0">
                  <div className="text-[13px] font-medium text-foreground leading-tight">
                    {label}
                  </div>
                  <div className="truncate text-[11px] text-secondary mt-0.5">
                    {sub}
                  </div>
                </div>
              </button>
            );
          })}
        </div>
      </div>

      {/* 3. 封装格式 */}
      <div className="flex items-center justify-between border-t border-hairline/60 pt-3.5">
        <label className="text-[13px] font-medium text-secondary">容器封装格式</label>
        <SegmentedControl
          value={containerFormat}
          onChange={(v) => setContainerFormat(v)}
          options={CONTAINERS}
        />
      </div>
    </div>
  );
}
