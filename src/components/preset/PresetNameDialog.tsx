import { useState } from "react";
import { Save } from "lucide-react";
import Dialog from "@/components/layout/Dialog";
import AppleButton from "@/components/layout/AppleButton";
import AppleInput from "@/components/layout/AppleInput";
import { formatError } from "@/lib/utils";

export interface PresetNameDialogProps {
  mode: "save" | "import";
  /** 默认保存名(可留空,由用户输入) */
  defaultName?: string;
  onConfirm: (name: string) => Promise<void> | void;
  onClose: () => void;
  open?: boolean;
}

export default function PresetNameDialog({
  mode,
  defaultName = "",
  onConfirm,
  onClose,
  open = true,
}: PresetNameDialogProps) {
  const [name, setName] = useState(defaultName);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const isSave = mode === "save";

  const handleSave = async () => {
    if (!name.trim() || saving) return;
    setSaving(true);
    setError(null);
    try {
      await onConfirm(name.trim());
    } catch (e) {
      setError(formatError(e));
      setSaving(false);
    }
  };

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={isSave ? "保存为预设" : "导入预设"}
      footer={
        <>
          <AppleButton variant="secondary" onClick={onClose}>
            取消
          </AppleButton>
          <AppleButton
            variant="primary"
            loading={saving}
            disabled={!name.trim()}
            onClick={handleSave}
          >
            {isSave && <Save className="h-3.5 w-3.5" />}
            {saving ? (isSave ? "保存中..." : "导入中...") : isSave ? "保存" : "导入"}
          </AppleButton>
        </>
      }
    >
      <div className="space-y-3">
        <div>
          <label className="mb-1.5 block text-[12px] font-medium text-secondary">
            {isSave ? "预设名称" : "保存名"}
          </label>
          <AppleInput
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder={isSave ? "例如：H.264 高清存档" : "预设名称"}
            className="h-10 w-full text-[14px]"
            autoFocus
            onFocus={(e) => e.target.select()}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                handleSave();
              }
            }}
          />
        </div>
        <div className="rounded-[9px] bg-fill p-2.5">
          <p className="text-[12px] leading-5 text-secondary">
            {isSave
              ? "将一次性保存当前所有编码设置：视频编码、封装格式、码率控制、编码预设、分辨率/帧率等高级选项、音频设置与硬件加速，可在预设页面管理和复用。"
              : "默认使用导入文件的文件名（不含扩展名），可修改后保存。"}
          </p>
        </div>
        {error && (
          <div className="rounded-[9px] bg-destructive/10 p-2.5">
            <p className="text-[12px] leading-5 text-destructive">{error}</p>
          </div>
        )}
      </div>
    </Dialog>
  );
}
