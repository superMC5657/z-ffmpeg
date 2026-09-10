import { useState } from "react";
import {
  Play,
  Terminal,
  Save,
  FolderOpen,
  RotateCcw,
  ChevronDown,
  Sliders,
  Volume2,
  Folder,
} from "lucide-react";
import type { EncoderPreset, AudioCodec } from "@/types";
import { useEncoderStore } from "@/store/encoderStore";
import { useQueueStore } from "@/store/queueStore";
import { usePresetStore } from "@/store/presetStore";
import { useToastStore } from "@/store/toastStore";
import { useNavigate } from "react-router-dom";
import { isTauriRuntime, cn } from "@/lib/utils";
import { buildFfmpegCommands } from "@/lib/tauri";
import EngineSelector from "./EngineSelector";
import PresetSelector from "./PresetSelector";
import SavePresetDialog from "./SavePresetDialog";
import FfmpegCommandDialog from "./FfmpegCommandDialog";
import SegmentedControl from "@/components/layout/SegmentedControl";
import AppleSelect from "@/components/layout/AppleSelect";
import AppleInput from "@/components/layout/AppleInput";

const PRESETS: { value: EncoderPreset; label: string }[] = [
  { value: "ultrafast", label: "Ultrafast（最快）" },
  { value: "superfast", label: "Superfast" },
  { value: "veryfast", label: "Veryfast" },
  { value: "faster", label: "Faster" },
  { value: "fast", label: "Fast" },
  { value: "medium", label: "Medium（均衡推荐）" },
  { value: "slow", label: "Slow（画质优先）" },
  { value: "slower", label: "Slower" },
  { value: "veryslow", label: "Veryslow（极致画质）" },
];

const AUDIO_CODECS: { value: AudioCodec; label: string }[] = [
  { value: "Copy", label: "Copy（直接复制源音频）" },
  { value: "AAC", label: "AAC（标准兼容）" },
  { value: "Opus", label: "Opus（高效高质量）" },
  { value: "None", label: "无音频（静音）" },
];

