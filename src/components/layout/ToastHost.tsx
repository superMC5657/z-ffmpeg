import { CheckCircle2, Info, X, XCircle } from "lucide-react";
import { useToastStore } from "@/store/toastStore";
import { cn } from "@/lib/utils";

const styles = {
  success: {
    icon: CheckCircle2,
    cls: "border-success/40 bg-card/95 text-success",
  },
  error: {
    icon: XCircle,
    cls: "border-destructive/40 bg-card/95 text-destructive",
  },
  info: {
    icon: Info,
    cls: "border-primary/40 bg-card/95 text-primary",
  },
} as const;

export default function ToastHost() {
  const toasts = useToastStore((s) => s.toasts);
  const dismissToast = useToastStore((s) => s.dismissToast);

  return (
    <div className="pointer-events-none fixed right-4 top-14 z-[100] flex w-72 flex-col gap-2">
      {toasts.map((toast) => {
        const { icon: Icon, cls } = styles[toast.type];
        return (
          <div
            key={toast.id}
            className={cn(
              "pointer-events-auto flex items-center gap-2.5 rounded-xl border px-3 py-2.5 text-sm shadow-xl backdrop-blur",
              cls
            )}
          >
            <Icon className="h-4 w-4 shrink-0" />
            <span className="flex-1 text-foreground">{toast.message}</span>
            <button
              onClick={() => dismissToast(toast.id)}
              className="text-muted-foreground transition-colors hover:text-foreground"
            >
              <X className="h-3.5 w-3.5" />
            </button>
          </div>
        );
      })}
    </div>
  );
}
