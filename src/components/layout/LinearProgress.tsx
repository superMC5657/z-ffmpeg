import { cn } from "@/lib/utils";

export interface LinearProgressProps {
  /** 进度百分比（0-100） */
  value: number;
  /** 进度条主题变体：默认 accent */
  variant?: "accent" | "success" | "destructive";
  /** 尺寸：sm (h-1) 或 md (h-1.5，默认) */
  size?: "sm" | "md";
  /** 自定义类名（可覆盖轨道高度、圆角与背景色） */
  className?: string;
}

const VARIANT_CLASSES: Record<"accent" | "success" | "destructive", string> = {
  accent: "bg-accent",
  success: "bg-success",
  destructive: "bg-destructive",
};

const SIZE_CLASSES: Record<"sm" | "md", string> = {
  sm: "h-1",
  md: "h-1.5",
};

export default function LinearProgress({
  value,
  variant = "accent",
  size = "md",
  className,
}: LinearProgressProps) {
  const numeric = typeof value === "number" && !Number.isNaN(value) ? value : 0;
  const clamped = Math.min(100, Math.max(0, numeric));

  return (
    <div
      role="progressbar"
      aria-valuenow={Math.round(clamped)}
      aria-valuemin={0}
      aria-valuemax={100}
      className={cn(
        "overflow-hidden rounded-full bg-fill-strong",
        SIZE_CLASSES[size],
        className
      )}
    >
      <div
        className={cn(
          "h-full rounded-full transition-all duration-300 ease-out",
          VARIANT_CLASSES[variant]
        )}
        style={{ width: `${clamped}%` }}
      />
    </div>
  );
}
