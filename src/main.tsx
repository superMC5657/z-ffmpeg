import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";
import { initTheme, watchSystemTheme } from "./lib/theme";
import { initZoom } from "./lib/zoom";
import { initZLog } from "./lib/z-log";

// 渲染前同步应用主题与缩放偏好，避免首帧闪跳；系统外观变化时跟随
initTheme();
watchSystemTheme();
initZoom();
// 统一日志：DEV 镜像 console + 全局错误上报 Rust 落盘；失败不阻塞启动
void initZLog();

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