export default function UnifiedInspector() {
  const navigate = useNavigate();

  const inputFiles = useEncoderStore((s) => s.inputFiles);
  const clearFiles = useEncoderStore((s) => s.clearFiles);
  const buildConfig = useEncoderStore((s) => s.buildConfig);
  const outputDir = useEncoderStore((s) => s.outputDir);
  const setOutputDir = useEncoderStore((s) => s.setOutputDir);

  const videoCodec = useEncoderStore((s) => s.videoCodec);
  const hwAccel = useEncoderStore((s) => s.hwAccel);
  const rateControl = useEncoderStore((s) => s.rateControl);
  const setRateControl = useEncoderStore((s) => s.setRateControl);
  const encoderPreset = useEncoderStore((s) => s.encoderPreset);
  const setEncoderPreset = useEncoderStore((s) => s.setEncoderPreset);
  const resolution = useEncoderStore((s) => s.resolution);
  const setResolution = useEncoderStore((s) => s.setResolution);
  const frameRate = useEncoderStore((s) => s.frameRate);
  const setFrameRate = useEncoderStore((s) => s.setFrameRate);
  const audioCodec = useEncoderStore((s) => s.audioCodec);
  const setAudioCodec = useEncoderStore((s) => s.setAudioCodec);
  const audioBitrate = useEncoderStore((s) => s.audioBitrate);
  const setAudioBitrate = useEncoderStore((s) => s.setAudioBitrate);

  const importPreset = usePresetStore((s) => s.importPreset);
  const addJobs = useQueueStore((s) => s.addJobs);

  const [showAdvanced, setShowAdvanced] = useState(false);
  const [saveDialogOpen, setSaveDialogOpen] = useState(false);
  const [commandDialogOpen, setCommandDialogOpen] = useState(false);
  const [commandEntries, setCommandEntries] = useState<
    { fileName: string; command: string }[]
  >([]);
  const [building, setBuilding] = useState(false);

  const hasFiles = inputFiles.length > 0;

  const handleSelectOutputDir = async () => {
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

  const handleSavePreset = async (name: string) => {
    const config = buildConfig();
    const preset = await importPreset(JSON.stringify(config), name);
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

  const handleAddToQueueAndStart = async () => {
    if (!hasFiles) return;
    const config = buildConfig();
    const paths = inputFiles.map((f) => f.path);
    try {
      await addJobs(paths, config, outputDir || null);
      clearFiles();
      useToastStore
        .getState()
        .showToast(`已将 ${paths.length} 个任务加入转码队列`, "success");
      navigate("/queue");
    } catch (err) {
      useToastStore.getState().showToast(
        `添加队列失败: ${err instanceof Error ? err.message : String(err)}`,
        "error"
      );
    }
  };

  const presetHint = (() => {
    if (hwAccel) {
      switch (hwAccel.device) {
        case "NVENC":
          return "NVENC 预设映射为 p1-p7：Ultrafast→p1 … Veryslow→p7";
        case "QSV":
          return "QSV 预设支持 veryfast … veryslow 原生命名";
        case "AMF":
          return "AMF 预设映射为 speed / balanced / quality";
        case "VAAPI":
          return "VAAPI 使用 -compression_level 1-7";
        case "VideoToolbox":
          return "VideoToolbox 自动匹配最佳硬件质量";
      }
    }
    if (videoCodec === "AV1") return "SVT-AV1 速度等级 0-13：数字越小画质越高";
    if (videoCodec === "VP9") return "VP9 -cpu-used 0-8：数字越小画质越高";
    return null;
  })();

  return (
    <div className="flex flex-col gap-4 rounded-2xl border border-hairline bg-surface/70 backdrop-blur-md p-4.5 shadow-card">
      {/* 1. 顶部预设快捷切换器与另存按钮 */}
      <div className="flex items-center justify-between gap-2 border-b border-hairline/80 pb-3.5">
        <div className="flex items-center gap-1.5 min-w-0">
          <Sliders className="h-4 w-4 text-accent shrink-0" />
          <span className="text-[13px] font-semibold text-foreground shrink-0">
            编码配置
          </span>
        </div>
        <div className="flex items-center gap-2">
          <PresetSelector />
          <button
            onClick={() => setSaveDialogOpen(true)}
            title="将当前参数保存为自定义预设"
            className="flex h-8 items-center gap-1 rounded-lg bg-fill px-2.5 text-[12px] font-medium text-foreground transition-colors hover:bg-fill-strong shrink-0"
          >
            <Save className="h-3.5 w-3.5 text-secondary" />
            <span className="hidden sm:inline">存为预设</span>
          </button>
        </div>
      </div>

      {/* 2. 编码引擎与加速矩阵 */}
      <EngineSelector />

      {/* 3. 质量与速率控制 */}
      <div className="space-y-3.5 border-t border-hairline/80 pt-3.5">
        <div className="flex items-center justify-between">
          <label className="text-[12px] font-medium text-secondary">
            码率控制模式
          </label>
          <SegmentedControl
            value={rateControl.type === "CQP" ? "CRF" : rateControl.type}
            onChange={(type) =>
              setRateControl(
                type === "CRF"
                  ? {
                      type: "CRF",
                      value:
                        rateControl.type === "CRF" ? rateControl.value : 23,
                    }
                  : {
                      type: "ABR",
                      bitrateKbps:
                        rateControl.type === "ABR"
                          ? rateControl.bitrateKbps
                          : 5000,
                    }
              )
            }
            options={[
              { value: "CRF", label: "恒定画质 (CRF)" },
              { value: "ABR", label: "平均比特率 (ABR)" },
            ]}
          />
        </div>

        {/* CRF 调节滑块 */}
        {(rateControl.type === "CRF" || rateControl.type === "CQP") && (
          <div className="rounded-xl bg-fill/30 p-3 border border-hairline/50">
            <div className="mb-2 flex items-center justify-between">
              <span className="text-[12px] text-secondary">画质系数 (CRF)</span>
              <div className="flex items-baseline gap-1">
                <span className="text-[16px] font-bold text-accent tabular-nums">
                  {rateControl.value}
                </span>
                <span className="text-[11px] text-tertiary">
                  {rateControl.value <= 18
                    ? "(无损级)"
                    : rateControl.value <= 23
                      ? "(视觉无损·推荐)"
                      : rateControl.value <= 28
                        ? "(平衡推荐)"
                        : "(低码率体积)"}
                </span>
              </div>
            </div>

            <input
              type="range"
              min={0}
              max={51}
              value={rateControl.value}
              onChange={(e) =>
                setRateControl({
                  type: "CRF",
                  value: parseInt(e.target.value),
                })
              }
              className="w-full accent-accent"
            />

            <div className="mt-1.5 flex justify-between text-[10px] text-tertiary">
              <span className="text-accent/80 font-medium">0 极佳无损</span>
              <span>18</span>
              <span className="text-success font-medium">23 推荐</span>
              <span>28</span>
              <span>51 低画质</span>
            </div>
          </div>
        )}

        {/* ABR 比特率输入 */}
        {rateControl.type === "ABR" && (
          <div className="flex items-center justify-between rounded-xl bg-fill/30 p-3 border border-hairline/50">
            <span className="text-[12px] text-secondary">目标比特率</span>
            <div className="flex items-center gap-2">
              <AppleInput
                type="number"
                className="w-28 text-right"
                value={rateControl.bitrateKbps}
                onChange={(e) =>
                  setRateControl({
                    type: "ABR",
                    bitrateKbps: parseInt(e.target.value) || 0,
                  })
                }
              />
              <span className="text-[12px] text-secondary">kbps</span>
            </div>
          </div>
        )}

        {/* 速度预设 */}
        <div>
          <div className="flex items-center justify-between gap-2">
            <label className="text-[12px] text-secondary">编码速度预设</label>
            <AppleSelect
              className="w-48"
              value={encoderPreset}
              onChange={(e) =>
                setEncoderPreset(e.target.value as EncoderPreset)
              }
            >
              {PRESETS.map((p) => (
                <option key={p.value} value={p.value}>
                  {p.label}
                </option>
              ))}
            </AppleSelect>
          </div>
          {presetHint && (
            <p className="mt-1 text-right text-[10px] text-tertiary">
              {presetHint}
            </p>
          )}
        </div>
      </div>

      {/* 4. 音频与高级参数（折叠面板） */}
      <div className="border-t border-hairline/80 pt-2">
        <button
          type="button"
          onClick={() => setShowAdvanced(!showAdvanced)}
          className="flex w-full items-center justify-between py-1.5 text-[12px] font-medium text-secondary hover:text-foreground transition-colors"
        >
          <span className="flex items-center gap-1.5">
            <Volume2 className="h-3.5 w-3.5" />
            <span>音频轨、分辨率与帧率</span>
          </span>
          <ChevronDown
            className={cn(
              "h-3.5 w-3.5 transition-transform duration-200",
              showAdvanced ? "rotate-180" : ""
            )}
          />
        </button>

        {showAdvanced && (
          <div className="mt-2 space-y-3 rounded-xl bg-fill/20 p-3 border border-hairline/40">
            {/* 音频配置 */}
            <div className="flex items-center justify-between gap-2">
              <span className="text-[12px] text-secondary">音频编码</span>
              <div className="flex items-center gap-2">
                <AppleSelect
                  className="w-44"
                  value={audioCodec}
                  onChange={(e) => setAudioCodec(e.target.value as AudioCodec)}
                >
                  {AUDIO_CODECS.map((c) => (
                    <option key={c.value} value={c.value}>
                      {c.label}
                    </option>
                  ))}
                </AppleSelect>
                {audioCodec !== "Copy" && audioCodec !== "None" && (
                  <div className="flex items-center gap-1">
                    <AppleInput
                      type="number"
                      className="w-18 text-right"
                      value={audioBitrate}
                      onChange={(e) =>
                        setAudioBitrate(parseInt(e.target.value) || 0)
                      }
                    />
                    <span className="text-[11px] text-tertiary">k</span>
                  </div>
                )}
              </div>
            </div>

            {/* 分辨率 */}
            <div className="flex items-center justify-between gap-2">
              <span className="text-[12px] text-secondary">限制分辨率</span>
              <div className="flex items-center gap-1.5">
                <AppleInput
                  type="number"
                  placeholder="宽"
                  className="w-18 text-center"
                  value={resolution?.width || ""}
                  onChange={(e) =>
                    setResolution({
                      width: parseInt(e.target.value) || 0,
                      height: resolution?.height || 0,
                    })
                  }
                />
                <span className="text-tertiary text-[11px]">×</span>
                <AppleInput
                  type="number"
                  placeholder="高"
                  className="w-18 text-center"
                  value={resolution?.height || ""}
                  onChange={(e) =>
                    setResolution({
                      width: resolution?.width || 0,
                      height: parseInt(e.target.value) || 0,
                    })
                  }
                />
              </div>
            </div>

            {/* 帧率 */}
            <div className="flex items-center justify-between gap-2">
              <span className="text-[12px] text-secondary">最大帧率 (FPS)</span>
              <div className="flex items-center gap-1.5">
                <AppleInput
                  type="number"
                  placeholder="保持原始"
                  className="w-24 text-center"
                  value={frameRate || ""}
                  onChange={(e) =>
                    setFrameRate(
                      e.target.value ? parseFloat(e.target.value) : null
                    )
                  }
                />
                <span className="text-[11px] text-tertiary">fps</span>
              </div>
            </div>
          </div>
        )}
      </div>

      {/* 5. 输出目录选择 */}
      <div className="border-t border-hairline/80 pt-3">
        <div className="flex items-center justify-between mb-1.5">
          <label className="text-[12px] font-medium text-secondary flex items-center gap-1.5">
            <Folder className="h-3.5 w-3.5 text-tertiary" />
            <span>输出位置</span>
          </label>
          {outputDir && (
            <button
              onClick={() => setOutputDir("")}
              className="flex items-center gap-1 text-[11px] text-secondary hover:text-foreground transition-colors"
            >
              <RotateCcw className="h-3 w-3" />
              <span>恢复源目录</span>
            </button>
          )}
        </div>

        <div className="flex items-center gap-2">
          <div className="flex h-9 flex-1 items-center rounded-xl bg-fill/40 px-3 border border-hairline/60 overflow-hidden">
            <span
              className={cn(
                "truncate text-[12px]",
                outputDir ? "text-foreground font-mono" : "text-tertiary"
              )}
              title={outputDir || undefined}
            >
              {outputDir || "默认保存到源文件所在目录"}
            </span>
          </div>
          <button
            type="button"
            onClick={handleSelectOutputDir}
            className="flex h-9 items-center gap-1.5 rounded-xl bg-fill px-3 text-[12px] font-medium text-foreground transition-colors hover:bg-fill-strong shrink-0"
          >
            <FolderOpen className="h-3.5 w-3.5 text-secondary" />
            <span>浏览</span>
          </button>
        </div>
      </div>

      {/* 6. 行动区 (CTA Actions) */}
      <div className="border-t border-hairline/80 pt-4 space-y-2">
        <button
          onClick={handleAddToQueueAndStart}
          disabled={!hasFiles}
          className={cn(
            "flex h-11 w-full items-center justify-center gap-2 rounded-xl text-[14px] font-semibold transition-all shadow-md active:scale-[0.98]",
            hasFiles
              ? "bg-accent text-on-accent hover:bg-accent-hover hover:shadow-accent/25 hover:shadow-lg cursor-pointer"
              : "cursor-not-allowed bg-fill text-tertiary opacity-70"
          )}
        >
          <Play className="h-4 w-4 fill-current" />
          <span>
            {hasFiles
              ? `添加到转码队列 (${inputFiles.length} 个文件)`
              : "请先添加待处理文件"}
          </span>
        </button>

        <button
          onClick={handleBuildCommand}
          disabled={building}
          className="flex h-8 w-full items-center justify-center gap-1.5 rounded-lg text-[12px] text-secondary hover:text-foreground hover:bg-fill transition-colors"
        >
          <Terminal className="h-3.5 w-3.5" />
          <span>{building ? "正在生成…" : "查看 FFmpeg CLI 命令行"}</span>
        </button>
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
