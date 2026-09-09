import { useEffect, useState } from "react";
import { Download, Loader2 } from "lucide-react";
import {
  checkFfmpegStatus,
  downloadFfmpeg,
  onFfmpegDownloadProgress,
  onFfmpegReady,
} from "@/lib/tauri";
import type { UnlistenFn } from "@tauri-apps/api/event";
import type { FfmpegStatusInfo } from "@/types";
import { useToastStore } from "@/store/toastStore";
import Card from "@/components/layout/Card";
import { isTauriRuntime } from "@/lib/utils";
import { cn } from "@/lib/utils";

export default function FfmpegSection() {
  const [ffmpeg, setFfmpeg] = useState<FfmpegStatusInfo | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [downloadProgress, setDownloadProgress] = useState<number | null>(null);
  const [downloadError, setDownloadError] = useState<string | null>(null);

  useEffect(() => {
    checkFfmpegStatus()
      .then(setFfmpeg)
      .catch((e) => {
        useToastStore.getState().showToast(
          `获取 FFmpeg 状态失败: ${e instanceof Error ? e.message : String(e)}`,
          "error"
        );
      });
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

  return (
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
      {ffmpeg ? (
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
      ) : (
        <p className="text-[13px] text-tertiary">正在获取 FFmpeg 状态…</p>
      )}
    </Card>
  );
}
