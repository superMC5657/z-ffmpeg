import { useEffect, type ReactNode } from "react";
import { X } from "lucide-react";
import { cn } from "@/lib/utils";

export interface DialogProps {
  open?: boolean;
  onClose: () => void;
  title: ReactNode;
  description?: ReactNode;
  children: ReactNode;
  footer?: ReactNode;
  actions?: ReactNode;
  wide?: boolean;
  className?: string;
}

export default function Dialog({
  open = true,
  onClose,
  title,
  description,
  children,
  footer,
  actions,
  wide = false,
  className,
}: DialogProps) {
  useEffect(() => {
    if (!open) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        onClose();
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [open, onClose]);

  if (!open) return null;

  const dialogFooter = footer ?? actions;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-overlay backdrop-blur-[2px]"
      onClick={onClose}
    >
      <div
        role="dialog"
        aria-modal="true"
        className={cn(
          "animate-dialog-in w-full rounded-[14px] border border-hairline bg-surface p-5 shadow-pop",
          wide ? "max-w-2xl" : "max-w-md",
          className
        )}
        onClick={(e) => e.stopPropagation()}
      >
        <div
          className={`mb-4 flex justify-between gap-3 ${
            description ? "items-start" : "items-center"
          }`}
        >
          <div>
            <h2 className="flex items-center gap-2 text-[15px] font-semibold text-foreground">
              {title}
            </h2>
            {description && (
              <p className="mt-1 text-[12px] text-secondary leading-normal">
                {description}
              </p>
            )}
          </div>
          <button
            type="button"
            aria-label="关闭"
            onClick={onClose}
            className="flex h-7 w-7 shrink-0 items-center justify-center rounded-full text-tertiary transition-colors hover:bg-fill-strong hover:text-foreground"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        {children}

        {dialogFooter && (
          <div className="mt-5 flex justify-end gap-2">{dialogFooter}</div>
        )}
      </div>
    </div>
  );
}
