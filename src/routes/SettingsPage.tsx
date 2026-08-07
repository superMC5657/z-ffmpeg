import { useEffect, useState } from "react";
import {
  Settings as SettingsIcon,
  Layers,
  Gauge,
  RefreshCw,
  Download,
  Loader2,
  CheckCircle2,
} from "lucide-react";
import {
  checkFfmpegStatus,
  downloadFfmpeg,
  onFfmpegDownloadProgress,
  onFfmpegReady,
} from "@/lib/tauri";
import type { UnlistenFn } from "@tauri-apps/api/event";
import type { FfmpegStatusInfo } from "@/types";
import { useSystemStore } from "@/store/systemStore";
import { useQueueStore } from "@/store/queueStore";
import { useToastStore } from "@/store/toastStore";
import PageHeader from "@/components/layout/PageHeader";
import { isTauriRuntime } from "@/lib/utils";

export default function SettingsPage() {
  const [ffmpeg, setFfmpeg] = useState<FfmpegStatusInfo | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [downloadProgress, setDownloadProgress] = useState<number | null>(null);
  const [downloadError, setDownloadError] = useState<string | null>(null);
  const hwAccels = useSystemStore((s) => s.hwAccels);
  const loadingHw = useSystemStore((s) => s.loading);
  const fetchHwAccels = useSystemStore((s) => s.fetchHwAccels);
  const [loading, setLoading] = useState(true);

  // Shared with Queue page — editing here syncs there and vice versa
  const maxConcurrent = useQueueStore((s) => s.maxConcurrent);
  const maxConcurrentLoaded = useQueueStore((s) => s.maxConcurrentLoaded);
  const fetchMaxConcurrent = useQueueStore((s) => s.fetchMaxConcurrent);
  const updateMaxConcurrent = useQueueStore((s) => s.updateMaxConcurrent);
  const [savingConcurrent, setSavingConcurrent] = useState(false);

  // VMAF 段数设置（0 = 全量对比，N = N 段 × 5 秒均匀采样），队列页计算按钮用
  const vmafSegments = useQueueStore((s) => s.vmafSegments);
  const vmafSegmentsLoaded = useQueueStore((s) => s.vmafSegmentsLoaded);
  const fetchVmafSegments = useQueueStore((s) => s.fetchVmafSegments);
  const updateVmafSegments = useQueueStore((s) => s.updateVmafSegments);
  const [savingVmaf, setSavingVmaf] = useState(false);

  useEffect(() => {
    fetchHwAccels();
    checkFfmpegStatus()
      .then(setFfmpeg)
      .catch(() => {})
      .finally(() => setLoading(false));
    fetchMaxConcurrent();
    fetchVmafSegments();
  }, []);

  // Listen for FFmpeg download progress & completion events
  useEffect(() => {
    if (!isTauriRuntime()) return;
    const unlisteners: UnlistenFn[] = [];
    onFfmpegDownloadProgress((p) => setDownloadProgress(p))
      .then((u) => unlisteners.push(u))
      .catch(() => {});
    onFfmpegReady(() => {
      setDownloading(false);
      setDownloadProgress(null);
      setDownloadError(null);
      checkFfmpegStatus().then(setFfmpeg).catch(() => {});
    })
      .then((u) => unlisteners.push(u))
      .catch(() => {});
    return () => {
      unlisteners.forEach((u) => u());
    };
  }, []);

  const handleDownloadFfmpeg = async () => {
    if (!isTauriRuntime()) {
      setDownloadError("浏览器环境不支持下载 FFmpeg");
      return;
    }
    setDownloading(true);
    setDownloadError(null);
    setDownloadProgress(0);
    try {
      const info = await downloadFfmpeg();
      setFfmpeg(info);
      setDownloading(false);
      setDownloadProgress(null);
    } catch (e) {
      setDownloading(false);
      setDownloadError(e instanceof Error ? e.message : String(e));
    }
  };

  const handleConcurrentChange = async (value: number) => {
    setSavingConcurrent(true);
    try {
      const saved = await updateMaxConcurrent(value);
      useToastStore.getState().showToast(
        `队列并发数已更新为 ${saved}`,
        "success"
      );
    } catch (e) {
      useToastStore.getState().showToast(
        `保存失败: ${e instanceof Error ? e.message : String(e)}`,
        "error"
      );
    } finally {
      setSavingConcurrent(false);
    }
  };

  const handleVmafSegmentsChange = async (value: number) => {
    setSavingVmaf(true);
    try {
      const saved = await updateVmafSegments(value);
      useToastStore.getState().showToast(
        saved === 0 ? "VMAF 已切换为全量对比" : `VMAF 采样段数已更新为 ${saved}`,
        "success"
      );
    } catch (e) {
      useToastStore.getState().showToast(
        `保存失败: ${e instanceof Error ? e.message : String(e)}`,
        "error"
      );
    } finally {
      setSavingVmaf(false);
    }
  };

  // ---- Update checking (Tauri only; browsers have no updater) ----
  const [updating, setUpdating] = useState(false);
  const [updateInfo, setUpdateInfo] = useState<{
    available: boolean;
    version: string | null;
    downloading: boolean;
    progress: number;
    error: string | null;
    installed: boolean;
    checked: boolean;
  }>({
    available: false,
    version: null,
    downloading: false,
    progress: 0,
    error: null,
    installed: false,
    checked: false,
  });

  const handleCheckUpdate = async () => {
    if (!isTauriRuntime()) {
      setUpdateInfo((s) => ({
        ...s,
        error: "浏览器环境不支持自动更新",
        checked: true,
      }));
      return;
    }
    setUpdating(true);
    setUpdateInfo((s) => ({
      ...s,
      error: null,
      available: false,
      installed: false,
    }));
    try {
      const { check } = await import("@tauri-apps/plugin-updater");
      const update = await check();
      if (!update) {
        setUpdateInfo((s) => ({
          ...s,
          available: false,
          version: null,
          error: null,
          checked: true,
        }));
      } else {
        setUpdateInfo((s) => ({
          ...s,
          available: true,
          version: update.version,
          error: null,
          checked: true,
        }));
      }
    } catch (e) {
      setUpdateInfo((s) => ({
        ...s,
        error: `检查更新失败: ${e instanceof Error ? e.message : String(e)}`,
        checked: true,
      }));
    } finally {
      setUpdating(false);
    }
  };

  const handleDownloadUpdate = async () => {
    if (!updateInfo.available) return;
    setUpdateInfo((s) => ({ ...s, downloading: true, error: null }));
    try {
      const { check } = await import("@tauri-apps/plugin-updater");
      const { relaunch } = await import("@tauri-apps/plugin-process");
      const update = await check();
      if (!update) throw new Error("未找到更新");
      let received = 0;
      let totalBytes: number | null = null;
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") {
          received = 0;
          totalBytes = event.data.contentLength ?? null;
          setUpdateInfo((s) => ({ ...s, progress: 0 }));
        } else if (event.event === "Progress") {
          received += event.data.chunkLength;
          if (totalBytes && totalBytes > 0) {
            setUpdateInfo((s) => ({
              ...s,
              progress: Math.min(100, Math.round((received / totalBytes!) * 100)),
            }));
          }
        }
      });
      setUpdateInfo((s) => ({ ...s, downloading: false, installed: true }));
      useToastStore.getState().showToast("更新已安装,正在重启...", "success");
      // Windows 上 install 已完成,重启应用生效
      await relaunch();
    } catch (e) {
      setUpdateInfo((s) => ({
        ...s,
        downloading: false,
        error: `下载失败: ${e instanceof Error ? e.message : String(e)}`,
      }));
    }
  };

  if (loading || loadingHw) {
    return (
      <div className="mx-auto max-w-5xl space-y-8">
        <PageHeader
          icon={SettingsIcon}
          title="设置"
          description="系统信息与应用配置"
        />
        <p className="text-sm text-muted-foreground">加载中...</p>
      </div>
    );
  }

  return (
    <div className="mx-auto max-w-5xl space-y-8">
      <PageHeader
        icon={SettingsIcon}
        title="设置"
        description="系统信息与应用配置"
      />

      {/* Queue Settings */}
      <div className="rounded-xl border border-border bg-card p-5 shadow-sm">
        <h2 className="mb-3 flex items-center gap-2 text-[15px] font-semibold">
          <Layers className="h-4 w-4 text-primary" />
          队列设置
        </h2>
        <div className="flex flex-wrap items-center gap-4">
          <div>
            <label className="mb-1.5 block text-[13px] font-medium text-muted-foreground">
              最大并发编码任务数
            </label>
            <div className="flex items-center gap-3">
              <input
                type="number"
                min={1}
                max={16}
                value={maxConcurrentLoaded ? maxConcurrent : ""}
                onChange={(e) => {
                  const v = parseInt(e.target.value);
                  if (!Number.isNaN(v) && v >= 1 && v <= 16) {
                    handleConcurrentChange(v);
                  }
                }}
                placeholder={maxConcurrentLoaded ? undefined : "加载中..."}
                disabled={!maxConcurrentLoaded || savingConcurrent}
                className="w-24 rounded-lg border border-border bg-accent/60 px-3 py-2 text-sm transition-shadow focus:border-primary focus:ring-2 focus:ring-primary/20 focus:outline-none disabled:opacity-50"
              />
              <span className="text-[13px] text-muted-foreground">
                个任务同时编码 (1-16) {savingConcurrent ? "· 保存中..." : ""}
              </span>
            </div>
            <p className="mt-2 text-xs text-muted-foreground">
              与队列页同步：任一处修改立即生效并保存。建议根据 CPU/GPU 性能调整：硬件加速下 2-4 个即可占满显卡，软件编码可适当调高。
            </p>
          </div>
        </div>
      </div>

      {/* VMAF Settings */}
      <div className="rounded-xl border border-border bg-card p-5 shadow-sm">
        <h2 className="mb-3 flex items-center gap-2 text-[15px] font-semibold">
          <Gauge className="h-4 w-4 text-primary" />
          VMAF 质量评估
        </h2>
        <div className="flex flex-wrap items-center gap-4">
          <div>
            <label className="mb-1.5 block text-[13px] font-medium text-muted-foreground">
              采样段数
            </label>
            <div className="flex items-center gap-3">
              <input
                type="number"
                min={0}
                max={32}
                value={vmafSegmentsLoaded ? vmafSegments : ""}
                onChange={(e) => {
                  const v = parseInt(e.target.value);
                  if (!Number.isNaN(v) && v >= 0 && v <= 32) {
                    handleVmafSegmentsChange(v);
                  }
                }}
                placeholder={vmafSegmentsLoaded ? undefined : "加载中..."}
                disabled={!vmafSegmentsLoaded || savingVmaf}
                className="w-24 rounded-lg border border-border bg-accent/60 px-3 py-2 text-sm transition-shadow focus:border-primary focus:ring-2 focus:ring-primary/20 focus:outline-none disabled:opacity-50"
              />
              <span className="text-[13px] text-muted-foreground">
                段 × 5 秒 (0-32) {savingVmaf ? "· 保存中..." : ""}
              </span>
            </div>
            <p className="mt-2 text-xs text-muted-foreground">
              设为 <b>0</b>：全量对比（逐帧计算，结果最精确，但耗时长，与视频时长成正比）。<br />
              设为 <b>N</b>：从整个视频均匀取 N 段 × 5 秒计算并取平均，几秒到几十秒完成。修改立即生效并保存，队列页「VMAF」按钮按此设置计算。
            </p>
          </div>
        </div>
      </div>

      {/* FFmpeg Status */}
      <div className="rounded-xl border border-border bg-card p-5 shadow-sm">
        <div className="mb-3 flex flex-wrap items-center justify-between gap-3">
          <h2 className="text-[15px] font-semibold">FFmpeg 状态</h2>
          {ffmpeg?.status === "not-installed" && !downloading && (
            <button
              onClick={handleDownloadFfmpeg}
              className="flex items-center gap-2 rounded-lg bg-gradient-brand px-4 py-2 text-[13px] font-medium text-white shadow-md shadow-primary/20 transition-all hover:brightness-110 active:scale-95"
            >
              <Download className="h-4 w-4" />
              下载 FFmpeg
            </button>
          )}
        </div>
        {ffmpeg && (
          <div className="space-y-2 text-sm">
            <div className="flex items-center gap-2">
              <div className={`h-2.5 w-2.5 rounded-full ${ffmpeg.status === "installed" ? "bg-success" : "bg-destructive"}`} />
              <span>{ffmpeg.status === "installed" ? "已安装" : "未安装"}</span>
              {downloading && (
                <span className="flex items-center gap-1.5 text-[13px] text-muted-foreground">
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  正在下载并安装到本地...
                </span>
              )}
            </div>
            {ffmpeg.version && (
              <p className="text-[13px] text-muted-foreground">{ffmpeg.version}</p>
            )}
            {ffmpeg.path && (
              <p className="text-[13px] text-muted-foreground truncate">路径: {ffmpeg.path}</p>
            )}
            {downloading && (
              <div className="flex items-center gap-3">
                <div className="h-2 flex-1 overflow-hidden rounded-full bg-accent">
                  <div
                    className="h-full rounded-full bg-gradient-brand transition-all duration-200"
                    style={{ width: `${downloadProgress ?? 0}%` }}
                  />
                </div>
                <span className="w-12 text-right text-[13px] text-muted-foreground">
                  {Math.round(downloadProgress ?? 0)}%
                </span>
              </div>
            )}
            {downloadError && (
              <p className="text-[13px] text-destructive break-all">{downloadError}</p>
            )}
          </div>
        )}
      </div>

      {/* Hardware Accelerators */}
      <div className="rounded-xl border border-border bg-card p-5 shadow-sm">
        <h2 className="mb-3 text-[15px] font-semibold">硬件加速器</h2>
        <div className="grid gap-3 sm:grid-cols-2">
          {hwAccels.map((hw) => (
            <div
              key={hw.device}
              className={`rounded-lg border p-3 ${
                hw.available
                  ? "border-success/30 bg-success/5"
                  : "border-border bg-accent/30"
              }`}
            >
              <div className="flex items-center gap-2">
                <div className={`h-2 w-2 rounded-full ${hw.available ? "bg-success" : "bg-muted-foreground/30"}`} />
                <span className="text-sm font-semibold">{hw.device}</span>
              </div>
              <p className="mt-1 text-[13px] text-muted-foreground">
                {hw.available ? hw.deviceName : "未检测到"}
              </p>
              {hw.supportedCodecs && hw.supportedCodecs.length > 0 && (
                <div className="mt-1.5 flex flex-wrap gap-1">
                  {hw.supportedCodecs.map((c: {codec: string; encoder: string}) => (
                    <span key={c.codec} className="rounded bg-accent px-2 py-0.5 text-[13px] text-muted-foreground">
                      {c.codec.toUpperCase()}
                    </span>
                  ))}
                </div>
              )}
            </div>
          ))}
        </div>
      </div>
      {/* Software Updates */}
      <div className="rounded-xl border border-border bg-card p-5 shadow-sm">
        <h2 className="mb-3 text-[15px] font-semibold">软件更新</h2>
        <div className="flex flex-wrap items-center gap-3">
          <button
            onClick={handleCheckUpdate}
            disabled={updating || updateInfo.downloading}
            className="flex items-center gap-2 rounded-lg border border-border bg-accent/60 px-4 py-2 text-[14px] font-medium transition-all hover:border-primary/40 hover:text-foreground disabled:cursor-not-allowed disabled:opacity-50"
          >
            {updating ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <RefreshCw className="h-4 w-4" />
            )}
            检查更新
          </button>

          {updateInfo.available && !updateInfo.downloading && !updateInfo.installed && (
            <div className="flex flex-wrap items-center gap-3">
              <span className="text-[13px] text-muted-foreground">
                发现新版本 v{updateInfo.version}
              </span>
              <button
                onClick={handleDownloadUpdate}
                className="flex items-center gap-2 rounded-lg bg-gradient-brand px-4 py-2 text-[14px] font-medium text-white shadow-md shadow-primary/20 transition-all hover:brightness-110 active:scale-95"
              >
                <Download className="h-4 w-4" />
                下载并安装
              </button>
            </div>
          )}

          {updateInfo.downloading && (
            <div className="flex min-w-56 flex-1 items-center gap-3">
              <span className="text-[13px] text-muted-foreground">
                下载中 {updateInfo.progress}%
              </span>
              <div className="h-2 flex-1 overflow-hidden rounded-full bg-accent">
                <div
                  className="h-full rounded-full bg-gradient-brand transition-all duration-300"
                  style={{ width: `${updateInfo.progress}%` }}
                />
              </div>
            </div>
          )}

          {updateInfo.installed && (
            <span className="flex items-center gap-1.5 text-[13px] font-medium text-success">
              <CheckCircle2 className="h-4 w-4" />
              已安装,正在重启应用...
            </span>
          )}

          {updateInfo.checked &&
            !updateInfo.available &&
            !updateInfo.downloading &&
            !updateInfo.installed &&
            !updateInfo.error &&
            !updating && (
              <span className="text-[13px] text-muted-foreground">
                已是最新版本
              </span>
            )}

          {updateInfo.error && (
            <span className="text-[13px] text-destructive">
              {updateInfo.error}
            </span>
          )}
        </div>
      </div>
    </div>
  );
}
