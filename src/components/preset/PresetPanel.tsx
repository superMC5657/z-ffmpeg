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
      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
        {[0, 1, 2].map((i) => (
          <div
            key={i}
            className="h-32 animate-pulse rounded-[14px] border border-hairline bg-fill/50"
          />
        ))}
      </div>
    );
  }

  if (presets.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center rounded-[14px] border border-dashed border-hairline py-16">
        <div className="flex h-11 w-11 items-center justify-center rounded-[12px] bg-fill">
          <SlidersHorizontal className="h-5 w-5 text-tertiary" />
        </div>
        <p className="mt-3 text-[13px] font-medium">暂无预设</p>
        <p className="mt-0.5 text-[12px] text-secondary">
          导入预设 JSON，或在编码页保存当前配置为预设
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
