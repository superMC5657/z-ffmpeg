import type { VideoCodec, ContainerFormat } from "@/types";
import { useEncoderStore } from "@/store/encoderStore";
import { Film, Clapperboard, Sparkles, Globe, Check } from "lucide-react";
import SegmentedControl from "@/components/layout/SegmentedControl";
import { cn } from "@/lib/utils";

const CODECS: {
  value: VideoCodec;
  label: string;
  desc: string;
  icon: typeof Film;
}[] = [
  { value: "H264", label: "H.264", desc: "兼容性最好，通用选择", icon: Film },
  { value: "H265", label: "H.265", desc: "更高压缩率，文件更小", icon: Clapperboard },
  { value: "AV1", label: "AV1", desc: "最新编码，最优压缩率", icon: Sparkles },
  { value: "VP9", label: "VP9", desc: "Web 优化，开源免专利", icon: Globe },
];

const CONTAINERS: { value: ContainerFormat; label: string }[] = [
  { value: "MP4", label: "MP4" },
  { value: "MKV", label: "MKV" },
  { value: "WebM", label: "WebM" },
  { value: "MOV", label: "MOV" },
];

export default function CodecSelector() {
  const videoCodec = useEncoderStore((s) => s.videoCodec);
  const setVideoCodec = useEncoderStore((s) => s.setVideoCodec);
  const containerFormat = useEncoderStore((s) => s.containerFormat);
  const setContainerFormat = useEncoderStore((s) => s.setContainerFormat);

  return (
    <div className="space-y-4">
      {/* 编码器选择卡 */}
      <div className="grid grid-cols-2 gap-2.5">
        {CODECS.map(({ value, label, desc, icon: Icon }) => {
          const selected = videoCodec === value;
          return (
            <button
              key={value}
              onClick={() => setVideoCodec(value)}
              aria-pressed={selected}
              className={cn(
                "relative rounded-[11px] border p-3 text-left transition-all",
                selected
                  ? "border-accent bg-accent/[0.06]"
                  : "border-hairline bg-fill/40 hover:bg-fill"
              )}
            >
              <div className="flex items-start justify-between">
                <div
                  className={cn(
                    "flex h-7 w-7 items-center justify-center rounded-[8px] transition-colors",
                    selected ? "bg-accent/15 text-accent" : "bg-fill text-secondary"
                  )}
                >
                  <Icon className="h-3.5 w-3.5" />
                </div>
                {selected && (
                  <span className="flex h-4 w-4 items-center justify-center rounded-full bg-accent">
                    <Check className="h-2.5 w-2.5 text-on-accent" strokeWidth={3.5} />
                  </span>
                )}
              </div>
              <div className="mt-2 text-[13px] font-semibold leading-5">{label}</div>
              <div className="mt-0.5 text-[11px] leading-4 text-secondary">{desc}</div>
            </button>
          );
        })}
      </div>

      {/* 封装格式 */}
      <div className="flex items-center justify-between">
        <label className="text-[13px] text-secondary">封装格式</label>
        <SegmentedControl
          value={containerFormat}
          onChange={(v) => setContainerFormat(v)}
          options={CONTAINERS}
        />
      </div>
    </div>
  );
}
