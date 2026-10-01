import React, { forwardRef } from "react";
import { Loader2 } from "lucide-react";
import { cn } from "@/lib/utils";

export type AppleButtonVariant = "primary" | "secondary" | "ghost" | "danger";
export type AppleButtonSize = "sm" | "md" | "lg";

export interface AppleButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: AppleButtonVariant;
  size?: AppleButtonSize;
  loading?: boolean;
  disabled?: boolean;
  onClick?: React.MouseEventHandler<HTMLButtonElement>;
  children?: React.ReactNode;
  className?: string;
  type?: "button" | "submit" | "reset";
}

const VARIANT_CLASSES: Record<AppleButtonVariant, string> = {
  primary:
    "bg-accent text-on-accent shadow-sm hover:bg-accent-hover disabled:hover:bg-accent",
  secondary:
    "bg-fill text-foreground hover:bg-fill-strong disabled:hover:bg-fill",
  ghost:
    "bg-transparent text-secondary hover:bg-fill-strong hover:text-foreground disabled:opacity-40 disabled:hover:bg-transparent disabled:hover:text-secondary",
  danger:
    "bg-destructive text-white shadow-sm hover:bg-destructive/90 disabled:hover:bg-destructive",
};

const SIZE_CLASSES: Record<AppleButtonSize, string> = {
  sm: "h-7.5 px-3 rounded-[7px] text-[12px]",
  md: "h-9 px-4 rounded-[9px] text-[13px]",
  lg: "h-11 px-5 rounded-[11px] text-[15px]",
};

const SPINNER_SIZES: Record<AppleButtonSize, string> = {
  sm: "h-3 w-3",
  md: "h-3.5 w-3.5",
  lg: "h-4 w-4",
};

/**
 * Apple 风格统一按钮基座
 * 
 * 规范全站按钮高度、圆角、字号、点击回弹与 loading 态：
 * - primary: bg-accent text-on-accent hover:bg-accent-hover
 * - secondary: bg-fill hover:bg-fill-strong
 * - ghost: bg-transparent text-secondary hover:bg-fill-strong
 * - danger: bg-destructive text-white hover:bg-destructive/90
 */
export const AppleButton = forwardRef<HTMLButtonElement, AppleButtonProps>(
  function AppleButton(
    {
      variant = "primary",
      size = "md",
      loading = false,
      disabled = false,
      type = "button",
      className,
      children,
      ...props
    },
    ref
  ) {
    const isDisabled = disabled || loading;

    return (
      <button
        ref={ref}
        type={type}
        disabled={isDisabled}
        aria-busy={loading ? "true" : undefined}
        className={cn(
          "inline-flex items-center justify-center gap-1.5 font-medium select-none transition-all outline-none",
          "focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-accent",
          "active:scale-[0.98] disabled:cursor-default disabled:opacity-50 disabled:active:scale-100",
          SIZE_CLASSES[size],
          VARIANT_CLASSES[variant],
          className
        )}
        {...props}
      >
        {loading && (
          <Loader2
            className={cn("animate-spin shrink-0", SPINNER_SIZES[size])}
            aria-hidden="true"
          />
        )}
        {children}
      </button>
    );
  }
);

export default AppleButton;
