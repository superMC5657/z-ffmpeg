import { useState } from "react";
import { Save, Terminal, Play } from "lucide-react";
import FileSelector from "./FileSelector";
import FileInfo from "./FileInfo";
import CodecSelector from "./CodecSelector";
import EncodingParams from "./EncodingParams";
import HwAccelSelector from "./HwAccelSelector";
import OutputDirSelector from "./OutputDirSelector";
import PresetSelector from "./PresetSelector";
import SavePresetDialog from "./SavePresetDialog";
import FfmpegCommandDialog from "./FfmpegCommandDialog";
import { useEncoderStore } from "@/store/encoderStore";
import { usePresetStore } from "@/store/presetStore";
import { useQueueStore } from "@/store/queueStore";
import { useNavigate } from "react-router-dom";
import { useToastStore } from "@/store/toastStore";
import { buildFfmpegCommands } from "@/lib/tauri";
import { formatFileSizeCompact } from "@/lib/utils";
import type { VideoCodec } from "@/types";
import Card from "@/components/layout/Card";

const CODEC_LABELS: Record<VideoCodec, string> = {
  H264: "H.264",
  H265: "H.265 / HEVC",
  AV1: "AV1",
  VP9: "VP9",
};

/** 检查器式摘要卡：实时反映当前编码配置 */
function EncodeSummary() {
  const videoCodec = useEncoderStore((s) => s.videoCodec);
  const containerFormat = useEncoderStore((s) => s.containerFormat);
  const rateControl = useEncoderStore((s) => s.rateControl);
  const encoderPreset = useEncoderStore((s) => s.encoderPreset);
  const resolution = useEncoderStore((s) => s.resolution);
  const frameRate = useEncoderStore((s) => s.frameRate);
  const audioCodec = useEncoderStore((s) => s.audioCodec);
  const audioBitrate = useEncoderStore((s) => s.audioBitrate);
  const hwAccel = useEncoderStore((s) => s.hwAccel);
  const outputDir = useEncoderStore((s) => s.outputDir);
  const inputFiles = useEncoderStore((s) => s.inputFiles);
  const estimatedSizes = useEncoderStore((s) => s.estimatedSizes);

  const quality =
    rateControl.type === "ABR"
      ? `${rateControl.bitrateKbps} kbps`
      : `${rateControl.type} ${rateControl.value}`;
  const audio =
    audioCodec === "Copy" || audioCodec === "None"
      ? audioCodec === "Copy"
        ? "复制源音频"
        : "移除音频"
      : `${audioCodec} · ${audioBitrate} kbps`;
  const size = Object.values(estimatedSizes).reduce<number>(
    (a, b) => a + (b ?? 0),
    0
  );
  const hasEstimate =
    inputFiles.length > 0 &&
    Object.values(estimatedSizes).length === inputFiles.length;

  const rows: { label: string; value: string; mono?: boolean }[] = [
    { label: "编码器", value: CODEC_LABELS[videoCodec] },
    { label: "封装格式", value: containerFormat },
    { label: "质量", value: quality, mono: true },
    { label: "速度预设", value: encoderPreset },
    { label: "硬件加速", value: hwAccel ? hwAccel.device : "软件编码" },
  ];
  if (resolution?.width && resolution?.height) {
    rows.push({
      label: "分辨率",
      value: `${resolution.width}×${resolution.height}`,
      mono: true,
    });
  }
  if (frameRate) {
    rows.push({ label: "帧率", value: `${frameRate} fps`, mono: true });
  }
  rows.push({ label: "音频", value: audio });
  rows.push({
    label: "输出位置",
    value: outputDir || "源文件所在目录",
  });

  return (
    <Card
      title="编码摘要"
      contentClassName="px-5 py-2"
    >
      <dl>
        {rows.map(({ label, value, mono }) => (
          <div
            key={label}
            className="flex items-baseline justify-between gap-3 border-b border-hairline py-2 last:border-0"
          >
            <dt className="shrink-0 text-[12px] text-secondary">{label}</dt>
            <dd
              className={`truncate text-right text-[12px] font-medium ${
                mono ? "tabular-nums" : ""
              }`}
              title={value}
            >
              {value}
            </dd>
          </div>
        ))}
        {hasEstimate && (
          <div className="flex items-baseline justify-between gap-3 pt-2">
            <dt className="shrink-0 text-[12px] text-secondary">
              预计输出{inputFiles.length > 1 ? `（${inputFiles.length} 个文件）` : ""}
            </dt>
            <dd className="text-right text-[12px] font-semibold tabular-nums text-accent">
              ≈ {formatFileSizeCompact(size)}
            </dd>
          </div>
        )}
      </dl>
    </Card>
  );
}

