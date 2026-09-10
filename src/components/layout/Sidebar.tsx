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
import { useQueueStore } from "@/store/queueStore";
import { isTauriRuntime } from "@/lib/utils";
import type { UnlistenFn } from "@tauri-apps/api/event";
import type { FfmpegStatusInfo } from "@/types";

const navItems = [
  { to: "/", label: "转码工作台", icon: Video },
  { to: "/presets", label: "编码预设", icon: SlidersHorizontal },
  { to: "/queue", label: "任务队列", icon: Layers },
  { to: "/history", label: "转码历史", icon: History },
  { to: "/settings", label: "系统设置", icon: Settings },
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
      useSystemStore.getState().fetchHwAccels(true);
    })
      .then((u) => unlisteners.push(u))
      .catch(() => {});
    onFfmpegError(() => {
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
    <div className="flex items-center gap-2.5 rounded-xl bg-fill/35 px-3 py-2 border border-hairline/60 text-[13px] text-secondary">
      <span className={cn("h-2 w-2 shrink-0 rounded-full", state.dot)} />
      <span className="truncate font-medium">{state.text}</span>
    </div>
  );
}

export default function Sidebar() {
  const location = useLocation();
  const queueCount = useQueueStore((s) =>
    s.jobs.filter((j) => j.status === "Pending" || j.status === "Encoding").length
  );

  return (
    <aside className="flex w-[210px] shrink-0 flex-col border-r border-hairline bg-sidebar backdrop-blur-2xl select-none">
      {/* 导航项列表 */}
      <nav className="flex flex-1 flex-col gap-1.5 px-3 pt-5">
        {navItems.map(({ to, label, icon: Icon }) => {
          const isActive =
            to === "/"
              ? location.pathname === "/"
              : location.pathname.startsWith(to);

          return (
            <NavLink
              key={to}
              to={to}
              className={cn(
                "group flex h-11 items-center gap-3 rounded-xl px-3.5 text-[14px] font-medium transition-all",
                isActive
                  ? "bg-accent font-semibold text-on-accent shadow-xs"
                  : "text-foreground/85 hover:bg-fill/70 hover:text-foreground"
              )}
            >
              <Icon
                className={cn(
                  "h-4.5 w-4.5 shrink-0 transition-colors",
                  isActive
                    ? "text-on-accent"
                    : "text-secondary group-hover:text-foreground"
                )}
                strokeWidth={isActive ? 2.3 : 1.9}
              />
              <span>{label}</span>

              {to === "/queue" && queueCount > 0 && (
                <span
                  className={cn(
                    "ml-auto rounded-full px-2 py-0.5 text-[11px] font-semibold tabular-nums",
                    isActive
                      ? "bg-on-accent/20 text-on-accent"
                      : "bg-accent/15 text-accent"
                  )}
                >
                  {queueCount}
                </span>
              )}
            </NavLink>
          );
        })}
      </nav>

      {/* 底部 FFmpeg 就绪状态卡 */}
      <div className="border-t border-hairline p-3">
        <FfmpegStatusFooter />
      </div>
    </aside>
  );
}
