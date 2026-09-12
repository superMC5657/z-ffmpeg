import { isTauriRuntime } from "./utils";

export type ZoomLevel = "90" | "100" | "110" | "125";

export const ZOOM_OPTIONS: { value: ZoomLevel; label: string; factor: number }[] = [
  { value: "90", label: "90%", factor: 0.9 },
  { value: "100", label: "100%", factor: 1.0 },
  { value: "110", label: "110%", factor: 1.1 },
  { value: "125", label: "125%", factor: 1.25 },
];

const STORAGE_KEY = "z-ffmpeg-ui-zoom";

export function getStoredZoom(): ZoomLevel {
  if (typeof localStorage === "undefined") return "100";
  const v = localStorage.getItem(STORAGE_KEY);
  return v === "90" || v === "100" || v === "110" || v === "125" ? v : "100";
}

/**
 * 实际应用缩放比例：
 * 优先调用 Tauri getCurrentWebview().setZoom(factor)，保持原生 WebView2 字体渲染与 DPI 质感；
 * 若不在 Tauri 环境或调用失败，降级使用 CSS document.documentElement.style.zoom。
 */
export async function applyZoom(level: ZoomLevel) {
  const option = ZOOM_OPTIONS.find((o) => o.value === level) ?? ZOOM_OPTIONS[1];
  const factor = option.factor;

  let appliedViaTauri = false;
  if (isTauriRuntime()) {
    try {
      const { getCurrentWebview } = await import("@tauri-apps/api/webview");
      await getCurrentWebview().setZoom(factor);
      appliedViaTauri = true;
      if (typeof document !== "undefined") {
        document.documentElement.style.zoom = "";
      }
    } catch {
      // 降级使用 CSS zoom
    }
  }

  if (!appliedViaTauri && typeof document !== "undefined") {
    document.documentElement.style.zoom = `${factor}`;
  }
}

export function setZoomLevel(level: ZoomLevel) {
  if (typeof localStorage !== "undefined") {
    localStorage.setItem(STORAGE_KEY, level);
  }
  applyZoom(level);
}

/** 拦截并彻底禁用所有原生的 Ctrl/Cmd + / - / 0 及滚轮缩放快捷键 */
export function disableNativeZoomHotkeys() {
  if (typeof window === "undefined") return;

  const isZoomKey = (e: KeyboardEvent) => {
    if (!e.ctrlKey && !e.metaKey) return false;
    const zoomKeys = ["=", "+", "-", "_", "0"];
    const zoomCodes = ["Equal", "Minus", "Digit0", "NumpadAdd", "NumpadSubtract", "Numpad0"];
    return zoomKeys.includes(e.key) || zoomCodes.includes(e.code);
  };

  window.addEventListener(
    "keydown",
    (e) => {
      if (isZoomKey(e)) {
        e.preventDefault();
        e.stopPropagation();
      }
    },
    { capture: true }
  );

  window.addEventListener(
    "wheel",
    (e) => {
      if (e.ctrlKey || e.metaKey) {
        e.preventDefault();
        e.stopPropagation();
      }
    },
    { passive: false, capture: true }
  );
}

/** 应用启动时初始化缩放并禁用快捷键 */
export function initZoom(): ZoomLevel {
  disableNativeZoomHotkeys();
  const level = getStoredZoom();
  // 首帧在同步环境应用 CSS zoom，避免页面布局首跳
  const option = ZOOM_OPTIONS.find((o) => o.value === level) ?? ZOOM_OPTIONS[1];
  if (typeof document !== "undefined") {
    document.documentElement.style.zoom = `${option.factor}`;
  }
  applyZoom(level);
  return level;
}
