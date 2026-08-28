import { useEffect, useMemo } from "react";
import type { HwAccelDevice } from "@/types";
import { usePresetStore } from "@/store/presetStore";
import { useSystemStore } from "@/store/systemStore";
import { useEncoderStore } from "@/store/encoderStore";
import AppleSelect from "@/components/layout/AppleSelect";

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
    if (!presetId) return;
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
    <AppleSelect
      className="w-48"
      value={selectedPresetId || ""}
      onChange={(e) => applyPreset(e.target.value)}
      aria-label="应用预设"
    >
      <option value="">选择预设…</option>
      {usablePresets.map((p) => (
        <option key={p.id} value={p.id}>
          {p.name}
          {p.isBuiltin ? "（内置）" : ""}
        </option>
      ))}
    </AppleSelect>
  );
}
