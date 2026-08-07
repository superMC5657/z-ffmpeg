import { Minus, Square, X } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isTauriRuntime } from "@/lib/utils";
import appIcon from "@/assets/app-icon.png";

export default function Titlebar() {
  const appWindow = isTauriRuntime() ? getCurrentWindow() : null;

  return (
    <div
      data-tauri-drag-region
      className="titlebar-drag-region flex h-10 shrink-0 items-center justify-between border-b border-border bg-card/70 backdrop-blur"
    >
      {/* App title */}
      <div className="flex items-center gap-2.5 pl-4">
        <img
          src={appIcon}
          alt="zffmpeg"
          draggable={false}
          className="h-6 w-6 shrink-0 rounded-md shadow-sm shadow-primary/30"
        />
        <span className="text-[13px] font-semibold tracking-tight">zffmpeg</span>
        <span className="rounded-full bg-accent px-2 py-0.5 text-[10px] font-medium leading-none text-muted-foreground">
          v{__APP_VERSION__}
        </span>
      </div>

      {/* Window controls (only inside Tauri; browsers have their own) */}
      {appWindow && (
        <div className="titlebar-no-drag flex h-full">
          <button
            onClick={() => appWindow.minimize()}
            className="flex h-full w-10 items-center justify-center text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
          >
            <Minus className="h-3.5 w-3.5" />
          </button>
          <button
            onClick={() => appWindow.toggleMaximize()}
            className="flex h-full w-10 items-center justify-center text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
          >
            <Square className="h-3 w-3" />
          </button>
          <button
            onClick={() => appWindow.close()}
            className="flex h-full w-10 items-center justify-center text-muted-foreground transition-colors hover:bg-destructive hover:text-white"
          >
            <X className="h-3.5 w-3.5" />
          </button>
        </div>
      )}
    </div>
  );
}
