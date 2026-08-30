import { useEffect, useRef, useState } from "react";
import {
  CheckCircle,
  XCircle,
  Ban,
  History as HistoryIcon,
  Trash2,
  Loader2,
  RotateCcw,
  Search,
  X,
  ChevronLeft,
  ChevronRight,
} from "lucide-react";
import PageHeader from "@/components/layout/PageHeader";
import SegmentedControl from "@/components/layout/SegmentedControl";
import {
  getHistory,
  deleteHistory,
  clearHistory,
} from "@/lib/tauri";
import { useToastStore } from "@/store/toastStore";
import { formatFileSizeCompact } from "@/lib/utils";
import { cn } from "@/lib/utils";
import type { HistoryEntry } from "@/types";

const statusIcons: Record<string, { icon: typeof CheckCircle; tint: string; label: string }> = {
  Completed: { icon: CheckCircle, tint: "bg-success/12 text-success", label: "已完成" },
  Failed: { icon: XCircle, tint: "bg-destructive/10 text-destructive", label: "失败" },
  Cancelled: { icon: Ban, tint: "bg-fill text-secondary", label: "已取消" },
};

const STATUS_FILTERS = [
  { value: "All", label: "全部" },
  { value: "Completed", label: "已完成" },
  { value: "Failed", label: "失败" },
  { value: "Cancelled", label: "已取消" },
] as const;

type StatusFilterValue = (typeof STATUS_FILTERS)[number]["value"];

/** 每页条数 */
const PAGE_SIZE = 20;

/** 单一查询状态：page/status/search 一起变更、一起触发重新拉取 */
interface HistoryQuery {
  /** 0-based 页码 */
  page: number;
  status: StatusFilterValue;
  search: string;
}

