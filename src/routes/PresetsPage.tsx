import { useState } from "react";
import { Upload } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { readTextFile } from "@tauri-apps/plugin-fs";
import PresetPanel from "@/components/preset/PresetPanel";
import PresetNameDialog from "@/components/preset/PresetNameDialog";
import PageHeader from "@/components/layout/PageHeader";
import AppleButton from "@/components/layout/AppleButton";
import { usePresetStore } from "@/store/presetStore";
import { getFileName } from "@/lib/utils";

interface PendingImport {
  content: string;
  defaultName: string;
}

/** 从路径中提取文件名并去掉扩展名（默认保存名） */
function defaultNameFromPath(path: string): string {
  const fileName = getFileName(path);
  const stem = fileName.replace(/\.[^.]+$/, "");
  return stem || fileName;
}

export default function PresetsPage() {
  const importPreset = usePresetStore((s) => s.importPreset);
  const [pendingImport, setPendingImport] = useState<PendingImport | null>(null);

  const handleImport = async () => {
    try {
      const result = await open({
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (!result) return;
      const path = typeof result === "string" ? result : (result as { path: string }).path;
      const content = await readTextFile(path);
      setPendingImport({ content, defaultName: defaultNameFromPath(path) });
    } catch {
      // 降级为浏览器 prompt 输入框
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
          <AppleButton variant="primary" onClick={handleImport}>
            <Upload className="h-3.5 w-3.5" />
            导入预设
          </AppleButton>
        }
      />
      <PresetPanel />

      {pendingImport && (
        <PresetNameDialog
          mode="import"
          defaultName={pendingImport.defaultName}
          onConfirm={handleConfirmImport}
          onClose={() => setPendingImport(null)}
        />
      )}
    </div>
  );
}
