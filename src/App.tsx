import { HashRouter, Routes, Route } from "react-router-dom";
import AppLayout from "./components/layout/AppLayout";
import EncoderPage from "./routes/EncoderPage";
import QueuePage from "./routes/QueuePage";
import PresetsPage from "./routes/PresetsPage";
import SettingsPage from "./routes/SettingsPage";
import HistoryPage from "./routes/HistoryPage";
import { useEncodeEvents } from "./hooks/useEncodeEvents";

function EventListener() {
  useEncodeEvents();
  return null;
}

export default function App() {
  return (
    <HashRouter>
      <EventListener />
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
