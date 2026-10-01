import { useEffect } from "react";
import { SlidersHorizontal } from "lucide-react";
import { usePresetStore } from "@/store/presetStore";
import { useSystemStore } from "@/store/systemStore";
import EmptyState from "@/components/layout/EmptyState";
import { SkeletonCard } from "@/components/layout/Skeleton";
import PresetCard from "./PresetCard";

export default function PresetPanel() {
  const presets = usePresetStore((s) => s.presets);
  const fetchPresets = usePresetStore((s) => s.fetchPresets);
  const isLoading = usePresetStore((s) => s.isLoading);
  const fetchHwAccels = useSystemStore((s) => s.fetchHwAccels);

  useEffect(() => {
    fetchPresets();
    fetchHwAccels();
  }, [fetchPresets, fetchHwAccels]);

  if (isLoading) {
    return <SkeletonCard />;
  }

  if (presets.length === 0) {
    return (
      <EmptyState
        icon={SlidersHorizontal}
        title="暂无预设"
        description="导入预设 JSON，或在编码页保存当前配置为预设"
      />
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
