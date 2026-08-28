import { useState } from "react";
import { Save, X } from "lucide-react";
import AppleInput from "@/components/layout/AppleInput";

interface SavePresetDialogProps {
  /** 默认保存名(可留空,由用户输入) */
  defaultName?: string;
  onConfirm: (name: string) => Promise<void> | void;
  onClose: () => void;
}

export default function SavePresetDialog({
  defaultName = "",
  onConfirm,
  onClose,
}: SavePresetDialogProps) {
  const [name, setName] = useState(defaultName);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSave = async () => {
    if (!name.trim() || saving) return;
    setSaving(true);
    setError(null);
    try {
      await onConfirm(name.trim());
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setSaving(false);
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-overlay backdrop-blur-[2px]"
      onClick={onClose}
    >
      <div
        className="animate-dialog-in w-full max-w-md rounded-[14px] border border-hairline bg-surface p-5 shadow-pop"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="mb-4 flex items-center justify-between">
          <h2 className="text-[15px] font-semibold">保存为预设</h2>
          <button
            aria-label="关闭"
            onClick={onClose}
            className="flex h-7 w-7 items-center justify-center rounded-full text-tertiary transition-colors hover:bg-fill-strong hover:text-foreground"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="space-y-3">
          <div>
            <label className="mb-1.5 block text-[12px] font-medium text-secondary">
              预设名称
            </label>
            <AppleInput
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="例如：H.264 高清存档"
              className="h-10 w-full text-[14px]"
              autoFocus
              onFocus={(e) => e.target.select()}
            />
          </div>
          <div className="rounded-[9px] bg-fill p-2.5">
            <p className="text-[12px] leading-5 text-secondary">
              将一次性保存当前所有编码设置：视频编码、封装格式、码率控制、编码预设、分辨率/帧率等高级选项、音频设置与硬件加速，可在预设页面管理和复用。
            </p>
          </div>
          {error && (
            <div className="rounded-[9px] bg-destructive/10 p-2.5">
              <p className="text-[12px] leading-5 text-destructive">{error}</p>
            </div>
          )}
        </div>

        <div className="mt-5 flex justify-end gap-2">
          <button
            onClick={onClose}
            className="h-9 rounded-[9px] bg-fill px-4 text-[13px] font-medium text-foreground transition-colors hover:bg-fill-strong active:scale-[0.98]"
          >
            取消
          </button>
          <button
            onClick={handleSave}
            disabled={!name.trim() || saving}
            className={`flex h-9 items-center gap-1.5 rounded-[9px] px-4 text-[13px] font-medium shadow-sm transition-all active:scale-[0.98] disabled:cursor-default disabled:opacity-50 ${
              name.trim() && !saving
                ? "bg-accent text-on-accent hover:bg-accent-hover"
                : "bg-fill text-tertiary"
            }`}
          >
            <Save className="h-3.5 w-3.5" />
            {saving ? "保存中..." : "保存"}
          </button>
        </div>
      </div>
    </div>
  );
}
