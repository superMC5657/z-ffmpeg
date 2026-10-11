import { create } from "zustand";
import type { EncodeJob, EncodeProgress, JobStatus, CodecConfig } from "@/types";
import { isEncodeProgressStrict } from "@/lib/progress";
import {
  addToQueue,
  removeFromQueue,
  cancelJob,
  startQueue,
  pauseQueue,
  resumeQueue,
  getQueueStatus,
  clearCompleted,
  retryJob,
  getMaxConcurrent,
  setMaxConcurrent,
  getVmafSegments,
  setVmafSegments,
} from "@/lib/tauri";
import { zlog } from "@/lib/z-log";

interface QueueState {
  jobs: EncodeJob[];
  isLoading: boolean;
  /** 队列级暂停：暂停后不再自动启动下一个任务（正在编码的不受影响） */
  paused: boolean;

  // 并发数上限（队列页与设置页共享）
  maxConcurrent: number;
  maxConcurrentLoaded: boolean;
  fetchMaxConcurrent: () => Promise<void>;
  updateMaxConcurrent: (value: number) => Promise<number>;

  // VMAF 段数设置（0 = 全量对比，N = N 段 × 5 秒均匀采样），队列页与设置页共享
  vmafSegments: number;
  vmafSegmentsLoaded: boolean;
  fetchVmafSegments: () => Promise<void>;
  updateVmafSegments: (value: number) => Promise<number>;

  addJobs: (files: string[], config: CodecConfig, outputDir?: string | null) => Promise<void>;
  startJobs: () => Promise<void>;
  /** 暂停/恢复队列自动调度 */
  pauseJobs: () => Promise<void>;
  resumeJobs: () => Promise<void>;
  removeJobs: (ids: string[]) => Promise<void>;
  cancelJob: (id: string) => Promise<void>;
  clearCompleted: () => Promise<void>;
  /** 重试一个失败/已取消的任务 */
  retryJob: (id: string) => Promise<void>;
  refreshQueue: () => Promise<void>;
  updateProgress: (progress: EncodeProgress) => void;
  updateJobStatus: (jobId: string, status: JobStatus, error?: string) => void;
  setJobs: (jobs: EncodeJob[]) => void;
}

export const useQueueStore = create<QueueState>((set, get) => ({
  jobs: [],
  isLoading: false,
  paused: false,
  maxConcurrent: 2,
  maxConcurrentLoaded: false,
  vmafSegments: 4,
  vmafSegmentsLoaded: false,

  fetchMaxConcurrent: async () => {
    try {
      const v = await getMaxConcurrent();
      set({ maxConcurrent: v, maxConcurrentLoaded: true });
    } catch {
      set({ maxConcurrentLoaded: true });
    }
  },

  updateMaxConcurrent: async (value) => {
    const saved = await setMaxConcurrent(value);
    set({ maxConcurrent: saved, maxConcurrentLoaded: true });
    return saved;
  },

  fetchVmafSegments: async () => {
    try {
      const v = await getVmafSegments();
      set({ vmafSegments: v, vmafSegmentsLoaded: true });
    } catch {
      set({ vmafSegmentsLoaded: true });
    }
  },

  updateVmafSegments: async (value) => {
    const saved = await setVmafSegments(value);
    set({ vmafSegments: saved, vmafSegmentsLoaded: true });
    return saved;
  },

  addJobs: async (files, config, outputDir) => {
    void zlog.uiAction("添加任务至队列", {
      count: files.length,
      videoCodec: config.videoCodec,
      container: config.containerFormat,
      hw: config.hwAccel?.device ?? "none",
    });
    await addToQueue(files, config, outputDir || null);
    await get().refreshQueue();
  },

  startJobs: async () => {
    void zlog.uiAction("点击开始执行队列");
    await startQueue();
    await get().refreshQueue();
  },

  pauseJobs: async () => {
    void zlog.uiAction("点击暂停队列调度");
    await pauseQueue();
    set({ paused: true });
  },

  resumeJobs: async () => {
    void zlog.uiAction("点击恢复队列调度");
    await resumeQueue();
    set({ paused: false });
    await get().refreshQueue();
  },

  removeJobs: async (ids) => {
    void zlog.uiAction("移除队列任务", { count: ids.length, ids });
    await removeFromQueue(ids);
    set((s) => ({ jobs: s.jobs.filter((j) => !ids.includes(j.id)) }));
  },

  cancelJob: async (id) => {
    void zlog.uiAction("取消队列任务", { id });
    await cancelJob(id);
    await get().refreshQueue();
  },

  clearCompleted: async () => {
    void zlog.uiAction("清空已完成队列任务");
    await clearCompleted();
    await get().refreshQueue();
  },

  retryJob: async (id) => {
    void zlog.uiAction("重试失败队列任务", { id });
    const ok = await retryJob(id);
    if (ok) {
      // 本地立即反映状态变化,再与后端快照对齐
      set((s) => ({
        jobs: s.jobs.map((j) =>
          j.id === id
            ? { ...j, status: "Pending", error: null, completedAt: null, progress: null }
            : j
        ),
      }));
      await get().refreshQueue();
    }
  },

  refreshQueue: async () => {
    set({ isLoading: true });
    try {
      const status = await getQueueStatus();
      set({
        jobs: status.jobs,
        paused: status.paused,
        isLoading: false,
      });
    } catch {
      set({ isLoading: false });
    }
  },

  updateProgress: (progress) => {
    set((s) => ({
      jobs: s.jobs.map((j) =>
        j.id === progress.jobId
          ? { ...j, progress, status: "Encoding" as JobStatus }
          : j
      ),
    }));
  },

  updateJobStatus: (jobId, status, error) => {
    set((s) => ({
      jobs: s.jobs.map((j) =>
        j.id === jobId
          ? {
              ...j,
              status,
              error: error || null,
              // 本地先合成完成时间，保证 UI 即时翻转；下一次 queue://updated
              // 到达时以后端快照的 completed_at 为准（见 setJobs 合并逻辑）。
              completedAt:
                status === "Completed" || status === "Failed"
                  ? new Date().toISOString()
                  : j.completedAt,
            }
          : j
      ),
    }));
  },

  setJobs: (jobs) =>
    set((s) => ({
      jobs: jobs.map((j) => {
        let next = j;
        const live = s.jobs.find((e) => e.id === j.id);
        // completedAt 以后端快照优先：后端已写入时直接采用，避免本地合成时间
        // 与后端时间差造成的闪烁；后端尚未写入（null）时保留本地已合成的值，避免回退为空。
        if (
          next.completedAt == null &&
          live?.completedAt != null &&
          (next.status === "Completed" ||
            next.status === "Failed" ||
            next.status === "Cancelled")
        ) {
          next = { ...next, completedAt: live.completedAt };
        }
        // 后端队列快照的 progress 是百分比数字；编码中若已有实时进度对象，
        // 保留它以免刷新导致进度条归零
        const liveProgress = live?.progress;
        if (next.status === "Encoding" && isEncodeProgressStrict(liveProgress)) {
          return { ...next, progress: liveProgress };
        }
        return next;
      }),
    })),
}));
