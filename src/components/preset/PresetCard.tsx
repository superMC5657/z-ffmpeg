import { Trash2, Download, Check, Ban } from "lucide-react";
import type { HwAccelDevice, Preset } from "@/types";
import { usePresetStore } from "@/store/presetStore";
import { useSystemStore } from "@/store/systemStore";
import { useToastStore } from "@/store/toastStore";
import { useEncoderStore } from "@/store/encoderStore";

interface PresetCardProps {
  preset: Preset;
}

export default function PresetCard({ preset }: PresetCardProps) {
  const removePreset = usePresetStore((s) => s.removePreset);
  const selectPreset = usePresetStore((s) => s.selectPreset);
  const selectedPresetId = usePresetStore((s) => s.selectedPresetId);
  const isHwAccelAvailable = useSystemStore((s) => s.isHwAccelAvailable);

  const config = preset.config as unknown as Record<string, unknown>;
  const vs = config.videoSettings as Record<string, unknown> | undefined;
  const rc = vs?.rateControl as Record<string, unknown> | undefined;
  const hw = config.hwAccel as { device?: HwAccelDevice } | undefined;
  const hwUnavailable = !!hw?.device && !isHwAccelAvailable(hw.device);
  const isSelected = selectedPresetId === preset.id;

  const handleExport = async () => {
    const { save } = await import("@tauri-apps/plugin-dialog");
    const path = await save({
      defaultPath: `${preset.name}.json`,
      filters: [{ name: "JSON", extensions: ["json"] }],
    });
    if (!path) return; // 用户取消

    try {
      const { exportPresetToFile } = await import("@/lib/tauri");
      await exportPresetToFile(preset.id, path);
      useToastStore.getState().showToast(
        `已导出到 ${path}`,
        "success"
      );
    } catch (e) {
      useToastStore.getState().showToast(
        `导出失败: ${e instanceof Error ? e.message : String(e)}`,
        "error"
      );
    }
  };

  return (
    <div
      title={hwUnavailable ? `当前设备不支持 ${hw?.device} 硬件加速` : undefined}
      className={`group rounded-xl border p-3 shadow-sm transition-all ${
        hwUnavailable
          ? "cursor-not-allowed border-border/50 bg-card/40 opacity-55 grayscale"
          : isSelected
            ? "border-primary bg-primary/10 ring-1 ring-primary/40"
            : "border-border bg-card hover:border-primary/30 hover:shadow-md"
      }`}
    >
      <div className="flex items-start justify-between">
        <div className="min-w-0">
          <div className="flex items-center gap-1.5">
            <p className="truncate text-sm font-semibold">{preset.name}</p>
            {preset.isBuiltin && (
              <span className="shrink-0 rounded bg-accent px-1.5 py-0.5 text-[13px] text-muted-foreground">
                内置
              </span>
            )}
            {hwUnavailable && (
              <span className="shrink-0 rounded bg-destructive/10 px-1.5 py-0.5 text-[13px] text-destructive">
                设备不可用
              </span>
            )}
          </div>
          <p className="mt-1 truncate text-[13px] text-muted-foreground">
            {preset.description}
          </p>
        </div>
      </div>

      <div className="mt-2.5 flex flex-wrap gap-1">
        <span className="rounded bg-accent px-2 py-0.5 text-[13px] text-muted-foreground">
          {config.videoCodec as string || "?"}
        </span>
        {rc && (
          <span className="rounded bg-accent px-2 py-0.5 text-[13px] text-muted-foreground">
            {String(rc.type)} {String(rc.value)}
          </span>
        )}
        {hw?.device && (
          <span
            className={`rounded px-2 py-0.5 text-[13px] ${
              hwUnavailable
                ? "bg-destructive/10 text-destructive/70"
                : "bg-primary/10 text-primary"
            }`}
          >
            {String(hw.device)}
          </span>
        )}
        <span className="rounded bg-accent px-2 py-0.5 text-[13px] text-muted-foreground">
          {(vs?.encoderPreset as string) || "?"}
        </span>
      </div>

      {/* Actions */}
      <div className="mt-2.5 flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
        <button
          onClick={() => {
            if (isSelected) {
              selectPreset(null);
            } else {
              // 选中预设并把其配置应用到编码表单,确保“添加到队列”使用新设置
              selectPreset(preset.id);
              useEncoderStore.getState().applyConfig(preset.config);
              useToastStore.getState().showToast(
                `已应用预设「${preset.name}」`,
                "success"
              );
            }
          }}
          disabled={hwUnavailable}
          className={`flex items-center gap-1.5 rounded px-2.5 py-1.5 text-[14px] font-medium ${
            hwUnavailable
              ? "cursor-not-allowed text-muted-foreground/40"
              : isSelected
                ? "text-primary"
                : "text-muted-foreground hover:bg-accent"
          }`}
        >
          {hwUnavailable ? <Ban className="h-3.5 w-3.5" /> : <Check className="h-3.5 w-3.5" />}
          {hwUnavailable ? "不可用" : isSelected ? "已选中" : "选择"}
        </button>
        {!preset.isBuiltin && (
          <button
            onClick={() => removePreset(preset.id)}
            className="flex items-center gap-1.5 rounded px-2.5 py-1.5 text-[14px] font-medium text-muted-foreground hover:bg-destructive/10 hover:text-destructive"
          >
            <Trash2 className="h-3.5 w-3.5" />
            删除
          </button>
        )}
        <button
          onClick={handleExport}
          disabled={hwUnavailable}
          className={`flex items-center gap-1.5 rounded px-2.5 py-1.5 text-[14px] font-medium ${
            hwUnavailable
              ? "cursor-not-allowed text-muted-foreground/40"
              : "text-muted-foreground hover:bg-accent"
          }`}
        >
          <Download className="h-3.5 w-3.5" />
          导出
        </button>
      </div>
    </div>
  );
}
