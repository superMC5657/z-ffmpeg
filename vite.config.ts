import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import path from "path";
import { readFileSync } from "fs";

// 从 package.json 读取应用版本号，以便 UI 可以复用展示而无需重复定义
const pkg = JSON.parse(
  readFileSync(path.resolve(import.meta.dirname, "./package.json"), "utf-8")
);

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [react(), tailwindcss()],

  // 在构建时注入 package.json 中的版本号
  define: {
    __APP_VERSION__: JSON.stringify(pkg.version),
  },

  // index.html 位于 src/ 目录，因此 Vite 根目录设为 src
  root: "src",

  resolve: {
    alias: {
      "@": path.resolve(import.meta.dirname, "./src"),
    },
  },

  // 阻止 Vite 清屏，以免掩盖 Rust 的错误输出
  clearScreen: false,

  server: {
    // Tauri 期望固定端口，如果该端口不可用则直接报错失败
    strictPort: true,
    // 仅监听本地 localhost
    host: "localhost",
    port: 1430,
  },

  // 以 TAURI_ 开头的环境变量将暴露给 Tauri 源码
  envPrefix: ["VITE_", "TAURI_"],

  build: {
    // 将打包产物输出到项目根目录的 dist/，供 Tauri 的 frontendDist 使用
    outDir: "../dist",
    emptyOutDir: true,
    // Tauri v2 在 Windows 上使用 Chromium (Edge WebView2)，在 macOS/Linux 上使用 WebKit
    target: process.env.TAURI_ENV_PLATFORM === "windows" ? "chrome120" : "safari16",
    // Debug 调试构建时不压缩代码
    minify: !process.env.TAURI_DEBUG ? "esbuild" : false,
    // Debug 调试构建时生成 SourceMap
    sourcemap: !!process.env.TAURI_DEBUG,
  },
});
