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
  }, [fetchPresets, fetchHwAccels]);

  const applyPreset = (presetId: string) => {
    if (!presetId) return;
    const preset = presets.find((p) => p.id === presetId);
    if (!preset) return;
    usePresetStore.getState().selectPreset(presetId);
    useEncoderStore.getState().applyConfig(preset.config);
  };

  // 当前设备不支持的硬件加速预设（或显卡不支持的特定编码格式，如 RTX 30 系列不支持 AV1 硬编）不出现在下拉中
  const usablePresets = useMemo(
    () =>
      presets.filter((p) => {
        const config = p.config as unknown as {
          videoCodec?: string;
          hwAccel?: { device?: HwAccelDevice } | null;
        };
        const hw = config.hwAccel;
        if (!hw?.device) return true; // 软件编码
        const found = hwAccels.find((h) => h.device === hw.device);
        if (!found || !found.available) return false;

        // 校验该硬件是否支持该预设指定的编码格式
        const codec = config.videoCodec;
        if (codec && found.supportedCodecs) {
          const codecL = codec.toLowerCase();
          return found.supportedCodecs.some((c) => {
            const cl = c.codec.toLowerCase();
            if (codecL === "h264") return cl === "h264";
            if (codecL === "h265") return cl === "hevc" || cl === "h265";
            if (codecL === "av1") return cl === "av1";
            if (codecL === "vp9") return cl === "vp9";
            return false;
          });
        }
        return true;
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

  const builtinPresets = useMemo(
    () => usablePresets.filter((p) => p.isBuiltin),
    [usablePresets]
  );
  const customPresets = useMemo(
    () => usablePresets.filter((p) => !p.isBuiltin),
    [usablePresets]
  );

  return (
    <AppleSelect
      className="w-52"
      value={selectedPresetId || ""}
      onChange={(e) => applyPreset(e.target.value)}
      aria-label="应用预设"
    >
      <option value="">选择预设…</option>
      {builtinPresets.length > 0 && (
        <optgroup label="内置推荐预设">
          {builtinPresets.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </optgroup>
      )}
      {customPresets.length > 0 && (
        <optgroup label="自定义预设">
          {customPresets.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </optgroup>
      )}
    </AppleSelect>
  );
}
