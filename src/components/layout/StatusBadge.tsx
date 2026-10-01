import type { LucideIcon } from "lucide-react";
import { CheckCircle, XCircle, Ban, Clock, Loader2 } from "lucide-react";
import { cn } from "@/lib/utils";

export type StatusTone = "success" | "destructive" | "warning" | "accent" | "secondary";

export const STATUS_TONE: Record<StatusTone, string> = {
  success: "bg-success/12 text-success",
  destructive: "bg-destructive/12 text-destructive",
  warning: "bg-warning/12 text-warning",
  accent: "bg-accent/12 text-accent",
  secondary: "bg-fill text-secondary",
};

export interface StatusConfig {
  label: string;
  tint: string;
  icon: LucideIcon;
}

export const STATUS_CONFIG: Record<string, StatusConfig> = {
  Completed: { icon: CheckCircle, tint: STATUS_TONE.success, label: "已完成" },
  Failed: { icon: XCircle, tint: STATUS_TONE.destructive, label: "失败" },
  Cancelled: { icon: Ban, tint: STATUS_TONE.secondary, label: "已取消" },
  Pending: { icon: Clock, tint: STATUS_TONE.warning, label: "等待中" },
  Encoding: { icon: Loader2, tint: STATUS_TONE.accent, label: "编码中" },
};

export function getStatusConfig(status?: string): StatusConfig {
  if (!status) {
    return {
      icon: CheckCircle,
      tint: STATUS_TONE.secondary,
      label: "",
    };
  }
  return (
    STATUS_CONFIG[status] ?? {
      icon: CheckCircle,
      tint: STATUS_TONE.secondary,
      label: status,
    }
  );
}

export type StatusBadgeSize = "sm" | "md";

export interface StatusBadgeProps extends React.HTMLAttributes<HTMLSpanElement> {
  status?: string;
  tone?: StatusTone;
  size?: StatusBadgeSize;
  className?: string;
  showIcon?: boolean;
  children?: React.ReactNode;
}

export function StatusBadge({
  status,
  tone,
  size = "sm",
  className,
  showIcon = false,
  children,
  ...props
}: StatusBadgeProps) {
  const config = status ? getStatusConfig(status) : undefined;
  const tint = tone ? STATUS_TONE[tone] : (config?.tint ?? STATUS_TONE.secondary);
  const Icon = config?.icon;

  const sizeClasses =
    size === "md"
      ? "rounded-md px-2 py-0.5 text-[11px] leading-4"
      : "rounded-full px-2 py-0.5 text-[10px] leading-4";

  return (
    <span
      className={cn(
        "inline-flex shrink-0 items-center gap-1 font-medium",
        sizeClasses,
        tint,
        className
      )}
      {...props}
    >
      {showIcon && Icon && (
        <Icon className={cn("h-3 w-3", status === "Encoding" && "animate-spin")} />
      )}
      {children ?? config?.label ?? status ?? null}
    </span>
  );
}

export interface StatusIconProps {
  status: string;
  className?: string;
  iconClassName?: string;
}

export function StatusIcon({ status, className, iconClassName }: StatusIconProps) {
  const config = getStatusConfig(status);
  const Icon = config.icon;

  return (
    <div
      className={cn(
        "flex h-9 w-9 shrink-0 items-center justify-center rounded-[9px]",
        config.tint,
        className
      )}
    >
      <Icon
        className={cn(
          "h-4 w-4",
          status === "Encoding" && "animate-spin",
          iconClassName
        )}
      />
    </div>
  );
}

StatusBadge.STATUS_CONFIG = STATUS_CONFIG;
StatusBadge.STATUS_TONE = STATUS_TONE;

export default StatusBadge;
