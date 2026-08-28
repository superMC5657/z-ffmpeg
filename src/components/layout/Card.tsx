import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

interface CardProps {
  title?: string;
  description?: string;
  /** 标题行右侧操作区 */
  action?: ReactNode;
  children: ReactNode;
  className?: string;
  contentClassName?: string;
}

/** Apple 分组卡片：14px 圆角、hairline 边、柔和投影 */
export default function Card({
  title,
  description,
  action,
  children,
  className,
  contentClassName,
}: CardProps) {
  const hasHeader = !!title || !!action;
  return (
    <section
      className={cn(
        "rounded-[14px] border border-hairline bg-surface shadow-card",
        className
      )}
    >
      {hasHeader && (
        <header className="flex items-center justify-between gap-3 px-5 pt-4">
          <div>
            {title && (
              <h2 className="text-[15px] font-semibold leading-6">{title}</h2>
            )}
            {description && (
              <p className="mt-0.5 text-[12px] text-secondary">{description}</p>
            )}
          </div>
          {action && <div className="shrink-0">{action}</div>}
        </header>
      )}
      <div className={cn("p-5", hasHeader && "pt-3", contentClassName)}>
        {children}
      </div>
    </section>
  );
}
