import type { ReactNode } from "react";
import { Crown } from "lucide-react";
import { useLicenseStore } from "@/store/licenseStore";
import { cn } from "@/lib/utils";

interface ProGateProps {
  children: ReactNode;
  /** 外层包装容器的类名（默认 inline-flex 自适应内容尺寸） */
  className?: string;
  /** 悬停提示，默认「该功能为 Pro 版功能」 */
  title?: string;
}

/**
 * Pro 门控包装器：未激活时内容半透明禁点，覆盖层显示 PRO 徽标，
 * 点击任意处拉起全局激活对话框。已激活时原样渲染 children。
 * 注意这只是 UI 层锁定——真正的强制在后端命令层（ensure_config_allowed 等）。
 */
export default function ProGate({ children, className, title }: ProGateProps) {
  const isPro = useLicenseStore((s) => s.status?.pro === true);
  const setActivationOpen = useLicenseStore((s) => s.setActivationOpen);

  if (isPro) return <>{children}</>;

  return (
    <div className={cn("relative inline-flex", className)}>
      <div aria-hidden className="pointer-events-none select-none opacity-50">
        {children}
      </div>
      <button
        onClick={() => setActivationOpen(true)}
        title={title ?? "该功能为 Pro 版功能，点击激活"}
        className="absolute inset-0 z-10 flex items-center justify-center rounded-[inherit]"
      >
        <span className="flex items-center gap-1 rounded-full bg-amber-500/90 px-2 py-0.5 text-[10px] font-semibold leading-4 text-white shadow-sm transition-transform hover:scale-105">
          <Crown className="h-3 w-3" />
          PRO
        </span>
      </button>
    </div>
  );
}

/** Pro 徽标（内联小标签，用于卡片标题等处标注 Pro 专属功能） */
export function ProBadge({ className }: { className?: string }) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-0.5 rounded-full bg-amber-500/90 px-1.5 py-0.5 text-[10px] font-semibold leading-4 text-white",
        className
      )}
    >
      <Crown className="h-2.5 w-2.5" />
      PRO
    </span>
  );
}
