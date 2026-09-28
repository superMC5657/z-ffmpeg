import { describe, expect, it } from "vitest";
import {
  cn,
  estimateRemainingSeconds,
  formatBitrate,
  formatCompressionRatio,
  formatDuration,
  formatError,
  formatFileSize,
  formatFileSizeCompact,
  formatPercentage,
  formatSpeed,
  getFileName,
  isTauriRuntime,
  parseElapsedSeconds,
} from "@/lib/utils";

describe("formatFileSize", () => {
  it("formats bytes across unit boundaries", () => {
    expect(formatFileSize(0)).toBe("0 B");
    expect(formatFileSize(512)).toBe("512.0 B");
    expect(formatFileSize(1024)).toBe("1.0 KB");
    expect(formatFileSize(1536)).toBe("1.5 KB");
    expect(formatFileSize(20 * 1024 * 1024)).toBe("20.0 MB");
  });
});

describe("formatFileSizeCompact", () => {
  it("strips trailing .0 and spaces", () => {
    expect(formatFileSizeCompact(0)).toBe("0B");
    expect(formatFileSizeCompact(20 * 1024 * 1024)).toBe("20MB");
    expect(formatFileSizeCompact(41.8 * 1024 * 1024)).toBe("41.8MB");
    expect(formatFileSizeCompact(4.2 * 1024)).toBe("4.2KB");
  });
});

describe("formatDuration", () => {
  it("renders mm:ss, h:mm:ss, and a placeholder for invalid input", () => {
    expect(formatDuration(1)).toBe("0:01");
    expect(formatDuration(65)).toBe("1:05");
    expect(formatDuration(3661)).toBe("1:01:01");
    expect(formatDuration(NaN)).toBe("--:--:--");
    expect(formatDuration(Infinity)).toBe("--:--:--");
  });
});

describe("formatBitrate", () => {
  it("scales units", () => {
    expect(formatBitrate(500)).toBe("500 kbps");
    expect(formatBitrate(8000)).toBe("8.0 Mbps");
    expect(formatBitrate(1_500_000)).toBe("1.5 Gbps");
  });
});

describe("misc formatters", () => {
  it("formatSpeed / formatFps / formatPercentage", () => {
    expect(formatSpeed(1.234)).toBe("1.23x");
    expect(formatSpeed(1.234, 1)).toBe("1.2x");
    expect(formatPercentage(33.33)).toBe("33.3%");
  });
});

describe("formatCompressionRatio", () => {
  it("reports shrinkage as a positive ratio with a down arrow", () => {
    const r = formatCompressionRatio(100 * 1024 * 1024, 70 * 1024 * 1024);
    expect(r.text).toBe("↓30.0% 70MB");
    expect(r.ratio).toBeCloseTo(30);
    expect(r.enlarged).toBe(false);
  });

  it("reports enlargement as a negative ratio with an up arrow", () => {
    const r = formatCompressionRatio(10 * 1024 * 1024, 12 * 1024 * 1024);
    expect(r.text).toBe("↑20.0% 12MB");
    expect(r.ratio).toBeCloseTo(-20);
    expect(r.enlarged).toBe(true);
  });

  it("degrades to output size text when input size is missing or invalid", () => {
    expect(formatCompressionRatio(null, 20 * 1024 * 1024)).toEqual({
      text: "20MB",
      ratio: null,
      enlarged: false,
    });
    expect(formatCompressionRatio(0, 20 * 1024 * 1024).ratio).toBeNull();
  });

  it("returns empty text when there is no output size", () => {
    expect(formatCompressionRatio(100, null)).toEqual({
      text: "",
      ratio: null,
      enlarged: false,
    });
  });
});

describe("getFileName", () => {
  it("extracts the file name from windows and posix paths", () => {
    expect(getFileName("C:\\videos\\a.mp4")).toBe("a.mp4");
    expect(getFileName("/home/u/b.mkv")).toBe("b.mkv");
    expect(getFileName("plain.mp4")).toBe("plain.mp4");
  });
});

describe("formatError", () => {
  it("uses Error.message when available and stringifies other values", () => {
    expect(formatError(new Error("boom"))).toBe("boom");
    expect(formatError("nope")).toBe("nope");
    expect(formatError(42)).toBe("42");
  });
});

describe("parseElapsedSeconds", () => {
  it("parses hh:mm:ss and mm:ss", () => {
    expect(parseElapsedSeconds("01:02:03")).toBe(3723);
    expect(parseElapsedSeconds("02:03")).toBe(123);
    expect(parseElapsedSeconds("45")).toBe(45);
    expect(parseElapsedSeconds("xx:yy")).toBe(0);
  });
});

describe("estimateRemainingSeconds", () => {
  it("extrapolates remaining time from elapsed and progress", () => {
    // 100 秒跑了 50% → 剩 100 秒
    expect(estimateRemainingSeconds("00:01:40", 50)).toBe(100);
  });
  it("returns null when progress or elapsed is too small to trust", () => {
    expect(estimateRemainingSeconds("00:00:10", 0.3)).toBeNull();
    expect(estimateRemainingSeconds("00:00:00", 50)).toBeNull();
  });
});

describe("cn / isTauriRuntime", () => {
  it("merges class names", () => {
    expect(cn("a", undefined, false, "c")).toBe("a c");
  });
  it("is false outside the Tauri WebView", () => {
    expect(isTauriRuntime()).toBe(false);
  });
});
