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
        <main className="flex-1 overflow-y-auto">
          <div className="mx-auto w-full max-w-[940px] px-8 py-9">
            <Outlet />
          </div>
        </main>
      </div>
      <ToastHost />
    </div>
  );
}
