import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  CodecConfig,
  FileInfo,
  EncodeProgress,
  EncodeResult,
  QueueStatus,
  Preset,
  SystemInfo,
  FfmpegStatusInfo,
  VmafResult,
} from "@/types";

// ============================================================
// Encoding commands
// ============================================================

export async function probeFile(filePath: string): Promise<FileInfo> {
  return invoke<FileInfo>("probe_file", { filePath });
}

export async function startEncode(
  config: CodecConfig,
  inputPath: string,
  outputPath: string,
  jobId: string
): Promise<void> {
  return invoke("start_encode", { config, inputPath, outputPath, jobId });
}

export async function cancelEncode(jobId: string): Promise<void> {
  return invoke("cancel_encode", { jobId });
}

export async function buildFfmpegCommands(
  files: string[],
  config: CodecConfig,
  outputDir?: string | null
): Promise<string[]> {
  return invoke<string[]>("build_ffmpeg_commands", {
    files,
    config,
    outputDir: outputDir || null,
  });
}

export async function saveCommandToFile(
  content: string,
  path: string
): Promise<void> {
  return invoke("save_command_to_file", { content, path });
}

/** 按当前编码参数预估各输入文件输出体积（字节）；信息不足项为 null */
export async function estimateOutputSizes(
  config: CodecConfig,
  files: FileInfo[]
): Promise<(number | null)[]> {
  return invoke<(number | null)[]>("estimate_output_sizes", { config, files });
}

// ============================================================
// Queue commands
// ============================================================

export async function addToQueue(
  files: string[],
  config: CodecConfig,
  outputDir?: string | null
): Promise<string[]> {
  return invoke<string[]>("add_to_queue", { files, config, outputDir });
}

export async function removeFromQueue(jobIds: string[]): Promise<void> {
  return invoke("remove_from_queue", { jobIds });
}

export async function cancelJob(jobId: string): Promise<void> {
  return invoke("cancel_job", { jobId });
}

export async function startQueue(): Promise<void> {
  return invoke("start_queue");
}

/** 暂停队列自动调度：正在编码的任务继续，剩余 Pending 不再自动开始 */
export async function pauseQueue(): Promise<void> {
  return invoke("pause_queue");
}

/** 解除队列暂停，并立即拉起调度 */
export async function resumeQueue(): Promise<void> {
  return invoke("resume_queue");
}

export async function getQueueStatus(): Promise<QueueStatus> {
  return invoke<QueueStatus>("get_queue_status");
}

export async function clearCompleted(): Promise<void> {
  return invoke("clear_completed");
}

/** 重新排队一个失败/已取消的任务并开始处理 */
export async function retryJob(jobId: string): Promise<boolean> {
  return invoke<boolean>("retry_job", { jobId });
}

export async function getMaxConcurrent(): Promise<number> {
  return invoke<number>("get_max_concurrent");
}

export async function setMaxConcurrent(value: number): Promise<number> {
  return invoke<number>("set_max_concurrent", { value });
}

// ============================================================
// Preset commands
// ============================================================

export async function loadPresets(): Promise<Preset[]> {
  return invoke<Preset[]>("load_presets");
}

export async function deletePreset(id: string): Promise<void> {
  return invoke("delete_preset", { id });
}

export async function exportPreset(id: string): Promise<string> {
  return invoke<string>("export_preset", { id });
}

/** 直接把预设导出为 JSON 文件到指定路径(由 Rust 后端写文件) */
export async function exportPresetToFile(id: string, path: string): Promise<string> {
  return invoke<string>("export_preset_to_file", { id, path });
}

export async function importPreset(json: string, name: string): Promise<Preset> {
  return invoke<Preset>("import_preset", { json, name });
}

export async function getBuiltinPresets(): Promise<Preset[]> {
  return invoke<Preset[]>("get_builtin_presets");
}

// ============================================================
// History commands
// ============================================================

