import { create } from "zustand";
import type { EncodeJob, EncodeProgress, JobStatus, CodecConfig } from "@/types";
import {
  addToQueue,
  removeFromQueue,
  cancelJob,
  startQueue,
  getQueueStatus,
  clearCompleted,
  retryJob,
  getMaxConcurrent,
  setMaxConcurrent,
  getVmafSegments,
  setVmafSegments,
} from "@/lib/tauri";

interface QueueState {
  jobs: EncodeJob[];
  isLoading: boolean;

  // Concurrency limit (shared by Queue page and Settings page)
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
    await addToQueue(files, config, outputDir || null);
    await get().refreshQueue();
  },

  startJobs: async () => {
    await startQueue();
    await get().refreshQueue();
  },

  removeJobs: async (ids) => {
    await removeFromQueue(ids);
    set((s) => ({ jobs: s.jobs.filter((j) => !ids.includes(j.id)) }));
  },

  cancelJob: async (id) => {
    await cancelJob(id);
    await get().refreshQueue();
  },

  clearCompleted: async () => {
    await clearCompleted();
    await get().refreshQueue();
  },

  retryJob: async (id) => {
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
      // 后端队列快照的 progress 是百分比数字；编码中若已有实时进度对象，
      // 保留它以免刷新导致进度条归零
      jobs: jobs.map((j) => {
        if (j.status === "Encoding") {
          const live = s.jobs.find((e) => e.id === j.id);
          if (live?.progress && typeof live.progress === "object") {
            return { ...j, progress: live.progress };
          }
        }
        return j;
      }),
    })),
}));
