import type { VideoCodec } from "@/types";
import { useEncoderStore } from "@/store/encoderStore";
import { Clapperboard, Film, Globe, Sparkles } from "lucide-react";
import { cn } from "@/lib/utils";

const CODECS: {
  value: VideoCodec;
  label: string;
  desc: string;
  icon: typeof Film;
  iconCls: string;
  activeCls: string;
}[] = [
  {
    value: "H264",
    label: "H.264",
    desc: "兼容性最好，通用选择",
    icon: Film,
    iconCls: "text-sky-400 bg-sky-500/10",
    activeCls: "border-sky-500/60 bg-sky-500/10 ring-1 ring-sky-500/40",
  },
  {
    value: "H265",
    label: "H.265 / HEVC",
    desc: "更高压缩率，文件更小",
    icon: Clapperboard,
    iconCls: "text-violet-400 bg-violet-500/10",
    activeCls: "border-violet-500/60 bg-violet-500/10 ring-1 ring-violet-500/40",
  },
  {
    value: "AV1",
    label: "AV1",
    desc: "最新编码，最优压缩率",
    icon: Sparkles,
    iconCls: "text-rose-400 bg-rose-500/10",
    activeCls: "border-rose-500/60 bg-rose-500/10 ring-1 ring-rose-500/40",
  },
  {
    value: "VP9",
    label: "VP9",
    desc: "Web 优化，开源免专利",
    icon: Globe,
    iconCls: "text-emerald-400 bg-emerald-500/10",
    activeCls: "border-emerald-500/60 bg-emerald-500/10 ring-1 ring-emerald-500/40",
  },
];

const CONTAINERS: { value: string; label: string }[] = [
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
      {/* Video Codec Tabs */}
      <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
        {CODECS.map(({ value, label, desc, icon: Icon, iconCls, activeCls }) => (
          <button
            key={value}
            onClick={() => setVideoCodec(value)}
            className={cn(
              "group rounded-lg border p-3 text-left transition-all duration-150",
              videoCodec === value
                ? activeCls
                : "border-border hover:-translate-y-0.5 hover:border-muted-foreground/40"
            )}
          >
            <div
              className={cn(
                "mb-2 flex h-8 w-8 items-center justify-center rounded-md",
                iconCls
              )}
            >
              <Icon className="h-4 w-4" />
            </div>
            <div className="text-sm font-semibold">{label}</div>
            <div className="mt-0.5 text-[13px] text-muted-foreground">{desc}</div>
          </button>
        ))}
      </div>

      {/* Container Format */}
      <div className="flex items-center gap-3">
        <label className="text-[13px] font-medium text-muted-foreground">
          封装格式
        </label>
        <div className="flex gap-1 rounded-lg bg-accent p-0.5">
          {CONTAINERS.map(({ value, label }) => (
            <button
              key={value}
              onClick={() => setContainerFormat(value as typeof containerFormat)}
              className={cn(
                "rounded-md px-3.5 py-1.5 text-[14px] font-medium transition-all",
                containerFormat === value
                  ? "bg-gradient-brand text-white shadow-sm"
                  : "text-muted-foreground hover:text-foreground"
              )}
            >
              {label}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
