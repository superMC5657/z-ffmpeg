import { useState } from "react";
import { RefreshCw, Download, CheckCircle2 } from "lucide-react";
import { useToastStore } from "@/store/toastStore";
import Card from "@/components/layout/Card";
import AppleButton from "@/components/layout/AppleButton";
import LinearProgress from "@/components/layout/LinearProgress";
import { isTauriRuntime, formatError } from "@/lib/utils";

export default function UpdateSection() {
  // ---- 检查更新（仅 Tauri 运行时有效；浏览器无更新器） ----
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
        error: `检查更新失败: ${formatError(e)}`,
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
        error: `下载失败: ${formatError(e)}`,
      }));
    }
  };

  return (
    <Card title="软件更新">
      <div className="flex flex-wrap items-center gap-3">
        <AppleButton
          variant="secondary"
          loading={updating}
          disabled={updateInfo.downloading}
          onClick={handleCheckUpdate}
        >
          {!updating && <RefreshCw className="h-3.5 w-3.5" />}
          检查更新
        </AppleButton>

        {updateInfo.available && !updateInfo.downloading && !updateInfo.installed && (
          <div className="flex flex-wrap items-center gap-3">
            <span className="text-[13px] text-secondary">
              发现新版本 v{updateInfo.version}
            </span>
            <AppleButton
              variant="primary"
              onClick={handleDownloadUpdate}
            >
              <Download className="h-3.5 w-3.5" />
              下载并安装
            </AppleButton>
          </div>
        )}

        {updateInfo.downloading && (
          <div className="flex min-w-56 flex-1 items-center gap-3">
            <span className="shrink-0 tabular-nums text-[13px] text-secondary">
              下载中 {updateInfo.progress}%
            </span>
            <LinearProgress value={updateInfo.progress} className="flex-1" />
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