export default function HistoryPage() {
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [total, setTotal] = useState(0);
  const [query, setQuery] = useState<HistoryQuery>({
    page: 0,
    status: "All",
    search: "",
  });
  // 搜索框的即时输入（受控值）；防抖后写入 query.search 才真正发起查询
  const [searchInput, setSearchInput] = useState("");
  // 手动重刷标记（重试/删除/清空后 query 不变也需要重新拉取）
  const [reloadKey, setReloadKey] = useState(0);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [deleting, setDeleting] = useState<string | null>(null);
  const [clearing, setClearing] = useState(false);
  const searchTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  // query 变化（翻页/筛选/搜索）或手动重刷时拉取；cancelled 标记防止
  // 旧请求晚到覆盖新结果
  useEffect(() => {
    let cancelled = false;
    (async () => {
      setLoading(true);
      try {
        const result = await getHistory({
          limit: PAGE_SIZE,
          offset: query.page * PAGE_SIZE,
          status: query.status === "All" ? undefined : query.status,
          search: query.search || undefined,
        });
        if (cancelled) return;
        setEntries(result.entries);
        setTotal(result.total);
        setLoadError(null);
      } catch (e) {
        if (cancelled) return;
        // 区分"加载失败"与"没有历史"：失败时展示错误并给出重试入口
        setEntries([]);
        setTotal(0);
        setLoadError(e instanceof Error ? e.message : String(e));
        useToastStore.getState().showToast(
          `加载历史记录失败: ${e instanceof Error ? e.message : String(e)}`,
          "error"
        );
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [query, reloadKey]);

  // 卸载时清理搜索防抖定时器
  useEffect(() => {
    return () => {
      if (searchTimer.current) clearTimeout(searchTimer.current);
    };
  }, []);

  const handleStatusChange = (value: StatusFilterValue) => {
    setQuery((q) => ({ ...q, page: 0, status: value }));
  };

  const handleSearchInput = (value: string) => {
    setSearchInput(value);
    // 输入防抖，避免每个按键都打一次 IPC
    if (searchTimer.current) clearTimeout(searchTimer.current);
    searchTimer.current = setTimeout(() => {
      setQuery((q) => ({ ...q, page: 0, search: value }));
    }, 300);
  };

  const handlePageChange = (newPage: number) => {
    if (newPage < 0 || newPage >= Math.max(1, Math.ceil(total / PAGE_SIZE))) return;
    setQuery((q) => ({ ...q, page: newPage }));
  };

  /** query 不变时的强制重新拉取（重试/删除/清空后） */
  const forceReload = () => setReloadKey((k) => k + 1);

  const totalPages = Math.max(1, Math.ceil(total / PAGE_SIZE));

  const handleDelete = async (id: string) => {
    setDeleting(id);
    try {
      await deleteHistory([id]);
      useToastStore.getState().showToast("历史记录已删除", "success");
      // 删除后当前页可能变空，回第一页重新拉取（总数也会同步刷新）
      setQuery((q) => ({ ...q, page: 0 }));
      forceReload();
    } catch (e) {
      useToastStore.getState().showToast(
        `删除失败: ${e instanceof Error ? e.message : String(e)}`,
        "error"
      );
    } finally {
      setDeleting(null);
    }
  };

  const handleClearAll = async () => {
    setClearing(true);
    try {
      await clearHistory();
      useToastStore.getState().showToast("已清空全部历史记录", "success");
      setQuery((q) => ({ ...q, page: 0 }));
      forceReload();
    } catch (e) {
      useToastStore.getState().showToast(
        `清空失败: ${e instanceof Error ? e.message : String(e)}`,
        "error"
      );
    } finally {
      setClearing(false);
    }
  };

  const formatDate = (iso: string) => {
    try {
      return new Date(iso).toLocaleString("zh-CN");
    } catch {
      return iso;
    }
  };

  return (
    <div>
      <PageHeader
        title="编码历史"
        description="查看已完成的编码任务记录"
        action={
          entries.length > 0 ? (
            <button
              onClick={handleClearAll}
              disabled={clearing}
              className="flex h-9 items-center gap-1.5 rounded-[9px] bg-surface px-4 text-[13px] font-medium text-secondary shadow-card ring-1 ring-hairline transition-colors hover:bg-destructive/10 hover:text-destructive disabled:opacity-50"
            >
              {clearing ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <Trash2 className="h-3.5 w-3.5" />
              )}
              清空历史
            </button>
          ) : undefined
        }
      />

      {/* 筛选与搜索栏 */}
      <div className="flex flex-wrap items-center gap-2">
        <SegmentedControl<StatusFilterValue>
          value={query.status}
          onChange={handleStatusChange}
          options={STATUS_FILTERS.map((f) => ({ value: f.value, label: f.label }))}
        />
        <div className="relative ml-auto w-56">
          <Search className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-tertiary" />
          <input
            value={searchInput}
            onChange={(e) => handleSearchInput(e.target.value)}
            placeholder="搜索文件名…"
            className="h-9 w-full rounded-[9px] bg-fill pl-8 pr-8 text-[13px] text-foreground outline-none placeholder:text-tertiary transition-shadow focus:ring-2 focus:ring-accent/30"
          />
          {searchInput && (
            <button
              aria-label="清除搜索"
              onClick={() => handleSearchInput("")}
              className="absolute right-2 top-1/2 flex h-5 w-5 -translate-y-1/2 items-center justify-center rounded-full text-tertiary hover:bg-fill-strong hover:text-foreground"
            >
              <X className="h-3 w-3" />
            </button>
          )}
        </div>
      </div>

      {loading ? (
        <div className="space-y-2">
          {[0, 1, 2].map((i) => (
            <div key={i} className="h-16 animate-pulse rounded-[10px] bg-fill/70" />
          ))}
        </div>
      ) : loadError ? (
        <div className="flex flex-col items-center justify-center rounded-[14px] border border-dashed border-destructive/40 py-16">
          <div className="flex h-11 w-11 items-center justify-center rounded-[12px] bg-destructive/10">
            <XCircle className="h-5 w-5 text-destructive" />
          </div>
          <p className="mt-3 text-[13px] font-medium">加载历史记录失败</p>
          <p className="mt-0.5 max-w-md break-all text-center text-[12px] text-secondary">
            {loadError}
          </p>
          <button
            onClick={forceReload}
            className="mt-4 flex h-9 items-center gap-1.5 rounded-[9px] bg-fill px-4 text-[13px] font-medium text-foreground transition-colors hover:bg-fill-strong active:scale-[0.98]"
          >
            <RotateCcw className="h-3.5 w-3.5" />
            重试
          </button>
        </div>
      ) : entries.length === 0 ? (
        <div className="flex flex-col items-center justify-center rounded-[14px] border border-dashed border-hairline py-16">
          <div className="flex h-11 w-11 items-center justify-center rounded-[12px] bg-fill">
            <HistoryIcon className="h-5 w-5 text-tertiary" />
          </div>
          <p className="mt-3 text-[13px] font-medium">暂无历史记录</p>
          <p className="mt-0.5 text-[12px] text-secondary">
            完成的编码任务将显示在这里
          </p>
        </div>
      ) : (
        <>
        <div className="overflow-hidden rounded-[14px] border border-hairline bg-surface shadow-card">
          {entries.map((entry, i) => {
            const config = statusIcons[entry.status] || statusIcons.Failed;
            const Icon = config.icon;
            return (
              <div
                key={entry.id}
                className={cn(
                  "group flex items-center gap-3.5 px-3.5 py-3 transition-colors hover:bg-fill/40",
                  i > 0 && "border-t border-hairline"
                )}
              >
                <div
                  className={cn(
                    "flex h-9 w-9 shrink-0 items-center justify-center rounded-[9px]",
                    config.tint
                  )}
                >
                  <Icon className="h-4 w-4" />
                </div>
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="truncate text-[13px] font-medium leading-5">
                      {entry.fileName}
                    </span>
                    <span
                      className={cn(
                        "shrink-0 rounded-full px-2 py-0.5 text-[10px] font-medium leading-4",
                        config.tint
                      )}
                    >
                      {config.label}
                    </span>
                  </div>
                  <div className="mt-0.5 flex items-center gap-2 text-[11px] text-secondary tabular-nums">
                    <span className="shrink-0">{formatDate(entry.createdAt)}</span>
                    {entry.status === "Completed" &&
                      entry.outputSize != null &&
                      (() => {
                        const out = entry.outputSize!;
                        const ratio =
                          entry.inputSize != null && entry.inputSize > 0
                            ? (1 - out / entry.inputSize) * 100
                            : null;
                        return (
                          <span
                            className={cn(
                              "shrink-0 font-medium",
                              ratio != null && ratio < 0 ? "text-warning" : "text-success"
                            )}
                          >
                            {ratio != null
                              ? `${ratio >= 0 ? "↓" : "↑"}${Math.abs(ratio).toFixed(1)}% ${formatFileSizeCompact(out)}`
                              : formatFileSizeCompact(out)}
                          </span>
                        );
                      })()}
                    {entry.status === "Completed" && entry.vmafScore != null && (
                      <span className="shrink-0 font-medium text-success">
                        VMAF {entry.vmafScore.toFixed(1)}
                      </span>
                    )}
                    {entry.outputPath && (
                      <span className="truncate text-tertiary" title={entry.outputPath}>
                        → {entry.outputPath}
                      </span>
                    )}
                  </div>
                  {entry.error && (
                    <p className="mt-0.5 truncate text-[11px] text-destructive">
                      {entry.error}
                    </p>
                  )}
                </div>
                <button
                  onClick={() => handleDelete(entry.id)}
                  disabled={deleting === entry.id}
                  title="删除该条历史"
                  className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-tertiary opacity-0 transition-all hover:bg-destructive/10 hover:text-destructive group-hover:opacity-100 disabled:opacity-50"
                >
                  {deleting === entry.id ? (
                    <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  ) : (
                    <Trash2 className="h-3.5 w-3.5" />
                  )}
                </button>
              </div>
            );
          })}
        </div>

        {/* 分页 */}
        {totalPages > 1 && (
          <div className="flex items-center justify-end gap-3 pt-3 text-[12px] text-secondary">
            <span className="tabular-nums">
              共 {total} 条 · 第 {query.page + 1}/{totalPages} 页
            </span>
            <div className="flex items-center gap-1">
              <button
                aria-label="上一页"
                onClick={() => handlePageChange(query.page - 1)}
                disabled={query.page === 0}
                className="flex h-7 w-7 items-center justify-center rounded-[7px] bg-fill transition-colors hover:bg-fill-strong disabled:opacity-40"
              >
                <ChevronLeft className="h-3.5 w-3.5" />
              </button>
              <button
                aria-label="下一页"
                onClick={() => handlePageChange(query.page + 1)}
                disabled={query.page >= totalPages - 1}
                className="flex h-7 w-7 items-center justify-center rounded-[7px] bg-fill transition-colors hover:bg-fill-strong disabled:opacity-40"
              >
                <ChevronRight className="h-3.5 w-3.5" />
              </button>
            </div>
          </div>
        )}
        </>
      )}
    </div>
  );
}