/** 历史查询条件：全可选；limit 缺省 = 后端不分页，全量返回 */
export interface HistoryQuery {
  limit?: number;
  offset?: number;
  status?: string;
  search?: string;
}

/** 分页历史结果：entries 为当前页，total 为筛选后总条数 */
export interface HistoryPageResult {
  entries: unknown[];
  total: number;
}

export async function getHistory(query?: HistoryQuery): Promise<HistoryPageResult> {
  return invoke<HistoryPageResult>("get_history", {
    limit: query?.limit ?? null,
    offset: query?.offset ?? null,
    status: query?.status ?? null,
    search: query?.search ?? null,
  });
}

export async function deleteHistory(ids: string[]): Promise<void> {
  return invoke("delete_history", { ids });
}

export async function clearHistory(): Promise<void> {
  return invoke("clear_history");
}

// ============================================================
// VMAF quality commands
// ============================================================

/** 计算已完成编码任务的 VMAF 得分。segments=0 全量对比，否则 N 段 × 5 秒均匀采样 */
export async function computeVmaf(jobId: string, segments: number): Promise<VmafResult> {
  return invoke<VmafResult>("compute_vmaf", { jobId, segments });
}

/** 读取 VMAF 段数设置（0 = 全量，N = N 段 × 5 秒） */
export async function getVmafSegments(): Promise<number> {
  return invoke<number>("get_vmaf_segments");
}

/** 保存 VMAF 段数设置，返回保存后的值 */
export async function setVmafSegments(value: number): Promise<number> {
  return invoke<number>("set_vmaf_segments", { value });
}

// ============================================================
// System commands
// ============================================================

export async function detectHwAccel(): Promise<SystemInfo> {
  return invoke<SystemInfo>("detect_hw_accel");
}

export async function getSystemInfo(): Promise<SystemInfo> {
  return invoke<SystemInfo>("get_system_info");
}

export async function checkFfmpegStatus(): Promise<FfmpegStatusInfo> {
  return invoke<FfmpegStatusInfo>("check_ffmpeg_status");
}

/** 下载 FFmpeg 到本地 ({data_dir}/zffmpeg/ffmpeg), 完成后返回最新状态 */
export async function downloadFfmpeg(): Promise<FfmpegStatusInfo> {
  return invoke<FfmpegStatusInfo>("download_ffmpeg");
}

// ============================================================
// Event listeners
// ============================================================

export function onEncodeProgress(
  handler: (progress: EncodeProgress) => void
): Promise<UnlistenFn> {
  return listen<EncodeProgress>("encode://progress", (event) => {
    handler(event.payload);
  });
}

export function onEncodeComplete(
  handler: (result: EncodeResult) => void
): Promise<UnlistenFn> {
  return listen<EncodeResult>("encode://complete", (event) => {
    handler(event.payload);
  });
}

export function onEncodeError(
  handler: (error: { jobId: string; error: string }) => void
): Promise<UnlistenFn> {
  return listen<{ jobId: string; error: string }>(
    "encode://error",
    (event) => {
      handler(event.payload);
    }
  );
}

export function onQueueUpdated(
  handler: (status: QueueStatus) => void
): Promise<UnlistenFn> {
  return listen<QueueStatus>("queue://updated", (event) => {
    handler(event.payload);
  });
}

export function onFfmpegDownloadProgress(
  handler: (percentage: number) => void
): Promise<UnlistenFn> {
  return listen<number>("ffmpeg://download-progress", (event) => {
    handler(event.payload);
  });
}

export function onFfmpegReady(
  handler: (info: FfmpegStatusInfo) => void
): Promise<UnlistenFn> {
  return listen<FfmpegStatusInfo>("ffmpeg://ready", (event) => {
    handler(event.payload);
  });
}

export function onFfmpegError(
  handler: (info: FfmpegStatusInfo) => void
): Promise<UnlistenFn> {
  return listen<FfmpegStatusInfo>("ffmpeg://error", (event) => {
    handler(event.payload);
  });
}
