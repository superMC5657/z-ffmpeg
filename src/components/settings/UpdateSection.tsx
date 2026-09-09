import { useState } from "react";
import { RefreshCw, Download, Loader2, CheckCircle2 } from "lucide-react";
import { useToastStore } from "@/store/toastStore";
import Card from "@/components/layout/Card";
import { isTauriRuntime } from "@/lib/utils";

export default function UpdateSection() {
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

  return (
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
  );
}
