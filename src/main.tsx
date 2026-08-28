import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";
import { initTheme, watchSystemTheme } from "./lib/theme";

// 渲染前同步应用主题，避免首帧闪跳；系统外观变化时跟随
initTheme();
watchSystemTheme();

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
