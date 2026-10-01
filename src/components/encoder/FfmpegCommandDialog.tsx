import { useState } from "react";
import { Check, Copy, FileDown } from "lucide-react";
import { save } from "@tauri-apps/plugin-dialog";
import { isTauriRuntime } from "@/lib/utils";
import { useToastStore, showErrorToast } from "@/store/toastStore";
import { saveCommandToFile } from "@/lib/tauri";
import ProGate from "@/components/license/ProGate";
import Dialog from "@/components/layout/Dialog";
import AppleButton from "@/components/layout/AppleButton";
import { STATUS_TONE } from "@/components/layout/StatusBadge";

interface FfmpegCommandEntry {
  fileName: string;
  command: string;
}

interface FfmpegCommandDialogProps {
  entries: FfmpegCommandEntry[];
  onClose: () => void;
  open?: boolean;
}

/** 拼接全部命令为可复制/可保存的多行文本(每条前带文件名注释) */
function buildCombinedText(entries: FfmpegCommandEntry[]): string {
  return entries
    .map((e, i) => {
      const header = `# [${i + 1}/${entries.length}] ${e.fileName}`;
      return `${header}\n${e.command}`;
    })
    .join("\n\n");
}

export default function FfmpegCommandDialog({
  entries,
  onClose,
  open = true,
}: FfmpegCommandDialogProps) {
  const [copied, setCopied] = useState(false);
  const [saving, setSaving] = useState(false);
  const combined = buildCombinedText(entries);
  const count = entries.length;

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(combined);
      setCopied(true);
      useToastStore.getState().showToast(
        `${count} 条命令已复制到剪贴板`,
        "success"
      );
      setTimeout(() => setCopied(false), 2000);
    } catch {
      useToastStore.getState().showToast("复制失败，请手动选择文本复制", "error");
    }
  };

  const handleSaveFile = async () => {
    if (!isTauriRuntime()) return;
    try {
      const path = await save({
        defaultPath: "ffmpeg_commands.txt",
        filters: [{ name: "文本文件", extensions: ["txt", "bat", "sh"] }],
      });
      if (!path) return; // 用户取消
      setSaving(true);
      await saveCommandToFile(combined, path);
      useToastStore.getState().showToast(`命令已保存到 ${path}`, "success");
    } catch (e) {
      showErrorToast("保存失败", e);
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog
      open={open}
      onClose={onClose}
      wide
      title={`FFmpeg 命令${count > 1 ? `（${count} 条）` : ""}`}
      footer={
        <>
          <AppleButton variant="secondary" onClick={onClose}>
            关闭
          </AppleButton>
          <ProGate title="命令导出为文件是 Pro 功能，点击激活">
            <AppleButton
              variant="secondary"
              loading={saving}
              disabled={!isTauriRuntime()}
              onClick={handleSaveFile}
            >
              <FileDown className="h-3.5 w-3.5" />
              {saving ? "保存中..." : "保存为文件"}
            </AppleButton>
          </ProGate>
          <AppleButton
            variant="primary"
            onClick={handleCopy}
            className={copied ? STATUS_TONE.success : undefined}
          >
            {copied ? <Check className="h-3.5 w-3.5" /> : <Copy className="h-3.5 w-3.5" />}
            {copied ? "已复制" : "复制全部命令"}
          </AppleButton>
        </>
      }
    >
      <div className="max-h-80 space-y-3.5 overflow-auto rounded-[9px] bg-fill p-3.5">
        {entries.map((e, i) => (
          <div key={i} className="space-y-1.5">
            <div className="flex items-center gap-2">
              <span className="shrink-0 rounded-md bg-accent/12 px-1.5 py-0.5 text-[11px] font-medium tabular-nums text-accent">
                {i + 1}/{count}
              </span>
              <span className="truncate text-[12px] font-medium text-secondary">
                {e.fileName}
              </span>
            </div>
            <pre className="whitespace-pre-wrap break-all font-mono text-[12px] leading-relaxed text-foreground">
              {e.command}
            </pre>
          </div>
        ))}
      </div>
    </Dialog>
  );
}
