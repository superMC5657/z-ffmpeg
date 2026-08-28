import { useEffect, useState } from "react";
import { Monitor, Moon, Sun } from "lucide-react";
import { getStoredPref, setThemePref, type ThemePref } from "@/lib/theme";
import { cn } from "@/lib/utils";

const OPTIONS: { value: ThemePref; icon: typeof Sun; label: string }[] = [
  { value: "light", icon: Sun, label: "浅色" },
  { value: "dark", icon: Moon, label: "深色" },
  { value: "system", icon: Monitor, label: "跟随系统" },
];

/** macOS 分段控件式主题切换（浅色 / 深色 / 跟随系统） */
export default function ThemeToggleButton({ compact = false }: { compact?: boolean }) {
  const [pref, setPref] = useState<ThemePref>(() => getStoredPref());

  useEffect(() => {
    // 多处控件共享同一偏好：监听 storage 变化保持同步
    const onStorage = (e: StorageEvent) => {
      if (e.key === "zffmpeg-theme") setPref(getStoredPref());
    };
    window.addEventListener("storage", onStorage);
    return () => window.removeEventListener("storage", onStorage);
  }, []);

  if (compact) {
    const next = pref === "dark" ? "light" : "dark";
    const Icon = pref === "system" ? Monitor : pref === "dark" ? Moon : Sun;
    return (
      <button
        aria-label="切换深浅主题"
        title={pref === "dark" ? "切换到浅色" : "切换到深色"}
        onClick={() => {
          setThemePref(next);
          setPref(next);
        }}
        className="flex h-7 w-7 items-center justify-center rounded-lg text-secondary transition-colors hover:bg-fill-strong hover:text-foreground"
      >
        <Icon className="h-4 w-4" />
      </button>
    );
  }

  return (
    <div
      role="radiogroup"
      aria-label="外观"
      className="flex items-center gap-0.5 rounded-lg bg-fill p-0.5"
    >
      {OPTIONS.map(({ value, icon: Icon, label }) => (
        <button
          key={value}
          role="radio"
          aria-checked={pref === value}
          title={label}
          onClick={() => {
            setThemePref(value);
            setPref(value);
          }}
          className={cn(
            "flex h-7 w-9 items-center justify-center rounded-[7px] transition-all",
            pref === value
              ? "bg-surface text-foreground shadow-sm ring-1 ring-black/5"
              : "text-secondary hover:text-foreground"
          )}
        >
          <Icon className="h-3.5 w-3.5" />
        </button>
      ))}
    </div>
  );
}