export default function EncoderPanel() {
  const inputFiles = useEncoderStore((s) => s.inputFiles);
  const buildConfig = useEncoderStore((s) => s.buildConfig);
  const outputDir = useEncoderStore((s) => s.outputDir);
  const importPreset = usePresetStore((s) => s.importPreset);
  const addJobs = useQueueStore((s) => s.addJobs);
  const clearFiles = useEncoderStore((s) => s.clearFiles);
  const navigate = useNavigate();
  const [saveDialogOpen, setSaveDialogOpen] = useState(false);
  const [commandDialogOpen, setCommandDialogOpen] = useState(false);
  const [commandEntries, setCommandEntries] = useState<{ fileName: string; command: string }[]>([]);
  const [building, setBuilding] = useState(false);

  const hasFiles = inputFiles.length > 0;

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

  const handleAddToQueue = async () => {
    if (!hasFiles) return;
    const config = buildConfig();
    const paths = inputFiles.map((f) => f.path);
    try {
      await addJobs(paths, config, outputDir || null);
      // 添加成功后清空文件选择,避免下次误把同一批文件再次入队
      clearFiles();
      useToastStore.getState().showToast(
        `已将 ${paths.length} 个任务添加到队列`,
        "success"
      );
      navigate("/queue");
    } catch (err) {
      useToastStore.getState().showToast(
        `添加队列失败: ${err instanceof Error ? err.message : String(err)}`,
        "error"
      );
    }
  };

  return (
    <div className="grid grid-cols-1 items-start gap-6 lg:grid-cols-[minmax(0,1fr)_290px]">
      {/* 左列：配置流程 */}
      <div className="min-w-0 space-y-5">
        <Card title="输入文件">
          <FileSelector />
          {inputFiles.length > 0 && (
            <div className="mt-3 space-y-1.5">
              {inputFiles.map((file, index) => (
                <FileInfo key={file.path} file={file} index={index} />
              ))}
            </div>
          )}
        </Card>

        <Card
          title="编码配置"
          action={<PresetSelector />}
        >
          <CodecSelector />
        </Card>

        <Card title="编码参数">
          <EncodingParams />
        </Card>

        <Card title="硬件加速">
          <HwAccelSelector />
        </Card>

        <Card title="输出设置">
          <OutputDirSelector />
        </Card>
      </div>

      {/* 右列：检查器摘要 + 操作 */}
      <aside className="space-y-4 max-lg:static lg:sticky lg:top-2">
        <EncodeSummary />
        <div className="space-y-2.5 rounded-[14px] border border-hairline bg-surface p-4 shadow-card">
          <button
            onClick={handleAddToQueue}
            disabled={!hasFiles}
            className={`flex h-10 w-full items-center justify-center gap-2 rounded-[10px] text-[14px] font-medium transition-all active:scale-[0.98] ${
              hasFiles
                ? "bg-accent text-on-accent shadow-sm hover:bg-accent-hover"
                : "cursor-default bg-fill text-tertiary"
            }`}
          >
            <Play className="h-4 w-4" />
            添加到队列
          </button>
          <div className="grid grid-cols-2 gap-2.5">
            <button
              onClick={() => setSaveDialogOpen(true)}
              className="flex h-9 items-center justify-center gap-1.5 rounded-lg bg-fill text-[13px] font-medium text-foreground transition-colors hover:bg-fill-strong"
            >
              <Save className="h-3.5 w-3.5" />
              保存预设
            </button>
            <button
              onClick={handleBuildCommand}
              disabled={building}
              className="flex h-9 items-center justify-center gap-1.5 rounded-lg bg-fill text-[13px] font-medium text-foreground transition-colors hover:bg-fill-strong disabled:opacity-50"
            >
              <Terminal className="h-3.5 w-3.5" />
              {building ? "生成中…" : "查看命令"}
            </button>
          </div>
        </div>
      </aside>

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
