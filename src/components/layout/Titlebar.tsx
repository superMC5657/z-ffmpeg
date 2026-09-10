import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isTauriRuntime } from "@/lib/utils";
import appIcon from "@/assets/app-icon.png";

/**
 * Windows 11 Fluent 风格窗口三元控制按钮（最小化、最大化/向下还原、关闭）
 */
function WindowsWindowControls() {
  const [isMaximized, setIsMaximized] = useState(false);
  const appWindow = isTauriRuntime() ? getCurrentWindow() : null;

  useEffect(() => {
    if (!appWindow) return;

    // 初始化最大化状态
    appWindow.isMaximized().then(setIsMaximized).catch(() => {});

    // 监听窗口尺寸变化（如双击标题栏最大化/还原、拖拽贴边等）
    const unlistenPromise = appWindow.listen("tauri://resize", async () => {
      try {
        const max = await appWindow.isMaximized();
        setIsMaximized(max);
      } catch {
        // ignore
      }
    });

    return () => {
      unlistenPromise.then((unlisten) => unlisten()).catch(() => {});
    };
  }, [appWindow]);

  const handleMinimize = () => {
    if (appWindow) {
      appWindow.minimize().catch(() => {});
    }
  };

  const handleToggleMaximize = async () => {
    if (appWindow) {
      try {
        await appWindow.toggleMaximize();
        const max = await appWindow.isMaximized();
        setIsMaximized(max);
      } catch {
        // ignore
      }
    } else {
      setIsMaximized((prev) => !prev);
    }
  };

  const handleClose = () => {
    if (appWindow) {
      appWindow.close().catch(() => {});
    }
  };

  return (
    <div className="titlebar-no-drag flex h-full items-center select-none">
      {/* 最小化 */}
      <button
        type="button"
        aria-label="最小化窗口"
        title="最小化"
        onClick={handleMinimize}
        className="flex h-full w-11 items-center justify-center text-secondary transition-colors duration-150 hover:bg-foreground/[0.07] hover:text-foreground active:bg-foreground/[0.12] dark:hover:bg-white/[0.09] dark:active:bg-white/[0.14]"
      >
        <svg className="h-[10px] w-[10px]" viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1">
          <line x1="0" y1="5" x2="10" y2="5" />
        </svg>
      </button>

      {/* 最大化 / 向下还原 */}
      <button
        type="button"
        aria-label={isMaximized ? "向下还原窗口" : "最大化窗口"}
        title={isMaximized ? "向下还原" : "最大化"}
        onClick={handleToggleMaximize}
        className="flex h-full w-11 items-center justify-center text-secondary transition-colors duration-150 hover:bg-foreground/[0.07] hover:text-foreground active:bg-foreground/[0.12] dark:hover:bg-white/[0.09] dark:active:bg-white/[0.14]"
      >
        {isMaximized ? (
          // 还原图标：双层叠加窗口
          <svg className="h-[10px] w-[10px]" viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1">
            <path d="M2.5 2.5V0.5h7v7H7.5" />
            <rect x="0.5" y="2.5" width="7" height="7" />
          </svg>
        ) : (
          // 最大化图标：单层方框
          <svg className="h-[10px] w-[10px]" viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1">
            <rect x="0.5" y="0.5" width="9" height="9" />
          </svg>
        )}
      </button>

      {/* 关闭 */}
      <button
        type="button"
        aria-label="关闭窗口"
        title="关闭"
        onClick={handleClose}
        className="flex h-full w-11 items-center justify-center text-secondary transition-colors duration-150 hover:bg-[#e81123] hover:text-white active:bg-[#c42b1c] active:text-white"
      >
        <svg className="h-[10px] w-[10px]" viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1.1">
          <line x1="0.5" y1="0.5" x2="9.5" y2="9.5" />
          <line x1="9.5" y1="0.5" x2="0.5" y2="9.5" />
        </svg>
      </button>
    </div>
  );
}

export default function Titlebar() {
  return (
    <header
      data-tauri-drag-region
      className="titlebar-drag-region relative z-20 flex h-12 shrink-0 items-center border-b border-hairline bg-sidebar backdrop-blur-2xl"
    >
      {/* 左侧应用图标 */}
      <div className="flex flex-1 items-center pl-3.5">
        <div className="flex items-center gap-2 select-none">
          <img src={appIcon} alt="ZFFmpeg" className="h-4 w-4 rounded-xs pointer-events-none" />
        </div>
      </div>

      {/* 居中应用名与版本号 */}
      <div className="pointer-events-none absolute left-1/2 flex -translate-x-1/2 items-center gap-2 select-none">
        <span className="text-[13px] font-semibold tracking-tight">ZFFmpeg</span>
        <span className="rounded-full bg-fill px-2 py-0.5 text-[10px] font-medium leading-none text-secondary tabular-nums">
          v{__APP_VERSION__}
        </span>
      </div>

      {/* 右侧 Windows 三元窗口控制按钮 */}
      <div className="flex flex-1 items-center justify-end h-full">
        <WindowsWindowControls />
      </div>
    </header>
  );
}
