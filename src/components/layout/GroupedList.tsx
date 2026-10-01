import type { HTMLAttributes, ReactNode } from "react";
import { cn } from "@/lib/utils";

export interface GroupedListProps extends HTMLAttributes<HTMLDivElement> {
  children?: ReactNode;
  className?: string;
}

/** Inset 分组列表容器：14px 圆角卡片外框、统一 hairline 边框与柔和投影 */
export function GroupedList({ children, className, ...props }: GroupedListProps) {
  return (
    <div
      className={cn(
        "overflow-hidden rounded-[14px] border border-hairline bg-surface shadow-card",
        className
      )}
      {...props}
    >
      {children}
    </div>
  );
}

export interface GroupedListItemProps
  extends Omit<HTMLAttributes<HTMLDivElement>, "content"> {
  /** 左侧插槽（与 icon 互为别名） */
  left?: ReactNode;
  /** 图标/前缀插槽 */
  icon?: ReactNode;
  /** 主内容区（占据自适应伸缩宽度 flex-1） */
  content?: ReactNode;
  /** 右侧操作区插槽 */
  actions?: ReactNode;
  /** 主内容区样式类名 */
  contentClassName?: string;
  /** 是否为首行（省略时由 CSS first:border-t-0 自动判断） */
  isFirst?: boolean;
  children?: ReactNode;
  className?: string;
}

/** Inset 分组列表行：行间 hairline 分割、hover 背景、统一插槽 */
export function GroupedListItem({
  left,
  icon,
  content,
  actions,
  children,
  className,
  contentClassName,
  isFirst,
  ...props
}: GroupedListItemProps) {
  const leading = left ?? icon;
  const hasSlots = Boolean(leading || content || actions);

  return (
    <div
      className={cn(
        "group flex items-center gap-3.5 px-3.5 py-3 transition-colors hover:bg-fill/40",
        isFirst !== undefined
          ? isFirst
            ? "border-t-0"
            : "border-t border-hairline"
          : "border-t border-hairline first:border-t-0",
        className
      )}
      {...props}
    >
      {hasSlots ? (
        <>
          {leading && <div className="shrink-0">{leading}</div>}
          {(content || children) && (
            <div className={cn("min-w-0 flex-1", contentClassName)}>
              {content}
              {children}
            </div>
          )}
          {actions && (
            <div className="shrink-0 flex items-center gap-1.5">{actions}</div>
          )}
        </>
      ) : (
        children
      )}
    </div>
  );
}

export default GroupedList;
