import { cn } from "@/lib/utils";

interface AppleInputProps extends React.InputHTMLAttributes<HTMLInputElement> {
  className?: string;
}

/** Apple 风格输入框：填充底、无边框、隐藏 number spinner */
export default function AppleInput({ className, ...props }: AppleInputProps) {
  return (
    <input
      {...props}
      className={cn(
        "h-9 rounded-lg bg-fill px-3 text-[13px] tabular-nums transition-colors",
        "placeholder:text-tertiary hover:bg-fill-strong disabled:opacity-50",
        "focus-visible:outline-2 focus-visible:-outline-offset-1 focus-visible:outline-accent",
        "[appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none",
        className
      )}
    />
  );
}
