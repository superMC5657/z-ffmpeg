import { cn } from "@/lib/utils";

interface SegmentedControlProps<T extends string> {
  value: T;
  onChange: (value: T) => void;
  options: { value: T; label: string }[];
  /** 全宽平分（否则按内容自适应） */
  block?: boolean;
  className?: string;
}

/** macOS 分段控件 */
export default function SegmentedControl<T extends string>({
  value,
  onChange,
  options,
  block = false,
  className,
}: SegmentedControlProps<T>) {
  return (
    <div
      role="radiogroup"
      className={cn(
        "inline-flex items-center gap-0.5 rounded-[9px] bg-fill p-[2px]",
        block && "flex w-full",
        className
      )}
    >
      {options.map((opt) => {
        const selected = opt.value === value;
        return (
          <button
            key={opt.value}
            role="radio"
            aria-checked={selected}
            onClick={() => onChange(opt.value)}
            className={cn(
              "h-7 rounded-[7px] px-3 text-[13px] leading-none transition-all",
              block && "flex-1",
              selected
                ? "bg-surface font-medium text-foreground shadow-sm ring-1 ring-black/5"
                : "text-secondary hover:text-foreground"
            )}
          >
            {opt.label}
          </button>
        );
      })}
    </div>
  );
}
