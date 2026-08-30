import { beforeEach, describe, expect, it } from "vitest";
import { useQueueStore } from "@/store/queueStore";
import type { EncodeJob } from "@/types";

function job(id: string, status: EncodeJob["status"]): EncodeJob {
  return {
    id,
    inputPath: `C:\\in\\${id}.mp4`,
    outputPath: `C:\\in\\${id}_encoded.mp4`,
    codecConfig: {
      videoCodec: "H264",
      videoSettings: {
        rateControl: { type: "CRF", value: 23 },
        encoderPreset: "medium",
        resolution: null,
        frameRate: null,
        pixelFormat: null,
        profile: null,
        additionalParams: [],
      },
      audioSettings: { codec: "AAC", bitrateKbps: 192, channels: 2, sampleRate: 48000 },
      containerFormat: "MP4",
      hwAccel: null,
    },
    status,
    progress: null,
    inputSize: null,
    estimatedOutputSize: null,
    outputSize: null,
    vmafScore: null,
    vmafDetail: null,
    createdAt: new Date().toISOString(),
    startedAt: null,
    completedAt: null,
    error: null,
  };
}

describe("queueStore job status updates", () => {
  beforeEach(() => {
    useQueueStore.setState({
      jobs: [job("a", "Encoding"), job("b", "Pending")],
      paused: false,
    });
  });

  it("updateJobStatus marks completion time for finished jobs", () => {
    useQueueStore.getState().updateJobStatus("a", "Completed");
    const a = useQueueStore.getState().jobs.find((j) => j.id === "a")!;
    expect(a.status).toBe("Completed");
    expect(a.completedAt).toBeTruthy();
  });

  it("updateJobStatus records error text on failure", () => {
    useQueueStore.getState().updateJobStatus("a", "Failed", "FFmpeg exited with code 1");
    const a = useQueueStore.getState().jobs.find((j) => j.id === "a")!;
    expect(a.status).toBe("Failed");
    expect(a.error).toBe("FFmpeg exited with code 1");
  });

  it("updateProgress switches the job into Encoding state", () => {
    useQueueStore.getState().updateProgress({
      jobId: "b",
      fileName: "b.mp4",
      frame: 12,
      fps: 30,
      bitrate: 4000,
      totalSizeKb: 1024,
      estimatedSizeKb: 4096,
      elapsed: "00:00:10",
      percentage: 25,
      speed: 1.5,
      stage: "encoding",
      time: "00:00:05",
    });
    const b = useQueueStore.getState().jobs.find((j) => j.id === "b")!;
    expect(b.status).toBe("Encoding");
    expect(b.progress).toMatchObject({ percentage: 25 });
  });

  it("setJobs keeps the live progress object for encoding jobs", () => {
    useQueueStore.getState().updateProgress({
      jobId: "a",
      fileName: "a.mp4",
      frame: 1,
      fps: 30,
      bitrate: 0,
      totalSizeKb: 0,
      estimatedSizeKb: null,
      elapsed: "00:00:01",
      percentage: 1,
      speed: 1,
      stage: "encoding",
      time: "",
    });
    // 后端快照到达：a 仍在编码，b 仍在排队
    useQueueStore.getState().setJobs([job("a", "Encoding"), job("b", "Pending")]);
    const a = useQueueStore.getState().jobs.find((j) => j.id === "a")!;
    expect(a.progress).toMatchObject({ percentage: 1 });
  });
});
