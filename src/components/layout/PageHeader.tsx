import type { LucideIcon } from "lucide-react";
import type { ReactNode } from "react";

interface PageHeaderProps {
  icon: LucideIcon;
  title: string;
  description: string;
  /** 可选:渲染在标题行最右侧的操作区(与标题同高度) */
  action?: ReactNode;
}

export default function PageHeader({
  icon: Icon,
  title,
  description,
  action,
}: PageHeaderProps) {
  return (
    <div className="flex items-center justify-between gap-3.5">
      <div className="flex items-center gap-3.5">
        <div className="flex h-11 w-11 shrink-0 items-center justify-center rounded-xl bg-gradient-brand text-white shadow-lg shadow-primary/25">
          <Icon className="h-5.5 w-5.5" strokeWidth={2.2} />
        </div>
        <div>
          <h1 className="text-2xl font-bold tracking-tight">{title}</h1>
          <p className="mt-0.5 text-[13px] text-muted-foreground">{description}</p>
        </div>
      </div>
      {action && <div className="shrink-0">{action}</div>}
    </div>
  );
}
