import { useState } from "react";
import { ChevronRight } from "lucide-react";
import type { EncoderPreset, AudioCodec } from "@/types";
import { useEncoderStore } from "@/store/encoderStore";
import SegmentedControl from "@/components/layout/SegmentedControl";
import AppleSelect from "@/components/layout/AppleSelect";
import AppleInput from "@/components/layout/AppleInput";

const PRESETS: { value: EncoderPreset; label: string }[] = [
  { value: "ultrafast", label: "Ultrafast（最快）" },
  { value: "superfast", label: "Superfast" },
  { value: "veryfast", label: "Veryfast" },
  { value: "faster", label: "Faster" },
  { value: "fast", label: "Fast" },
  { value: "medium", label: "Medium（均衡）" },
  { value: "slow", label: "Slow" },
  { value: "slower", label: "Slower" },
  { value: "veryslow", label: "Veryslow（最佳）" },
];

const AUDIO_CODECS: { value: AudioCodec; label: string }[] = [
  { value: "AAC", label: "AAC" },
  { value: "Opus", label: "Opus" },
  { value: "Copy", label: "Copy（复制源音频）" },
  { value: "None", label: "无音频" },
];

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-4">
      <label className="shrink-0 text-[13px] text-secondary">{label}</label>
      {children}
    </div>
  );
}

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

  const presetHint = (() => {
    if (hwAccel) {
      switch (hwAccel.device) {
        case "NVENC":
          return "NVENC 预设映射为 p1-p7：Ultrafast→p1 … Veryslow→p7（p1 最快、p7 画质最佳）";
        case "QSV":
          return "QSV 预设支持 veryfast … veryslow 命名，直接生效";
        case "AMF":
          return "AMF 预设映射为 speed/balanced/quality：Ultrafast→speed … Veryslow→quality";
        case "VAAPI":
          return "VAAPI 使用 -compression_level 1-7：Ultrafast→7 … Veryslow→1（1 画质最佳）";
        case "VideoToolbox":
          return "VideoToolbox 不再支持 preset 参数，将使用编码器默认质量";
      }
    }
    if (videoCodec === "AV1")
      return "SVT-AV1 预设为数字 0-13：Ultrafast→13 … Veryslow→1（越小越慢、质量越高）";
    if (videoCodec === "VP9")
      return "VP9 使用 -cpu-used 0-8：Ultrafast→8 … Veryslow→0（越小越慢、质量越高）";
    return null;
  })();

  return (
    <div className="space-y-4">
      {/* 码率控制（CQP 视作恒定质量同款 UI，仅硬件预设会带入） */}
      <Row label="码率控制">
        <SegmentedControl
          value={rateControl.type === "CQP" ? "CRF" : rateControl.type}
          onChange={(type) =>
            setRateControl(
              type === "CRF"
                ? {
                    type: "CRF",
                    value: rateControl.type === "CRF" ? rateControl.value : 23,
                  }
                : {
                    type: "ABR",
                    bitrateKbps:
                      rateControl.type === "ABR" ? rateControl.bitrateKbps : 5000,
                  }
            )
          }
          options={[
            { value: "CRF", label: "恒定质量" },
            { value: "ABR", label: "平均比特率" },
          ]}
        />
      </Row>

      {/* CRF Slider */}
      {(rateControl.type === "CRF" || rateControl.type === "CQP") && (
        <div className="px-0.5">
          <div className="mb-2 flex items-center justify-between">
            <span className="text-[13px] text-secondary">质量值（CRF）</span>
            <span className="text-[15px] font-semibold tabular-nums">
              {rateControl.value}
            </span>
          </div>
          <input
            type="range"
            min={0}
            max={51}
            value={rateControl.value}
            onChange={(e) =>
              setRateControl({ type: "CRF", value: parseInt(e.target.value) })
            }
          />
          <div className="mt-1.5 flex justify-between text-[11px] text-tertiary">
            <span>无损</span>
            <span>高质量</span>
            <span>平衡</span>
            <span>低质量</span>
          </div>
        </div>
      )}

      {/* ABR Input */}
      {rateControl.type === "ABR" && (
        <Row label="目标比特率">
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
            <span className="text-[13px] text-secondary">kbps</span>
          </div>
        </Row>
      )}

      {/* Encoder Preset */}
      <div className="space-y-1.5">
        <Row label="速度预设">
          <AppleSelect
            className="w-52"
            value={encoderPreset}
            onChange={(e) => setEncoderPreset(e.target.value as EncoderPreset)}
          >
            {PRESETS.map((p) => (
              <option key={p.value} value={p.value}>
                {p.label}
              </option>
            ))}
          </AppleSelect>
        </Row>
        {presetHint && (
          <p className="text-[12px] leading-5 text-secondary">{presetHint}</p>
        )}
      </div>

      {/* Resolution & Frame Rate - Collapsible */}
      <div>
        <button
          onClick={() => setShowResolution(!showResolution)}
          aria-expanded={showResolution}
          className="flex items-center gap-1 rounded-md text-[13px] text-secondary transition-colors hover:text-foreground"
        >
          <ChevronRight
            className={`h-3.5 w-3.5 transition-transform duration-200 ${
              showResolution ? "rotate-90" : ""
            }`}
          />
          高级选项（分辨率 · 帧率）
        </button>
        {showResolution && (
          <div className="mt-3 space-y-3 border-l border-hairline pl-4">
            <Row label="分辨率">
              <div className="flex items-center gap-1.5">
                <AppleInput
                  type="number"
                  placeholder="宽"
                  className="w-20 text-center"
                  value={resolution?.width || ""}
                  onChange={(e) =>
                    // 任一维度为 0 → 后端视为未设置分辨率（保持原始），预估回退不缩放；
                    // 直接存对象（含 0），避免置 null 级联清空另一个输入框
                    setResolution({
                      width: parseInt(e.target.value) || 0,
                      height: resolution?.height || 0,
                    })
                  }
                />
                <span className="text-[13px] text-tertiary">×</span>
                <AppleInput
                  type="number"
                  placeholder="高"
                  className="w-20 text-center"
                  value={resolution?.height || ""}
                  onChange={(e) =>
                    setResolution({
                      width: resolution?.width || 0,
                      height: parseInt(e.target.value) || 0,
                    })
                  }
                />
              </div>
            </Row>
            <Row label="帧率">
              <div className="flex items-center gap-2">
                <AppleInput
                  type="number"
                  placeholder="原始"
                  className="w-20 text-center"
                  value={frameRate || ""}
                  onChange={(e) =>
                    setFrameRate(e.target.value ? parseFloat(e.target.value) : null)
                  }
                />
                <span className="text-[13px] text-secondary">fps（留空 = 原始）</span>
              </div>
            </Row>
          </div>
        )}
      </div>

      {/* Audio settings */}
      <Row label="音频">
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
            <>
              <AppleInput
                type="number"
                className="w-20 text-right"
                value={audioBitrate}
                onChange={(e) => setAudioBitrate(parseInt(e.target.value) || 0)}
              />
              <span className="text-[13px] text-secondary">kbps</span>
            </>
          )}
        </div>
      </Row>
    </div>
  );
}
