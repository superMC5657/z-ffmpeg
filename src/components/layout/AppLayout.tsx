import { Outlet } from "react-router-dom";
import Sidebar from "./Sidebar";
import Titlebar from "./Titlebar";
import ToastHost from "./ToastHost";
import GlobalEncodingDock from "./GlobalEncodingDock";

export default function AppLayout() {
  return (
    <div className="flex h-screen w-screen flex-col overflow-hidden bg-background text-foreground">
      <Titlebar />
      <div className="flex flex-1 overflow-hidden">
        <Sidebar />
        <main className="flex-1 overflow-y-auto">
          <div className="mx-auto w-full max-w-[1560px] px-6 py-6 lg:px-8 lg:py-7">
            <Outlet />
          </div>
        </main>
      </div>
      <GlobalEncodingDock />
      <ToastHost />
    </div>
  );
}
