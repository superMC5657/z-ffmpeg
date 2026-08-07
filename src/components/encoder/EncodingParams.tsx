import { useState } from "react";
import { ChevronDown, ChevronRight } from "lucide-react";
import type { EncoderPreset, AudioCodec } from "@/types";
import { useEncoderStore } from "@/store/encoderStore";

const PRESETS: { value: EncoderPreset; label: string }[] = [
  { value: "ultrafast", label: "Ultrafast" },
  { value: "superfast", label: "Superfast" },
  { value: "veryfast", label: "Veryfast" },
  { value: "faster", label: "Faster" },
  { value: "fast", label: "Fast" },
  { value: "medium", label: "Medium" },
  { value: "slow", label: "Slow" },
  { value: "slower", label: "Slower" },
  { value: "veryslow", label: "Veryslow" },
];

const AUDIO_CODECS: { value: AudioCodec; label: string }[] = [
  { value: "AAC", label: "AAC" },
  { value: "Opus", label: "Opus" },
  { value: "Copy", label: "Copy (复制)" },
  { value: "None", label: "无音频" },
];

export default function EncodingParams() {
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

  const [showResolution, setShowResolution] = useState(false);

  return (
    <div className="space-y-4">
      {/* Rate Control */}
      <div>
        <label className="mb-2 block text-[13px] font-medium text-muted-foreground">
          码率控制
        </label>
        <div className="flex items-center gap-3">
          <button
            onClick={() =>
              setRateControl({ type: "CRF", value: rateControl.type === "CRF" ? rateControl.value : 23 })
            }
            className={`rounded-lg px-4 py-2 text-[14px] font-medium transition-all ${
              rateControl.type === "CRF"
                ? "bg-gradient-brand text-white shadow-md shadow-primary/20"
                : "bg-accent/60 text-muted-foreground hover:bg-accent"
            }`}
          >
            CRF (恒定质量)
          </button>
          <button
            onClick={() =>
              setRateControl({
                type: "ABR",
                bitrateKbps: rateControl.type === "ABR" ? rateControl.bitrateKbps : 5000,
              })
            }
            className={`rounded-lg px-4 py-2 text-[14px] font-medium transition-all ${
              rateControl.type === "ABR"
                ? "bg-gradient-brand text-white shadow-md shadow-primary/20"
                : "bg-accent/60 text-muted-foreground hover:bg-accent"
            }`}
          >
            ABR (平均比特率)
          </button>
        </div>
      </div>

      {/* CRF Slider */}
      {rateControl.type === "CRF" && (
        <div>
          <div className="mb-1 flex items-center justify-between">
            <label className="text-[13px] font-medium text-muted-foreground">
              CRF 值
            </label>
            <span className="text-sm font-mono font-bold">{rateControl.value}</span>
          </div>
          <input
            type="range"
            min={0}
            max={51}
            value={rateControl.value}
            onChange={(e) =>
              setRateControl({ type: "CRF", value: parseInt(e.target.value) })
            }
            className="w-full h-2 cursor-pointer appearance-none rounded-full bg-accent
              [&::-webkit-slider-thumb]:h-4 [&::-webkit-slider-thumb]:w-4
              [&::-webkit-slider-thumb]:appearance-none
              [&::-webkit-slider-thumb]:rounded-full
              [&::-webkit-slider-thumb]:bg-primary
              [&::-webkit-slider-thumb]:shadow-md
              [&::-webkit-slider-thumb]:shadow-primary/40"
          />
          <div className="mt-1 flex justify-between text-[13px] text-muted-foreground">
            <span>无损</span>
            <span>高质量</span>
            <span>平衡</span>
            <span>低质量</span>
          </div>
        </div>
      )}

      {/* ABR Input */}
      {rateControl.type === "ABR" && (
        <div className="flex items-center gap-3">
          <label className="text-[13px] font-medium text-muted-foreground">
            目标比特率
          </label>
          <input
            type="number"
            value={rateControl.bitrateKbps}
            onChange={(e) =>
              setRateControl({
                type: "ABR",
                bitrateKbps: parseInt(e.target.value) || 0,
              })
            }
            className="w-28 rounded-lg border border-border bg-accent/60 px-3 py-1.5 text-sm transition-shadow focus:border-primary focus:ring-2 focus:ring-primary/20 focus:outline-none"
          />
          <span className="text-[13px] text-muted-foreground">kbps</span>
        </div>
      )}

      {/* Encoder Preset */}
      <div>
        <label className="mb-2 block text-[13px] font-medium text-muted-foreground">
          编码预设 (速度/质量)
        </label>
        <select
          value={encoderPreset}
          onChange={(e) => setEncoderPreset(e.target.value as EncoderPreset)}
          className="w-full rounded-lg border border-border bg-accent/60 px-3.5 py-2 text-sm transition-shadow focus:border-primary focus:ring-2 focus:ring-primary/20 focus:outline-none"
        >
          {PRESETS.map((p) => (
            <option key={p.value} value={p.value}>
              {p.label}
            </option>
          ))}
        </select>
        {videoCodec === "AV1" && !hwAccel && (
          <p className="mt-1.5 text-xs text-muted-foreground">
            SVT-AV1 预设为数字 0-13：Ultrafast→13 … Veryslow→1（越小越慢、质量越高）
          </p>
        )}
        {videoCodec === "VP9" && !hwAccel && (
          <p className="mt-1.5 text-xs text-muted-foreground">
            VP9 使用 -cpu-used 0-8：Ultrafast→8 … Veryslow→0（越小越慢、质量越高）
          </p>
        )}
        {hwAccel && (
          <p className="mt-1.5 text-xs text-muted-foreground">
            {hwAccel.device === "NVENC" &&
              "NVENC 预设映射为 p1-p7：Ultrafast→p1 … Veryslow→p7（p1 最快、p7 画质最佳）"}
            {hwAccel.device === "QSV" &&
              "QSV 预设支持 veryfast … veryslow 命名，直接生效"}
            {hwAccel.device === "AMF" &&
              "AMF 预设映射为 speed/balanced/quality：Ultrafast→speed … Veryslow→quality"}
            {hwAccel.device === "VAAPI" &&
              "VAAPI 使用 -compression_level 1-7：Ultrafast→7 … Veryslow→1（1 画质最佳）"}
            {hwAccel.device === "VideoToolbox" &&
              "VideoToolbox 不再支持 preset 参数，将使用编码器默认质量"}
          </p>
        )}
      </div>

      {/* Resolution & Frame Rate - Collapsible */}
      <div>
        <button
          onClick={() => setShowResolution(!showResolution)}
          className="flex items-center gap-1.5 text-[14px] font-medium text-muted-foreground hover:text-foreground"
        >
          {showResolution ? (
            <ChevronDown className="h-4 w-4" />
          ) : (
            <ChevronRight className="h-4 w-4" />
          )}
          高级选项 (分辨率 · 帧率)
        </button>
        {showResolution && (
          <div className="mt-3 space-y-3 pl-4">
            <div className="flex items-center gap-3">
              <label className="w-[3em] shrink-0 text-[13px] text-muted-foreground">
                分辨率
              </label>
              <input
                type="number"
                placeholder="宽"
                value={resolution?.width || ""}
                onChange={(e) =>
                  setResolution({
                    width: parseInt(e.target.value) || 0,
                    height: resolution?.height || 0,
                  })
                }
                className="w-24 rounded-lg border border-border bg-accent/60 px-2 py-1.5 text-[13px] transition-shadow focus:border-primary focus:ring-2 focus:ring-primary/20 focus:outline-none"
              />
              <span className="text-[13px] text-muted-foreground">×</span>
              <input
                type="number"
                placeholder="高"
                value={resolution?.height || ""}
                onChange={(e) =>
                  setResolution({
                    width: resolution?.width || 0,
                    height: parseInt(e.target.value) || 0,
                  })
                }
                className="w-24 rounded-lg border border-border bg-accent/60 px-2 py-1.5 text-[13px] transition-shadow focus:border-primary focus:ring-2 focus:ring-primary/20 focus:outline-none"
              />
            </div>
            <div className="flex items-center gap-3">
              <label className="w-[3em] shrink-0 text-[13px] text-muted-foreground">
                帧率
              </label>
              <input
                type="number"
                placeholder="原始"
                value={frameRate || ""}
                onChange={(e) =>
                  setFrameRate(e.target.value ? parseFloat(e.target.value) : null)
                }
                className="w-24 rounded-lg border border-border bg-accent/60 px-2 py-1.5 text-[13px] transition-shadow focus:border-primary focus:ring-2 focus:ring-primary/20 focus:outline-none"
              />
              <span className="text-[13px] text-muted-foreground">fps (留空=原始)</span>
            </div>
          </div>
        )}
      </div>

      {/* Audio settings */}
      <div className="border-t border-border pt-4">
        <label className="mb-2 block text-[13px] font-medium text-muted-foreground">
          音频设置
        </label>
        <div className="flex items-center gap-4">
          <select
            value={audioCodec}
            onChange={(e) => setAudioCodec(e.target.value as AudioCodec)}
            className="rounded-lg border border-border bg-accent/60 px-3 py-1.5 text-sm transition-shadow focus:border-primary focus:ring-2 focus:ring-primary/20 focus:outline-none"
          >
            {AUDIO_CODECS.map((c) => (
              <option key={c.value} value={c.value}>
                {c.label}
              </option>
            ))}
          </select>
          {audioCodec !== "Copy" && audioCodec !== "None" && (
            <div className="flex items-center gap-2">
              <input
                type="number"
                value={audioBitrate}
                onChange={(e) => setAudioBitrate(parseInt(e.target.value) || 0)}
                className="w-24 rounded-lg border border-border bg-accent/60 px-2 py-1.5 text-[13px] transition-shadow focus:border-primary focus:ring-2 focus:ring-primary/20 focus:outline-none"
              />
              <span className="text-[13px] text-muted-foreground">kbps</span>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
