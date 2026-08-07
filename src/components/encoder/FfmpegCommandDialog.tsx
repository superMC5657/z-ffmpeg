import { useState } from "react";
import { Check, Copy, FileDown, X } from "lucide-react";
import { save } from "@tauri-apps/plugin-dialog";
import { isTauriRuntime } from "@/lib/utils";
import { useToastStore } from "@/store/toastStore";
import { saveCommandToFile } from "@/lib/tauri";

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
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
      onClick={onClose}
    >
      <div
        className="w-full max-w-2xl rounded-xl border border-border bg-card p-5 shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="mb-4 flex items-center justify-between">
          <h2 className="text-base font-bold">
            FFmpeg 命令{count > 1 ? `（${count} 条）` : ""}
          </h2>
          <button
            onClick={onClose}
            className="rounded-md p-1 text-muted-foreground hover:bg-accent hover:text-foreground"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="max-h-80 space-y-3 overflow-auto rounded-lg border border-border bg-background/60 p-3.5">
          {entries.map((e, i) => (
            <div key={i} className="space-y-1">
              <div className="flex items-center gap-2">
                <span className="shrink-0 rounded bg-primary/10 px-1.5 py-0.5 text-[11px] font-medium text-primary">
                  {i + 1}/{count}
                </span>
                <span className="truncate text-xs font-medium text-muted-foreground">
                  {e.fileName}
                </span>
              </div>
              <pre className="whitespace-pre-wrap break-all font-mono text-[13px] leading-relaxed text-foreground/90">
                {e.command}
              </pre>
            </div>
          ))}
        </div>

        <div className="mt-4 flex justify-end gap-2">
          <button
            onClick={onClose}
            className="rounded-md px-5 py-2.5 text-[14px] font-medium text-muted-foreground hover:bg-accent hover:text-foreground"
          >
            关闭
          </button>
          <button
            onClick={handleSaveFile}
            disabled={!isTauriRuntime() || saving}
            className="flex items-center gap-1.5 rounded-md border border-border bg-accent/60 px-5 py-2.5 text-[14px] font-medium transition-all hover:border-primary/40 hover:text-foreground disabled:cursor-not-allowed disabled:opacity-50"
          >
            <FileDown className="h-4 w-4" />
            {saving ? "保存中..." : "保存为文件"}
          </button>
          <button
            onClick={handleCopy}
            className={`flex items-center gap-1.5 rounded-md px-5 py-2.5 text-[14px] font-medium transition-all ${
              copied
                ? "bg-success/20 text-success"
                : "bg-gradient-brand text-white shadow-md shadow-primary/25 hover:brightness-110"
            }`}
          >
            {copied ? <Check className="h-4 w-4" /> : <Copy className="h-4 w-4" />}
            {copied ? "已复制" : "复制全部命令"}
          </button>
        </div>
      </div>
    </div>
  );
}
