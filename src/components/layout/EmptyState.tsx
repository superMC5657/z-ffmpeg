import type { ReactNode } from "react";
import type { LucideIcon } from "lucide-react";
import { cn } from "@/lib/utils";

export interface EmptyStateProps {
  icon: LucideIcon;
  title: ReactNode;
  description?: ReactNode;
  action?: ReactNode;
  variant?: "default" | "error";
  className?: string;
}

export default function EmptyState({
  icon: Icon,
  title,
  description,
  action,
  variant = "default",
  className,
}: EmptyStateProps) {
  const isError = variant === "error";

  return (
    <div
      className={cn(
        "flex flex-col items-center justify-center rounded-[14px] border border-dashed py-16",
        isError ? "border-destructive/40" : "border-hairline",
        className
      )}
    >
      <div
        className={cn(
          "flex h-11 w-11 items-center justify-center rounded-[12px]",
          isError ? "bg-destructive/10 text-destructive" : "bg-fill text-tertiary"
        )}
      >
        <Icon className="h-5 w-5" />
      </div>
      <p className="mt-3 text-[13px] font-medium">{title}</p>
      {description && (
        <p
          className={cn(
            "mt-0.5 text-[12px] text-secondary",
            isError && "max-w-md break-all text-center"
          )}
        >
          {description}
        </p>
      )}
      {action && <div className="mt-4">{action}</div>}
    </div>
  );
}
