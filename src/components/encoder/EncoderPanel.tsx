import { useState } from "react";
import { Save, Terminal } from "lucide-react";
import FileSelector from "./FileSelector";
import FileInfo from "./FileInfo";
import CodecSelector from "./CodecSelector";
import EncodingParams from "./EncodingParams";
import HwAccelSelector from "./HwAccelSelector";
import OutputDirSelector from "./OutputDirSelector";
import EncodeButton from "./EncodeButton";
import PresetSelector from "./PresetSelector";
import SavePresetDialog from "./SavePresetDialog";
import FfmpegCommandDialog from "./FfmpegCommandDialog";
import { useEncoderStore } from "@/store/encoderStore";
import { usePresetStore } from "@/store/presetStore";
import { useToastStore } from "@/store/toastStore";
import { buildFfmpegCommands } from "@/lib/tauri";

export default function EncoderPanel() {
  const inputFiles = useEncoderStore((s) => s.inputFiles);
  const buildConfig = useEncoderStore((s) => s.buildConfig);
  const outputDir = useEncoderStore((s) => s.outputDir);
  const importPreset = usePresetStore((s) => s.importPreset);
  const [saveDialogOpen, setSaveDialogOpen] = useState(false);
  const [commandDialogOpen, setCommandDialogOpen] = useState(false);
  const [commandEntries, setCommandEntries] = useState<{ fileName: string; command: string }[]>([]);
  const [building, setBuilding] = useState(false);

  const handleSavePreset = async (name: string) => {
    const config = buildConfig();
    const preset = await importPreset(JSON.stringify(config), name);
    // 保存后自动选中新预设,保持参数与预设一致
    usePresetStore.getState().selectPreset(preset.id);
    useToastStore.getState().showToast(`预设「${name}」已保存`, "success");
  };

  const handleBuildCommand = async () => {
    if (inputFiles.length === 0) {
      useToastStore.getState().showToast("请先添加输入文件", "error");
      return;
    }
    setBuilding(true);
    try {
      const cmds = await buildFfmpegCommands(
        inputFiles.map((f) => f.path),
        buildConfig(),
        outputDir
      );
      setCommandEntries(
        inputFiles.map((f, i) => ({
          fileName: f.fileName,
          command: cmds[i] ?? "",
        }))
      );
      setCommandDialogOpen(true);
    } catch (err) {
      useToastStore.getState().showToast(
        `生成命令失败: ${err instanceof Error ? err.message : String(err)}`,
        "error"
      );
    } finally {
      setBuilding(false);
    }
  };

  return (
    <div className="space-y-7">
      {/* File Selection */}
      <div className="rounded-xl border border-border bg-card p-5 shadow-sm">
        <h2 className="mb-3 text-[15px] font-semibold">输入文件</h2>
        <FileSelector />
        {inputFiles.length > 0 && (
          <div className="mt-3 space-y-2">
            {inputFiles.map((file) => (
              <FileInfo key={file.path} file={file} index={inputFiles.indexOf(file)} />
            ))}
          </div>
        )}
      </div>

      {/* Codec Selection */}
      <div className="rounded-xl border border-border bg-card p-5 shadow-sm">
        <div className="mb-3 flex items-center justify-between">
          <h2 className="text-[15px] font-semibold">编码配置</h2>
          <PresetSelector />
        </div>
        <CodecSelector />
      </div>

      {/* Encoding Parameters */}
      <div className="rounded-xl border border-border bg-card p-5 shadow-sm">
        <h2 className="mb-3 text-[15px] font-semibold">编码参数</h2>
        <EncodingParams />
      </div>

      {/* Hardware Acceleration */}
      <HwAccelSelector />

      {/* Output Settings */}
      <OutputDirSelector />

      {/* Action */}
      <div className="flex items-center justify-end gap-3">
        <button
          onClick={() => setSaveDialogOpen(true)}
          className="flex items-center gap-2 rounded-lg border border-primary/30 bg-primary/10 px-5 py-3 text-[14px] font-medium text-primary transition-all hover:bg-primary/20 active:scale-95"
        >
          <Save className="h-4 w-4" />
          保存为预设
        </button>
        <button
          onClick={handleBuildCommand}
          disabled={building}
          className="flex items-center gap-2 rounded-lg border border-border bg-accent/60 px-5 py-3 text-[14px] font-medium transition-all hover:border-primary/40 hover:text-foreground disabled:cursor-not-allowed disabled:opacity-50"
        >
          <Terminal className="h-4 w-4" />
          {building ? "生成中..." : "生成 FFmpeg 命令"}
        </button>
        <EncodeButton />
      </div>

      {saveDialogOpen && (
        <SavePresetDialog
          onConfirm={handleSavePreset}
          onClose={() => setSaveDialogOpen(false)}
        />
      )}

      {commandDialogOpen && (
        <FfmpegCommandDialog
          entries={commandEntries}
          onClose={() => setCommandDialogOpen(false)}
        />
      )}
    </div>
  );
}
