import { Folder, FolderOpen, X } from "lucide-react";
import { useEncoderStore } from "@/store/encoderStore";
import { useToastStore } from "@/store/toastStore";
import { isTauriRuntime } from "@/lib/utils";

export default function OutputDirSelector() {
  const outputDir = useEncoderStore((s) => s.outputDir);
  const setOutputDir = useEncoderStore((s) => s.setOutputDir);

  const handleSelect = async () => {
    if (!isTauriRuntime()) return;
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({ directory: true, multiple: false });
      if (typeof selected === "string" && selected) {
        setOutputDir(selected);
        useToastStore.getState().showToast("输出目录已设置", "success");
      }
    } catch {
      useToastStore.getState().showToast("无法打开目录选择器", "error");
    }
  };

  const handleClear = () => {
    setOutputDir("");
    useToastStore.getState().showToast("已恢复默认输出到源文件目录", "info");
  };

  return (
    <div className="rounded-xl border border-border bg-card p-5 shadow-sm">
      <div className="mb-3 flex items-center justify-between">
        <h2 className="text-[15px] font-semibold">输出设置</h2>
        {outputDir && (
          <button
            onClick={handleClear}
            className="flex items-center gap-1.5 text-[14px] font-medium text-muted-foreground transition-colors hover:text-foreground"
          >
            <X className="h-3.5 w-3.5" />
            恢复默认
          </button>
        )}
      </div>

      <div className="flex items-center gap-3">
        <button
          onClick={handleSelect}
          disabled={!isTauriRuntime()}
          className="flex shrink-0 items-center gap-2 rounded-lg border border-border bg-accent/60 px-4 py-2.5 text-[14px] font-medium transition-all hover:border-primary/40 hover:text-foreground disabled:cursor-not-allowed disabled:opacity-50"
        >
          <FolderOpen className="h-4 w-4" />
          选择目录
        </button>
        <div className="flex min-w-0 flex-1 items-center gap-2 rounded-lg border border-border bg-accent/40 px-3.5 py-2.5">
          <Folder className="h-4 w-4 shrink-0 text-muted-foreground" />
          <span
            className={`truncate text-[13px] ${
              outputDir ? "text-foreground" : "text-muted-foreground"
            }`}
            title={outputDir || undefined}
          >
            {outputDir || "默认：输出到源文件所在目录（文件名_encoded）"}
          </span>
        </div>
      </div>
    </div>
  );
}
