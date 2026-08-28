import { ChevronDown } from "lucide-react";
import { cn } from "@/lib/utils";

interface AppleSelectProps extends React.SelectHTMLAttributes<HTMLSelectElement> {
  className?: string;
}

/** Apple 风格原生 select：填充底、无边框、自定义箭头 */
export default function AppleSelect({ className, ...props }: AppleSelectProps) {
  return (
    <div className={cn("relative inline-flex", className)}>
      <select
        {...props}
        className={cn(
          "h-9 w-full cursor-pointer appearance-none rounded-lg bg-fill py-0 pl-3 pr-8 text-[13px] text-foreground transition-colors",
          "hover:bg-fill-strong focus-visible:outline-2 focus-visible:outline-offset-0 focus-visible:outline-accent",
          "disabled:cursor-default disabled:opacity-50"
        )}
      />
      <ChevronDown className="pointer-events-none absolute right-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-tertiary" />
    </div>
  );
}
