export type ThemePref = "light" | "dark" | "system";

const STORAGE_KEY = "zffmpeg-theme";
const media = window.matchMedia("(prefers-color-scheme: dark)");

export function getStoredPref(): ThemePref {
  const v = localStorage.getItem(STORAGE_KEY);
  return v === "light" || v === "dark" || v === "system" ? v : "system";
}

function resolve(pref: ThemePref): boolean {
  return pref === "dark" || (pref === "system" && media.matches);
}

/** 应用主题到 <html>；首帧调用（无过渡），切换时带柔和过渡 */
export function applyTheme(pref: ThemePref, animate = false) {
  const root = document.documentElement;
  const run = () => {
    root.classList.toggle("dark", resolve(pref));
    if (animate) {
      root.classList.add("theme-anim");
      window.setTimeout(() => root.classList.remove("theme-anim"), 300);
    }
  };
  if (animate) requestAnimationFrame(run);
  else run();
}

/** 应用启动时同步调用，避免主题闪跳 */
export function initTheme(): ThemePref {
  const pref = getStoredPref();
  applyTheme(pref);
  return pref;
}

export function setThemePref(pref: ThemePref) {
  localStorage.setItem(STORAGE_KEY, pref);
  applyTheme(pref, true);
}

/** 跟随系统：pref 为 system 时监听系统外观变化 */
export function watchSystemTheme() {
  media.addEventListener("change", () => {
    if (getStoredPref() === "system") applyTheme("system");
  });
}
