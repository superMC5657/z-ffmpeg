import { useState } from "react";
import { Check, Copy, FileDown, X } from "lucide-react";
import { save } from "@tauri-apps/plugin-dialog";
import { isTauriRuntime } from "@/lib/utils";
import { useToastStore } from "@/store/toastStore";
import { saveCommandToFile } from "@/lib/tauri";
import ProGate from "@/components/license/ProGate";

interface FfmpegCommandEntry {
  fileName: string;
  command: string;
}

interface FfmpegCommandDialogProps {
  entries: FfmpegCommandEntry[];
  onClose: () => void;
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
      useToastStore.getState().showToast(
        `保存失败: ${e instanceof Error ? e.message : String(e)}`,
        "error"
      );
    } finally {
      setSaving(false);
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-overlay backdrop-blur-[2px]"
      onClick={onClose}
    >
      <div
        className="animate-dialog-in w-full max-w-2xl rounded-[14px] border border-hairline bg-surface p-5 shadow-pop"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="mb-4 flex items-center justify-between">
          <h2 className="text-[15px] font-semibold">
            FFmpeg 命令{count > 1 ? `（${count} 条）` : ""}
          </h2>
          <button
            aria-label="关闭"
            onClick={onClose}
            className="flex h-7 w-7 items-center justify-center rounded-full text-tertiary transition-colors hover:bg-fill-strong hover:text-foreground"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

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

        <div className="mt-5 flex justify-end gap-2">
          <button
            onClick={onClose}
            className="h-9 rounded-[9px] bg-fill px-4 text-[13px] font-medium text-foreground transition-colors hover:bg-fill-strong active:scale-[0.98]"
          >
            关闭
          </button>
          <ProGate title="命令导出为文件是 Pro 功能，点击激活">
            <button
              onClick={handleSaveFile}
              disabled={!isTauriRuntime() || saving}
              className="flex h-9 items-center gap-1.5 rounded-[9px] bg-fill px-4 text-[13px] font-medium text-foreground transition-colors hover:bg-fill-strong active:scale-[0.98] disabled:cursor-default disabled:opacity-50 disabled:hover:bg-fill"
            >
              <FileDown className="h-3.5 w-3.5" />
              {saving ? "保存中..." : "保存为文件"}
            </button>
          </ProGate>
          <button
            onClick={handleCopy}
            className={`flex h-9 items-center gap-1.5 rounded-[9px] px-4 text-[13px] font-medium shadow-sm transition-all active:scale-[0.98] ${
              copied
                ? "bg-success/15 text-success"
                : "bg-accent text-on-accent hover:bg-accent-hover"
            }`}
          >
            {copied ? <Check className="h-3.5 w-3.5" /> : <Copy className="h-3.5 w-3.5" />}
            {copied ? "已复制" : "复制全部命令"}
          </button>
        </div>
      </div>
    </div>
  );
}
