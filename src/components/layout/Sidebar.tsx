import { useEffect, useState } from "react";
import { NavLink, useLocation } from "react-router-dom";
import {
  Video,
  Layers,
  SlidersHorizontal,
  Settings,
  History,
} from "lucide-react";
import { cn } from "@/lib/utils";
import {
  checkFfmpegStatus,
  onFfmpegDownloadProgress,
  onFfmpegReady,
  onFfmpegError,
} from "@/lib/tauri";
import { useSystemStore } from "@/store/systemStore";
import { isTauriRuntime } from "@/lib/utils";
import type { UnlistenFn } from "@tauri-apps/api/event";
import type { FfmpegStatusInfo } from "@/types";

const navItems = [
  { to: "/", label: "编码", icon: Video },
  { to: "/presets", label: "预设", icon: SlidersHorizontal },
  { to: "/queue", label: "队列", icon: Layers },
  { to: "/history", label: "历史", icon: History },
  { to: "/settings", label: "设置", icon: Settings },
];

const FFMPEG_STATES: Record<string, { dot: string; text: string }> = {
  installed: { dot: "bg-success", text: "FFmpeg 已就绪" },
  checking: { dot: "bg-warning animate-pulse", text: "正在检测…" },
  downloading: { dot: "bg-accent animate-pulse", text: "正在下载…" },
  "not-installed": { dot: "bg-destructive", text: "FFmpeg 未安装" },
  error: { dot: "bg-destructive", text: "FFmpeg 异常" },
  browser: { dot: "bg-tertiary", text: "浏览器预览模式" },
};

function FfmpegStatusFooter() {
  const [info, setInfo] = useState<FfmpegStatusInfo | null>(null);

  useEffect(() => {
    checkFfmpegStatus()
      .then(setInfo)
      .catch(() => setInfo(null));

    if (!isTauriRuntime()) return;
    // 下载过程中显示"正在下载...",完成后用 ffmpeg://ready 的载荷
    // 直接刷新(设置页下载、其他入口下载都会同步到这里)。
    const unlisteners: UnlistenFn[] = [];
    onFfmpegDownloadProgress(() =>
      setInfo({
        status: "downloading",
        version: null,
        path: null,
        downloadProgress: null,
        error: null,
      })
    )
      .then((u) => unlisteners.push(u))
      .catch(() => {});
    onFfmpegReady((ready) => {
      setInfo(ready);
      // FFmpeg 刚就绪(如自动下载完成),硬件加速器需要基于新的
      // ffmpeg 重新检测;此处全局触发,所有页面共享同一份结果。
      useSystemStore.getState().fetchHwAccels(true);
    })
      .then((u) => unlisteners.push(u))
      .catch(() => {});
    onFfmpegError(() => {
      // 下载失败:重新拉取真实状态,避免 footer 卡在"正在下载..."
      // (失败时后端只 emit ffmpeg://error,不会 emit ffmpeg://ready)。
      checkFfmpegStatus()
        .then(setInfo)
        .catch(() => setInfo(null));
    })
      .then((u) => unlisteners.push(u))
      .catch(() => {});
    return () => {
      unlisteners.forEach((u) => u());
    };
  }, []);

  const key = info?.status ?? (isTauriRuntime() ? "checking" : "browser");
  const state = FFMPEG_STATES[key] ?? FFMPEG_STATES.error;

  return (
    <div className="flex items-center gap-2 px-2.5 py-1.5 text-[12px] text-secondary">
      <span className={cn("h-1.5 w-1.5 shrink-0 rounded-full", state.dot)} />
      <span className="truncate">{state.text}</span>
    </div>
  );
}

export default function Sidebar() {
  const location = useLocation();

  return (
    <aside className="flex w-[190px] shrink-0 flex-col border-r border-hairline bg-sidebar backdrop-blur-2xl">
      {/* System Settings 风格导航 */}
      <nav className="flex flex-1 flex-col gap-0.5 px-3 pt-5">
        {navItems.map(({ to, label, icon: Icon }) => {
          const isActive = to === "/"
            ? location.pathname === "/"
            : location.pathname.startsWith(to);
          return (
            <NavLink
              key={to}
              to={to}
              className={cn(
                "flex h-9 items-center gap-2.5 rounded-[8px] px-2.5 text-[13px] transition-colors",
                isActive
                  ? "bg-accent font-medium text-on-accent"
                  : "text-foreground hover:bg-fill-strong"
              )}
            >
              <Icon
                className={cn(
                  "h-4 w-4 shrink-0",
                  isActive ? "text-on-accent" : "text-secondary"
                )}
                strokeWidth={isActive ? 2.4 : 2}
              />
              {label}
            </NavLink>
          );
        })}
      </nav>

      {/* Footer */}
      <div className="border-t border-hairline px-1.5 py-2.5">
        <FfmpegStatusFooter />
      </div>
    </aside>
  );
}
