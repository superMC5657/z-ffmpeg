import { defineConfig } from "vitest/config";
import path from "path";

export default defineConfig({
  resolve: {
    alias: {
      "@": path.resolve(import.meta.dirname, "./src"),
    },
  },
  test: {
    // jsdom：store 测试依赖 localStorage/window（encoderStore 的 persist 中间件）
    environment: "jsdom",
    include: ["src/**/*.test.{ts,tsx}"],
  },
});
