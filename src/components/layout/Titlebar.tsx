import { Minus, Plus, X } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isTauriRuntime } from "@/lib/utils";
import ThemeToggleButton from "./ThemeToggleButton";

interface Light {
  base: string;
  glyph: React.ReactNode;
  onClick: () => void;
  label: string;
}

/** macOS 红绿灯窗口控制（仅 Tauri 运行时渲染，hover 浮现符号） */
function TrafficLights() {
  const appWindow = isTauriRuntime() ? getCurrentWindow() : null;
  if (!appWindow) return null;

  const lights: Light[] = [
    {
      base: "bg-[#ff5f57]",
      glyph: <X className="h-2.5 w-2.5 stroke-[3]" />,
      onClick: () => appWindow.close(),
      label: "关闭窗口",
    },
    {
      base: "bg-[#febc2e]",
      glyph: <Minus className="h-2.5 w-2.5 -translate-y-2.5 stroke-[3]" />,
      onClick: () => appWindow.minimize(),
      label: "最小化窗口",
    },
    {
      base: "bg-[#28c840]",
      glyph: <Plus className="h-2.5 w-2.5 stroke-[3]" />,
      onClick: () => appWindow.toggleMaximize(),
      label: "最大化窗口",
    },
  ];

  return (
    <div className="titlebar-no-drag group flex items-center gap-2">
      {lights.map(({ base, glyph, onClick, label }) => (
        <button
          key={label}
          aria-label={label}
          onClick={onClick}
          className={`flex h-3 w-3 items-center justify-center rounded-full text-black/55 ring-1 ring-black/10 transition-colors [&>svg]:opacity-0 [&>svg]:transition-opacity group-hover:[&>svg]:opacity-100 ${base}`}
        >
          {glyph}
        </button>
      ))}
    </div>
  );
}

export default function Titlebar() {
  return (
    <header
      data-tauri-drag-region
      className="titlebar-drag-region relative z-20 flex h-12 shrink-0 items-center border-b border-hairline bg-sidebar backdrop-blur-2xl"
    >
      <div className="flex flex-1 items-center pl-4">
        <TrafficLights />
      </div>

      {/* 居中应用名 */}
      <div className="pointer-events-none absolute left-1/2 flex -translate-x-1/2 items-center gap-2">
        <span className="text-[13px] font-semibold tracking-tight">ZFFmpeg</span>
        <span className="rounded-full bg-fill px-2 py-0.5 text-[10px] font-medium leading-none text-secondary tabular-nums">
          v{__APP_VERSION__}
        </span>
      </div>

      <div className="flex flex-1 items-center justify-end pr-3">
        <ThemeToggleButton />
      </div>
    </header>
  );
}
