import { FolderOpen, RotateCcw } from "lucide-react";
import { useEncoderStore } from "@/store/encoderStore";
import { useToastStore } from "@/store/toastStore";
import { isTauriRuntime } from "@/lib/utils";

/** 输出目录选择（卡片外壳由父级 Card 提供） */
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
    <div className="flex flex-wrap items-center gap-2.5">
      <button
        onClick={handleSelect}
        disabled={!isTauriRuntime()}
        className="flex h-9 shrink-0 items-center gap-1.5 rounded-lg bg-fill px-3.5 text-[13px] font-medium text-foreground transition-colors hover:bg-fill-strong disabled:opacity-50"
      >
        <FolderOpen className="h-4 w-4 text-secondary" />
        选择目录
      </button>

      <div className="flex h-9 min-w-0 flex-1 items-center gap-2 rounded-lg bg-fill/60 px-3">
        <span
          className={`truncate text-[12px] ${
            outputDir ? "text-foreground" : "text-tertiary"
          }`}
          title={outputDir || undefined}
        >
          {outputDir || "默认：输出到源文件所在目录（文件名_encoded）"}
        </span>
        {outputDir && (
          <button
            onClick={handleClear}
            title="恢复默认"
            className="ml-auto flex h-6 shrink-0 items-center gap-1 rounded-md px-1.5 text-[11px] text-secondary transition-colors hover:bg-fill-strong hover:text-foreground"
          >
            <RotateCcw className="h-3 w-3" />
            恢复默认
          </button>
        )}
      </div>
    </div>
  );
}
