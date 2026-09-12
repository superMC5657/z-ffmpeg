import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";
import { initTheme, watchSystemTheme } from "./lib/theme";
import { initZoom } from "./lib/zoom";

// 渲染前同步应用主题与缩放偏好，避免首帧闪跳；系统外观变化时跟随
initTheme();
watchSystemTheme();
initZoom();

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
