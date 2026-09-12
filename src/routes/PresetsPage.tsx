import { useState } from "react";
import { Upload } from "lucide-react";
import PresetPanel from "@/components/preset/PresetPanel";
import ImportPresetDialog from "@/components/preset/ImportPresetDialog";
import PageHeader from "@/components/layout/PageHeader";
import { usePresetStore } from "@/store/presetStore";

interface PendingImport {
  content: string;
  defaultName: string;
}

/** 从路径中提取文件名并去掉扩展名（默认保存名） */
function defaultNameFromPath(path: string): string {
  const fileName = path.split(/[\\/]/).pop() ?? "";
  const stem = fileName.replace(/\.[^.]+$/, "");
  return stem || fileName;
}

export default function PresetsPage() {
  const importPreset = usePresetStore((s) => s.importPreset);
  const [pendingImport, setPendingImport] = useState<PendingImport | null>(null);

  const handleImport = async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const result = await open({
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (!result) return;
      const path = typeof result === "string" ? result : (result as { path: string }).path;
      const { readTextFile } = await import("@tauri-apps/plugin-fs");
      const content = await readTextFile(path);
      setPendingImport({ content, defaultName: defaultNameFromPath(path) });
    } catch {
      // Fallback to prompt
      const text = prompt("粘贴预设 JSON：");
      if (text) setPendingImport({ content: text, defaultName: "导入的预设" });
    }
  };

  const handleConfirmImport = async (name: string) => {
    if (!pendingImport) return;
    await importPreset(pendingImport.content, name);
    setPendingImport(null);
  };

  return (
    <div>
      <PageHeader
        title="编码预设"
        description="保存和管理常用的编码配置，一键切换参数"
        action={
          <button
            onClick={handleImport}
            className="flex h-9 items-center gap-1.5 rounded-[9px] bg-accent px-4 text-[13px] font-medium text-on-accent shadow-sm transition-all hover:bg-accent-hover active:scale-[0.98]"
          >
            <Upload className="h-3.5 w-3.5" />
            导入预设
          </button>
        }
      />
      <PresetPanel />

      {pendingImport && (
        <ImportPresetDialog
          defaultName={pendingImport.defaultName}
          onConfirm={handleConfirmImport}
          onClose={() => setPendingImport(null)}
        />
      )}
    </div>
  );
}
