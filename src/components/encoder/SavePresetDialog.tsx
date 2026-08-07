import { useState } from "react";
import { Save, X } from "lucide-react";

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
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
      onClick={onClose}
    >
      <div
        className="w-full max-w-md rounded-xl border border-border bg-card p-5 shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="mb-4 flex items-center justify-between">
          <h2 className="text-base font-bold">保存为预设</h2>
          <button
            onClick={onClose}
            className="rounded-md p-1 text-muted-foreground hover:bg-accent hover:text-foreground"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="space-y-3">
          <div>
            <label className="mb-1 block text-[13px] font-medium text-muted-foreground">
              预设名称
            </label>
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="例如：H.264 高清存档"
              className="w-full rounded-md border border-border bg-accent px-3 py-2 text-sm focus:border-primary focus:outline-none"
              autoFocus
              onFocus={(e) => e.target.select()}
            />
          </div>
          <div className="rounded-md bg-accent/50 p-2.5">
            <p className="text-[13px] text-muted-foreground">
              将一次性保存当前所有编码设置：视频编码、封装格式、码率控制、编码预设、分辨率/帧率等高级选项、音频设置与硬件加速，可在预设页面管理和复用。
            </p>
          </div>
          {error && (
            <div className="rounded-md border border-destructive/40 bg-destructive/10 p-2.5">
              <p className="text-[13px] text-destructive">{error}</p>
            </div>
          )}
        </div>

        <div className="mt-4 flex justify-end gap-2">
          <button
            onClick={onClose}
            className="rounded-md px-5 py-2.5 text-[14px] font-medium text-muted-foreground hover:bg-accent hover:text-foreground"
          >
            取消
          </button>
          <button
            onClick={handleSave}
            disabled={!name.trim() || saving}
            className={`flex items-center gap-1.5 rounded-md px-5 py-2.5 text-[14px] font-medium ${
              name.trim() && !saving
                ? "bg-gradient-brand text-white shadow-md shadow-primary/25 hover:brightness-110"
                : "cursor-not-allowed bg-accent text-muted-foreground"
            }`}
          >
            <Save className="h-4 w-4" />
            {saving ? "保存中..." : "保存"}
          </button>
        </div>
      </div>
    </div>
  );
}
