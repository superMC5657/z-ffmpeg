import { CheckCircle2, Info, X, XCircle } from "lucide-react";
import { useToastStore } from "@/store/toastStore";
import { cn } from "@/lib/utils";

const styles = {
  success: {
    icon: CheckCircle2,
    iconCls: "text-success",
    chipCls: "bg-success/12",
  },
  error: {
    icon: XCircle,
    iconCls: "text-destructive",
    chipCls: "bg-destructive/12",
  },
  info: {
    icon: Info,
    iconCls: "text-accent",
    chipCls: "bg-accent/12",
  },
} as const;

/** macOS 通知风格 Toast：顶部右侧滑入的毛玻璃卡片 */
export default function ToastHost() {
  const toasts = useToastStore((s) => s.toasts);
  const dismissToast = useToastStore((s) => s.dismissToast);

  return (
    <div className="pointer-events-none fixed right-4 top-12 z-[100] flex w-80 flex-col gap-2.5">
      {toasts.map((toast) => {
        const { icon: Icon, iconCls, chipCls } = styles[toast.type];
        return (
          <div
            key={toast.id}
            role="status"
            className={cn(
              "animate-toast-in pointer-events-auto flex items-center gap-3 rounded-[14px] border border-hairline px-3.5 py-3 shadow-pop",
              "bg-surface/85 backdrop-blur-xl"
            )}
          >
            <span
              className={cn(
                "flex h-6 w-6 shrink-0 items-center justify-center rounded-full",
                chipCls
              )}
            >
              <Icon className={cn("h-4 w-4", iconCls)} />
            </span>
            <span className="flex-1 text-[13px] leading-5 text-foreground">
              {toast.message}
            </span>
            <button
              aria-label="关闭通知"
              onClick={() => dismissToast(toast.id)}
              className="flex h-5 w-5 shrink-0 items-center justify-center rounded-full text-tertiary transition-colors hover:bg-fill-strong hover:text-foreground"
            >
              <X className="h-3.5 w-3.5" />
            </button>
          </div>
        );
      })}
    </div>
  );
}
