import { Outlet } from "react-router-dom";
import Sidebar from "./Sidebar";
import Titlebar from "./Titlebar";
import ToastHost from "./ToastHost";

export default function AppLayout() {
  return (
    <div className="flex h-screen w-screen flex-col overflow-hidden bg-background">
      <Titlebar />
      <div className="flex flex-1 overflow-hidden">
        <Sidebar />
        <main className="flex-1 overflow-auto p-8">
          <Outlet />
        </main>
      </div>
      <ToastHost />
    </div>
  );
}
