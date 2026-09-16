import { beforeEach, describe, expect, it } from "vitest";
import { useEncoderStore } from "@/store/encoderStore";
import type { CodecConfig } from "@/types";

/** 一份完整的合法配置（schema 齐全） */
function validConfig(): CodecConfig {
  return {
    videoCodec: "H265",
    audioSettings: {
      codec: "AAC",
      bitrateKbps: 128,
      channels: 2,
      sampleRate: 48000,
    },
    videoSettings: {
      rateControl: { type: "CRF", value: 20 },
      encoderPreset: "slow",
      resolution: { width: 1280, height: 720 },
      frameRate: 30,
      pixelFormat: "yuv420p",
      profile: null,
      additionalParams: [],
    },
    containerFormat: "MKV",
    hwAccel: null,
  };
}

describe("encoderStore.applyConfig", () => {
  beforeEach(() => {
    // 回到默认值，避免测试间串扰
    useEncoderStore.setState({
      videoCodec: "H264",
      rateControl: { type: "CRF", value: 23 },
      encoderPreset: "medium",
      resolution: null,
      frameRate: null,
      pixelFormat: null,
      audioCodec: "AAC",
      audioBitrate: 192,
      containerFormat: "MP4",
      hwAccel: null,
    });
  });

  it("applies a complete config field by field", () => {
    useEncoderStore.getState().applyConfig(validConfig());
    const s = useEncoderStore.getState();
    expect(s.videoCodec).toBe("H265");
    expect(s.rateControl).toEqual({ type: "CRF", value: 20 });
    expect(s.encoderPreset).toBe("slow");
    expect(s.resolution).toEqual({ width: 1280, height: 720 });
    expect(s.audioBitrate).toBe(128);
    expect(s.containerFormat).toBe("MKV");
  });

  it("ignores missing/undefined fields from malformed imported presets", () => {
    // 模拟 import_preset 接受任意 JSON：videoSettings 缺失、字段为 undefined
    const malformed = {
      videoCodec: "H265",
      videoSettings: undefined,
      audioSettings: undefined,
      containerFormat: undefined,
    } as unknown as CodecConfig;

    expect(() => useEncoderStore.getState().applyConfig(malformed)).not.toThrow();

    const s = useEncoderStore.getState();
    // 已知字段被应用
    expect(s.videoCodec).toBe("H265");
    // 缺失字段不覆盖当前表单值（否则 buildConfig 会在 Rust 端反序列化失败）
    expect(s.rateControl).toEqual({ type: "CRF", value: 23 });
    expect(s.encoderPreset).toBe("medium");
    expect(s.audioCodec).toBe("AAC");
    expect(s.audioBitrate).toBe(192);
    expect(s.containerFormat).toBe("MP4");
  });
});

describe("encoderStore.buildConfig", () => {
  it("round-trips store fields into a codec config", () => {
    const config = useEncoderStore.getState().buildConfig();
    expect(config.videoCodec).toBeTypeOf("string");
    expect(config.audioSettings.bitrateKbps).toBeGreaterThan(0);
    expect(config.videoSettings.rateControl).toHaveProperty("type");
    expect(config.containerFormat).toBeTypeOf("string");
  });
});

describe("encoderStore quality adaptation", () => {
  it("adapts recommended quality when switching videoCodec", () => {
    // 默认 H264 CPU，值为 23
    useEncoderStore.setState({
      videoCodec: "H264",
      hwAccel: null,
      rateControl: { type: "CRF", value: 23 },
    });

    // 切到 AV1 CPU，自动适应为 30
    useEncoderStore.getState().setVideoCodec("AV1");
    expect(useEncoderStore.getState().rateControl).toEqual({ type: "CRF", value: 30 });

    // 切到 H265 CPU，自动适应为 26
    useEncoderStore.getState().setVideoCodec("H265");
    expect(useEncoderStore.getState().rateControl).toEqual({ type: "CRF", value: 26 });

    // 切回 H264，自动适应为 23
    useEncoderStore.getState().setVideoCodec("H264");
    expect(useEncoderStore.getState().rateControl).toEqual({ type: "CRF", value: 23 });
  });

  it("adapts recommended quality when enabling hardware acceleration", () => {
    useEncoderStore.setState({
      videoCodec: "AV1",
      hwAccel: null,
      rateControl: { type: "CRF", value: 30 },
    });

    // 启用 NVENC 硬编，AV1 推荐值由 30 自动提升为 32（抵消硬件编码码率浮躁）
    useEncoderStore.getState().setHwAccel({ device: "NVENC", deviceIndex: null });
    expect(useEncoderStore.getState().rateControl).toEqual({ type: "CRF", value: 32 });

    // 切回 CPU 软编，自动恢复为 30
    useEncoderStore.getState().setHwAccel(null);
    expect(useEncoderStore.getState().rateControl).toEqual({ type: "CRF", value: 30 });
  });

  it("preserves user relative quality preference across codec switches", () => {
    // 用户偏好更高画质：在 H264 下设为 20（比推荐 23 低 3 档，即高画质）
    useEncoderStore.setState({
      videoCodec: "H264",
      hwAccel: null,
      rateControl: { type: "CRF", value: 20 },
    });

    // 切换到 AV1（推荐 30），保持高画质偏好，相对偏移 -3 -> 27
    useEncoderStore.getState().setVideoCodec("AV1");
    expect(useEncoderStore.getState().rateControl).toEqual({ type: "CRF", value: 27 });
  });
});

