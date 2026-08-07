import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import path from "path";
import { readFileSync } from "fs";

// Read the app version from package.json so UI can display it without duplication
const pkg = JSON.parse(
  readFileSync(path.resolve(import.meta.dirname, "./package.json"), "utf-8")
);

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [react(), tailwindcss()],

  // Inject the version from package.json at build time
  define: {
    __APP_VERSION__: JSON.stringify(pkg.version),
  },

  // index.html lives in src/, so the Vite root is src
  root: "src",

  resolve: {
    alias: {
      "@": path.resolve(import.meta.dirname, "./src"),
    },
  },

  // Prevent vite from obscuring Rust errors
  clearScreen: false,

  server: {
    // Tauri expects a fixed port, fail if that port is not available
    strictPort: true,
    // Allow Tauri to access the dev server
    host: true,
    port: 1420,
  },

  // Env variables starting with TAURI_ will be exposed to tauri's source code
  envPrefix: ["VITE_", "TAURI_"],

  build: {
    // Emit the bundle to the project root's dist/ for Tauri's frontendDist
    outDir: "../dist",
    emptyOutDir: true,
    // Tauri v2 uses Chromium on Windows (Edge WebView2) and WebKit on macOS/Linux
    target: process.env.TAURI_ENV_PLATFORM === "windows" ? "chrome120" : "safari16",
    // Don't minify for debug builds
    minify: !process.env.TAURI_DEBUG ? "esbuild" : false,
    // Produce sourcemaps for debug builds
    sourcemap: !!process.env.TAURI_DEBUG,
  },
});
