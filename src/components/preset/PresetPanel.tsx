import { useEffect } from "react";
import { SlidersHorizontal } from "lucide-react";
import { usePresetStore } from "@/store/presetStore";
import { useSystemStore } from "@/store/systemStore";
import PresetCard from "./PresetCard";

export default function PresetPanel() {
  const presets = usePresetStore((s) => s.presets);
  const fetchPresets = usePresetStore((s) => s.fetchPresets);
  const isLoading = usePresetStore((s) => s.isLoading);
  const fetchHwAccels = useSystemStore((s) => s.fetchHwAccels);

  useEffect(() => {
    fetchPresets();
    fetchHwAccels();
  }, []);

  if (isLoading) {
    return (
      <div className="py-16 text-center text-sm text-muted-foreground">加载中...</div>
    );
  }

  if (presets.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center rounded-xl border border-dashed border-border bg-card/40 py-20">
        <div className="mb-3 flex h-14 w-14 items-center justify-center rounded-2xl bg-accent">
          <SlidersHorizontal className="h-7 w-7 text-muted-foreground/50" />
        </div>
        <p className="text-sm font-medium text-foreground/80">暂无预设</p>
        <p className="mt-1 text-[13px] text-muted-foreground">
          导入预设 JSON 以快速复用编码配置
        </p>
      </div>
    );
  }

  return (
    <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
      {presets.map((preset) => (
        <PresetCard key={preset.id} preset={preset} />
      ))}
    </div>
  );
}
