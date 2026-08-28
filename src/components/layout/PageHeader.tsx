import type { ReactNode } from "react";

interface PageHeaderProps {
  title: string;
  description: string;
  /** 可选:渲染在标题行最右侧的操作区(与标题同高度) */
  action?: ReactNode;
}

/** Apple 大标题风格页头 */
export default function PageHeader({ title, description, action }: PageHeaderProps) {
  return (
    <div className="flex items-end justify-between gap-4 pb-6">
      <div>
        <h1 className="text-[28px] font-bold leading-tight tracking-[-0.02em]">
          {title}
        </h1>
        <p className="mt-1 text-[13px] text-secondary">{description}</p>
      </div>
      {action && <div className="shrink-0 pb-1">{action}</div>}
    </div>
  );
}
