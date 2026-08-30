import { useEffect, useState } from "react";
import { RefreshCw, Download, Loader2, CheckCircle2 } from "lucide-react";
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
import Card from "@/components/layout/Card";
import AppleInput from "@/components/layout/AppleInput";
import ThemeToggleButton from "@/components/layout/ThemeToggleButton";
import { isTauriRuntime } from "@/lib/utils";
import { cn } from "@/lib/utils";

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
      .catch((e) => {
        useToastStore.getState().showToast(
          `获取 FFmpeg 状态失败: ${e instanceof Error ? e.message : String(e)}`,
          "error"
        );
      })
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
      checkFfmpegStatus()
        .then(setFfmpeg)
        .catch((e) => {
          useToastStore.getState().showToast(
            `获取 FFmpeg 状态失败: ${e instanceof Error ? e.message : String(e)}`,
            "error"
          );
        });
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
      <div className="space-y-4">
        <PageHeader title="设置" description="系统信息与应用配置" />
        <div className="space-y-3">
          {[0, 1, 2].map((i) => (
            <div key={i} className="h-24 animate-pulse rounded-[14px] bg-fill/70" />
          ))}
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <PageHeader title="设置" description="系统信息与应用配置" />

      {/* Appearance */}
      <Card title="外观" description="浅色、深色或跟随系统，切换立即生效">
        <div className="flex items-center justify-between gap-4">
          <p className="text-[13px] leading-5 text-secondary">
            深浅主题在标题栏与设置页均可切换，跟随系统时自动适配外观变化。
          </p>
          <ThemeToggleButton />
        </div>
      </Card>

      {/* Queue Settings */}
      <Card title="队列设置">
        <div className="flex items-center justify-between gap-4">
          <div className="min-w-0">
            <p className="text-[13px] font-medium">最大并发编码任务数</p>
            <p className="mt-0.5 text-[12px] leading-5 text-secondary">
              与队列页同步，任一处修改立即生效并保存。硬件加速下 2-4
              个即可占满显卡，软件编码可适当调高。
            </p>
          </div>
          <div className="flex shrink-0 items-center gap-2">
            <AppleInput
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
              placeholder={maxConcurrentLoaded ? undefined : "…"}
              disabled={!maxConcurrentLoaded || savingConcurrent}
              className="w-16 text-center"
            />
            {savingConcurrent && (
              <Loader2 className="h-3.5 w-3.5 animate-spin text-tertiary" />
            )}
          </div>
        </div>
      </Card>

      {/* VMAF Settings */}
      <Card title="VMAF 质量评估">
        <div className="flex items-center justify-between gap-4">
          <div className="min-w-0">
            <p className="text-[13px] font-medium">采样段数</p>
            <p className="mt-0.5 text-[12px] leading-5 text-secondary">
              设为 <b>0</b>：全量对比（逐帧计算，最精确但耗时长）。设为{" "}
              <b>N</b>：均匀取 N 段 × 5 秒计算取平均，几秒到几十秒完成。队列页「VMAF」按钮按此设置计算。
            </p>
          </div>
          <div className="flex shrink-0 items-center gap-2">
            <AppleInput
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
              placeholder={vmafSegmentsLoaded ? undefined : "…"}
              disabled={!vmafSegmentsLoaded || savingVmaf}
              className="w-16 text-center"
            />
            {savingVmaf && (
              <Loader2 className="h-3.5 w-3.5 animate-spin text-tertiary" />
            )}
          </div>
        </div>
      </Card>

      {/* FFmpeg Status */}
      <Card
        title="FFmpeg 状态"
        action={
          ffmpeg?.status === "not-installed" &&
          !downloading && (
            <button
              onClick={handleDownloadFfmpeg}
              className="flex h-9 items-center gap-1.5 rounded-[9px] bg-accent px-4 text-[13px] font-medium text-on-accent shadow-sm transition-all hover:bg-accent-hover active:scale-[0.98]"
            >
              <Download className="h-3.5 w-3.5" />
              下载 FFmpeg
            </button>
          )
        }
      >
        {ffmpeg && (
          <div className="space-y-2.5 text-[13px]">
            <div className="flex items-center gap-2">
              <span
                className={cn(
                  "h-2 w-2 rounded-full",
                  ffmpeg.status === "installed" ? "bg-success" : "bg-destructive"
                )}
              />
              <span className="font-medium">
                {ffmpeg.status === "installed" ? "已安装" : "未安装"}
              </span>
              {downloading && (
                <span className="flex items-center gap-1.5 text-secondary">
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  正在下载并安装到本地...
                </span>
              )}
            </div>
            {ffmpeg.version && (
              <p className="tabular-nums text-secondary">{ffmpeg.version}</p>
            )}
            {ffmpeg.path && (
              <p className="truncate text-secondary" title={ffmpeg.path}>
                路径: {ffmpeg.path}
              </p>
            )}
            {downloading && (
              <div className="flex items-center gap-3">
                <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-fill-strong">
                  <div
                    className="h-full rounded-full bg-accent transition-all duration-200"
                    style={{ width: `${downloadProgress ?? 0}%` }}
                  />
                </div>
                <span className="w-10 text-right tabular-nums text-secondary">
                  {Math.round(downloadProgress ?? 0)}%
                </span>
              </div>
            )}
            {downloadError && (
              <p className="break-all text-[12px] text-destructive">
                {downloadError}
              </p>
            )}
          </div>
        )}
      </Card>

      {/* Hardware Accelerators */}
      <Card title="硬件加速器">
        <div className="grid gap-3 sm:grid-cols-2">
          {hwAccels.map((hw) => (
            <div
              key={hw.device}
              className={cn(
                "rounded-[9px] p-3.5",
                hw.available ? "bg-success/8" : "bg-fill"
              )}
            >
              <div className="flex items-center gap-2">
                <span
                  className={cn(
                    "h-2 w-2 rounded-full",
                    hw.available ? "bg-success" : "bg-tertiary"
                  )}
                />
                <span className="text-[13px] font-semibold">{hw.device}</span>
              </div>
              <p className="mt-1.5 text-[12px] leading-5 text-secondary">
                {hw.available ? hw.deviceName : "未检测到"}
              </p>
              {hw.supportedCodecs && hw.supportedCodecs.length > 0 && (
                <div className="mt-2 flex flex-wrap gap-1">
                  {hw.supportedCodecs.map((c: {codec: string; encoder: string}) => (
                    <span
                      key={c.codec}
                      className="rounded-md bg-surface px-2 py-0.5 text-[11px] font-medium text-secondary"
                    >
                      {c.codec.toUpperCase()}
                    </span>
                  ))}
                </div>
              )}
            </div>
          ))}
        </div>
      </Card>

      {/* Software Updates */}
      <Card title="软件更新">
        <div className="flex flex-wrap items-center gap-3">
          <button
            onClick={handleCheckUpdate}
            disabled={updating || updateInfo.downloading}
            className="flex h-9 items-center gap-1.5 rounded-[9px] bg-fill px-4 text-[13px] font-medium text-foreground transition-colors hover:bg-fill-strong active:scale-[0.98] disabled:cursor-default disabled:opacity-50"
          >
            {updating ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <RefreshCw className="h-3.5 w-3.5" />
            )}
            检查更新
          </button>

          {updateInfo.available && !updateInfo.downloading && !updateInfo.installed && (
            <div className="flex flex-wrap items-center gap-3">
              <span className="text-[13px] text-secondary">
                发现新版本 v{updateInfo.version}
              </span>
              <button
                onClick={handleDownloadUpdate}
                className="flex h-9 items-center gap-1.5 rounded-[9px] bg-accent px-4 text-[13px] font-medium text-on-accent shadow-sm transition-all hover:bg-accent-hover active:scale-[0.98]"
              >
                <Download className="h-3.5 w-3.5" />
                下载并安装
              </button>
            </div>
          )}

          {updateInfo.downloading && (
            <div className="flex min-w-56 flex-1 items-center gap-3">
              <span className="shrink-0 tabular-nums text-[13px] text-secondary">
                下载中 {updateInfo.progress}%
              </span>
              <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-fill-strong">
                <div
                  className="h-full rounded-full bg-accent transition-all duration-300"
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
              <span className="text-[13px] text-secondary">已是最新版本</span>
            )}

          {updateInfo.error && (
            <span className="text-[13px] text-destructive">
              {updateInfo.error}
            </span>
          )}
        </div>
      </Card>
    </div>
  );
}
