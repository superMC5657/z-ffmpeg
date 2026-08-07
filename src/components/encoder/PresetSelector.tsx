import { useEffect, useMemo } from "react";
import { Bookmark } from "lucide-react";
import type { HwAccelDevice } from "@/types";
import { usePresetStore } from "@/store/presetStore";
import { useSystemStore } from "@/store/systemStore";
import { useEncoderStore } from "@/store/encoderStore";

export default function PresetSelector() {
  const presets = usePresetStore((s) => s.presets);
  const fetchPresets = usePresetStore((s) => s.fetchPresets);
  const selectedPresetId = usePresetStore((s) => s.selectedPresetId);
  const hwAccels = useSystemStore((s) => s.hwAccels);
  const fetchHwAccels = useSystemStore((s) => s.fetchHwAccels);

  useEffect(() => {
    fetchPresets();
    fetchHwAccels();
  }, []);

  const applyPreset = (presetId: string) => {
    const preset = presets.find((p) => p.id === presetId);
    if (!preset) return;
    usePresetStore.getState().selectPreset(presetId);
    useEncoderStore.getState().applyConfig(preset.config);
  };

  // 当前设备不支持的硬件加速预设不出现在下拉中
  const usablePresets = useMemo(
    () =>
      presets.filter((p) => {
        const hw = (p.config as unknown as { hwAccel?: { device?: HwAccelDevice } | null }).hwAccel;
        if (!hw?.device) return true; // 软件编码
        const found = hwAccels.find((h) => h.device === hw.device);
        return found ? found.available : false;
      }),
    [presets, hwAccels]
  );

  // 若当前选中预设因硬件不可用被过滤,清除选中态
  useEffect(() => {
    if (
      selectedPresetId &&
      !usablePresets.some((p) => p.id === selectedPresetId)
    ) {
      usePresetStore.getState().selectPreset(null);
    }
  }, [selectedPresetId, usablePresets]);

  return (
    <div className="flex items-center gap-2">
      <Bookmark className="h-4 w-4 text-muted-foreground" />
      <select
        value={selectedPresetId || ""}
        onChange={(e) => applyPreset(e.target.value)}
        className="rounded-md border border-border bg-accent px-3 py-2 text-[14px] focus:border-primary focus:outline-none"
      >
        <option value="">选择预设...</option>
        {usablePresets.map((p) => (
          <option key={p.id} value={p.id}>
            {p.isBuiltin ? "📦" : "💾"} {p.name}
          </option>
        ))}
      </select>
    </div>
  );
}
