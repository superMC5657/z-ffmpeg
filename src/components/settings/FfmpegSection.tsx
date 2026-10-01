import { useEffect } from "react";
import { Download, Loader2 } from "lucide-react";
import { useSystemStore } from "@/store/systemStore";
import Card from "@/components/layout/Card";
import AppleButton from "@/components/layout/AppleButton";
import LinearProgress from "@/components/layout/LinearProgress";
import { cn } from "@/lib/utils";

export default function FfmpegSection() {
  const ffmpeg = useSystemStore((s) => s.ffmpegStatus);
  const downloading = useSystemStore((s) => s.downloading);
  const downloadProgress = useSystemStore((s) => s.downloadProgress);
  const downloadError = useSystemStore((s) => s.downloadError);
  const downloadFfmpeg = useSystemStore((s) => s.downloadFfmpeg);
  const initFfmpeg = useSystemStore((s) => s.initFfmpeg);

  useEffect(() => {
    initFfmpeg();
  }, [initFfmpeg]);

  const handleDownloadFfmpeg = () => {
    void downloadFfmpeg();
  };

  return (
    <Card
      title="FFmpeg 状态"
      action={
        ffmpeg?.status === "not-installed" &&
        !downloading && (
          <AppleButton
            variant="primary"
            onClick={handleDownloadFfmpeg}
          >
            <Download className="h-3.5 w-3.5" />
            下载 FFmpeg
          </AppleButton>
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
              <LinearProgress
                value={downloadProgress ?? 0}
                className="flex-1"
              />
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
