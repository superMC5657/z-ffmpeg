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
