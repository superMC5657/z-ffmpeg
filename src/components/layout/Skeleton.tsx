import { cn } from "@/lib/utils";

export interface SkeletonProps {
  count?: number;
  className?: string;
}

/** 列表骨架屏，默认 3 行 h-16 */
export function SkeletonList({ count = 3, className }: SkeletonProps) {
  return (
    <div className={cn("space-y-2", className)}>
      {Array.from({ length: count }).map((_, i) => (
        <div key={i} className="h-16 animate-pulse rounded-[10px] bg-fill/70" />
      ))}
    </div>
  );
}

/** 网格卡片骨架屏，默认 3 个 h-32 卡片 */
export function SkeletonCard({ count = 3, className }: SkeletonProps) {
  return (
    <div className={cn("grid gap-3 sm:grid-cols-2 lg:grid-cols-3", className)}>
      {Array.from({ length: count }).map((_, i) => (
        <div
          key={i}
          className="h-32 animate-pulse rounded-[14px] border border-hairline bg-fill/50"
        />
      ))}
    </div>
  );
}
