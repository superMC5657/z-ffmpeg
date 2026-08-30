import { Trash2, Download, Check, Ban } from "lucide-react";
import type { HwAccelDevice, Preset } from "@/types";
import { usePresetStore } from "@/store/presetStore";
import { useSystemStore } from "@/store/systemStore";
import { useToastStore } from "@/store/toastStore";
import { useEncoderStore } from "@/store/encoderStore";
import ProGate from "@/components/license/ProGate";
import { cn } from "@/lib/utils";

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

  const chips = [
    String(config.videoCodec ?? "?"),
    rc ? `${String(rc.type)} ${String(rc.value)}` : null,
    (vs?.encoderPreset as string) || null,
  ].filter(Boolean) as string[];

  return (
    <div
      title={hwUnavailable ? `当前设备不支持 ${hw?.device} 硬件加速` : undefined}
      className={cn(
        "group rounded-[14px] border p-4 shadow-card transition-all",
        hwUnavailable
          ? "cursor-not-allowed border-hairline bg-surface opacity-55"
          : isSelected
            ? "border-accent bg-accent/[0.04]"
            : "border-hairline bg-surface hover:shadow-pop"
      )}
    >
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <div className="flex items-center gap-1.5">
            <p className="truncate text-[13px] font-semibold leading-5">{preset.name}</p>
            {preset.isBuiltin && (
              <span className="shrink-0 rounded-md bg-fill px-1.5 py-0.5 text-[10px] font-medium leading-4 text-secondary">
                内置
              </span>
            )}
            {hwUnavailable && (
              <span className="shrink-0 rounded-md bg-destructive/10 px-1.5 py-0.5 text-[10px] font-medium leading-4 text-destructive">
                设备不可用
              </span>
            )}
          </div>
          <p className="mt-1 line-clamp-2 text-[12px] leading-[18px] text-secondary">
            {preset.description}
          </p>
        </div>
        {isSelected && (
          <span className="flex h-4.5 w-4.5 shrink-0 items-center justify-center rounded-full bg-accent">
            <Check className="h-2.5 w-2.5 text-on-accent" strokeWidth={3.5} />
          </span>
        )}
      </div>

      <div className="mt-2.5 flex flex-wrap gap-1">
        {chips.map((chip) => (
          <span
            key={chip}
            className="rounded-md bg-fill px-1.5 py-0.5 text-[11px] font-medium leading-4 text-secondary"
          >
            {chip}
          </span>
        ))}
        {hw?.device && (
          <span
            className={cn(
              "rounded-md px-1.5 py-0.5 text-[11px] font-medium leading-4",
              hwUnavailable
                ? "bg-destructive/10 text-destructive/80"
                : "bg-accent/10 text-accent"
            )}
          >
            {String(hw.device)}
          </span>
        )}
      </div>

      {/* Actions（hover 显示） */}
      <div className="mt-3 flex items-center gap-0.5 border-t border-hairline pt-2.5 opacity-0 transition-opacity group-hover:opacity-100">
        <button
          onClick={() => {
            if (isSelected) {
              selectPreset(null);
            } else {
              // 选中预设并把其配置应用到编码表单,确保"添加到队列"使用新设置
              selectPreset(preset.id);
              useEncoderStore.getState().applyConfig(preset.config);
              useToastStore.getState().showToast(
                `已应用预设「${preset.name}」`,
                "success"
              );
            }
          }}
          disabled={hwUnavailable}
          className={cn(
            "flex items-center gap-1 rounded-md px-2 py-1 text-[12px] font-medium transition-colors",
            hwUnavailable
              ? "cursor-default text-tertiary"
              : isSelected
                ? "text-accent"
                : "text-secondary hover:bg-fill-strong hover:text-foreground"
          )}
        >
          {hwUnavailable ? <Ban className="h-3 w-3" /> : <Check className="h-3 w-3" />}
          {hwUnavailable ? "不可用" : isSelected ? "已选中" : "选择"}
        </button>
        {!preset.isBuiltin && (
          <button
            onClick={() => removePreset(preset.id)}
            className="flex items-center gap-1 rounded-md px-2 py-1 text-[12px] font-medium text-secondary transition-colors hover:bg-destructive/10 hover:text-destructive"
          >
            <Trash2 className="h-3 w-3" />
            删除
          </button>
        )}
        <ProGate title="预设导出为 Pro 功能，点击激活">
          <button
            onClick={handleExport}
            disabled={hwUnavailable}
            className={cn(
              "flex items-center gap-1 rounded-md px-2 py-1 text-[12px] font-medium transition-colors",
              hwUnavailable
                ? "cursor-default text-tertiary"
                : "ml-auto text-secondary hover:bg-fill-strong hover:text-foreground"
            )}
          >
            <Download className="h-3 w-3" />
            导出
          </button>
        </ProGate>
      </div>
    </div>
  );
}
