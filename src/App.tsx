import { HashRouter, Routes, Route, useLocation } from "react-router-dom";
import { useEffect } from "react";
import AppLayout from "./components/layout/AppLayout";
import EncoderPage from "./routes/EncoderPage";
import QueuePage from "./routes/QueuePage";
import PresetsPage from "./routes/PresetsPage";
import SettingsPage from "./routes/SettingsPage";
import HistoryPage from "./routes/HistoryPage";
import ActivationDialog from "./components/license/ActivationDialog";
import { useLicenseStore } from "./store/licenseStore";
import { trackEvent } from "./lib/tauri";
import { useEncodeEvents } from "./hooks/useEncodeEvents";

function EventListener() {
  useEncodeEvents();
  return null;
}

/** 页面导航埋点：纯 UI 行为，后端看不到，经 track_event 计数 */
function RouteTracker() {
  const pathname = useLocation().pathname;
  useEffect(() => {
    trackEvent(`page_view:${pathname}`).catch(() => {});
  }, [pathname]);
  return null;
}

/** 启动时拉取授权状态 + 挂载全局激活对话框 */
function LicenseBootstrap() {
  const fetchStatus = useLicenseStore((s) => s.fetchStatus);
  useEffect(() => {
    fetchStatus();
  }, [fetchStatus]);
  return <ActivationDialog />;
}

export default function App() {
  return (
    <HashRouter>
      <EventListener />
      <RouteTracker />
      <LicenseBootstrap />
      <Routes>
        <Route element={<AppLayout />}>
          <Route index element={<EncoderPage />} />
          <Route path="queue" element={<QueuePage />} />
          <Route path="presets" element={<PresetsPage />} />
          <Route path="settings" element={<SettingsPage />} />
          <Route path="history" element={<HistoryPage />} />
        </Route>
      </Routes>
    </HashRouter>
  );
}
