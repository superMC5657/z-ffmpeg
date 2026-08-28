import { useState } from "react";
import { X } from "lucide-react";
import AppleInput from "@/components/layout/AppleInput";

interface ImportPresetDialogProps {
  /** 默认保存名:导入文件的文件名去掉扩展名 */
  defaultName: string;
  onConfirm: (name: string) => Promise<void> | void;
  onClose: () => void;
}

export default function ImportPresetDialog({
  defaultName,
  onConfirm,
  onClose,
}: ImportPresetDialogProps) {
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
          <h2 className="text-[15px] font-semibold">导入预设</h2>
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
              保存名
            </label>
            <AppleInput
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="预设名称"
              className="h-10 w-full text-[14px]"
              autoFocus
              onFocus={(e) => e.target.select()}
            />
          </div>
          <div className="rounded-[9px] bg-fill p-2.5">
            <p className="text-[12px] leading-5 text-secondary">
              默认使用导入文件的文件名（不含扩展名），可修改后保存。
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
            className={`h-9 rounded-[9px] px-4 text-[13px] font-medium shadow-sm transition-all active:scale-[0.98] disabled:cursor-default disabled:opacity-50 ${
              name.trim() && !saving
                ? "bg-accent text-on-accent hover:bg-accent-hover"
                : "bg-fill text-tertiary"
            }`}
          >
            {saving ? "导入中..." : "导入"}
          </button>
        </div>
      </div>
    </div>
  );
}
