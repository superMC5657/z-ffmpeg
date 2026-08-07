import { useEffect, useState } from "react";
import {
  CheckCircle,
  XCircle,
  Ban,
  Clock,
  History as HistoryIcon,
  Trash2,
  Loader2,
} from "lucide-react";
import PageHeader from "@/components/layout/PageHeader";
import {
  getHistory,
  deleteHistory,
  clearHistory,
} from "@/lib/tauri";
import { useToastStore } from "@/store/toastStore";
import { formatFileSizeCompact } from "@/lib/utils";

interface HistoryEntry {
  id: string;
  inputPath: string;
  outputPath: string;
  fileName: string;
  status: string;
  /** VMAF 平均得分（0-100），计算过才有值 */
  vmafScore: number | null;
  /** 实际输出体积（字节），完成的任务才有值 */
  outputSize: number | null;
  /** 原始文件体积（字节），入队时读取，用于计算压缩率 */
  inputSize: number | null;
  createdAt: string;
  completedAt: string | null;
  error: string | null;
}

const statusIcons: Record<string, { icon: typeof CheckCircle; color: string; label: string }> = {
  Completed: { icon: CheckCircle, color: "text-green-400", label: "完成" },
  Failed: { icon: XCircle, color: "text-red-400", label: "失败" },
  Cancelled: { icon: Ban, color: "text-gray-400", label: "已取消" },
};

export default function HistoryPage() {
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [deleting, setDeleting] = useState<string | null>(null);
  const [clearing, setClearing] = useState(false);

  const refresh = async () => {
    setLoading(true);
    try {
      const data = await getHistory();
      setEntries(data as HistoryEntry[]);
    } catch {
      setEntries([]);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    refresh();
  }, []);

  const handleDelete = async (id: string) => {
    setDeleting(id);
    try {
      await deleteHistory([id]);
      useToastStore.getState().showToast("历史记录已删除", "success");
      await refresh();
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
      await refresh();
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

  if (loading) {
    return (
      <div className="mx-auto max-w-5xl space-y-8">
        <PageHeader
          icon={HistoryIcon}
          title="编码历史"
          description="查看已完成的编码任务记录"
        />
        <p className="text-sm text-muted-foreground">加载中...</p>
      </div>
    );
  }

  return (
    <div className="mx-auto max-w-5xl space-y-8">
      <PageHeader
        icon={HistoryIcon}
        title="编码历史"
        description="查看已完成的编码任务记录"
        action={
          entries.length > 0 ? (
            <button
              onClick={handleClearAll}
              disabled={clearing}
              className="flex items-center gap-1.5 rounded-md px-4 py-2 text-[14px] font-medium text-muted-foreground transition-colors hover:bg-destructive/15 hover:text-destructive disabled:cursor-not-allowed disabled:opacity-50"
            >
              {clearing ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <Trash2 className="h-4 w-4" />
              )}
              清空历史
            </button>
          ) : undefined
        }
      />

      {entries.length === 0 ? (
        <div className="flex flex-col items-center justify-center rounded-xl border border-dashed border-border bg-card/40 py-20">
          <div className="mb-3 flex h-14 w-14 items-center justify-center rounded-2xl bg-accent">
            <Clock className="h-7 w-7 text-muted-foreground/50" />
          </div>
          <p className="text-sm font-medium text-foreground/80">暂无历史记录</p>
          <p className="mt-1 text-[13px] text-muted-foreground">
            完成的编码任务将显示在这里
          </p>
        </div>
      ) : (
        <div className="space-y-2">
          {entries.map((entry) => {
            const config = statusIcons[entry.status] || statusIcons.Failed;
            const Icon = config.icon;
            return (
              <div
                key={entry.id}
                className="group flex items-center gap-3 rounded-xl border border-border bg-card p-3 shadow-sm transition-all hover:border-primary/30 hover:shadow-md"
              >
                <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md bg-accent">
                  <Icon className={`h-4 w-4 ${config.color}`} />
                </div>
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="truncate text-sm font-medium">
                      {entry.fileName}
                    </span>
                    <span className={`shrink-0 rounded-full px-2.5 py-0.5 text-[13px] font-medium bg-accent ${config.color}`}>
                      {config.label}
                    </span>
                  </div>
                  <div className="mt-1 flex gap-2 text-[13px] text-muted-foreground">
                    <span>{formatDate(entry.createdAt)}</span>
                    {entry.status === "Completed" &&
                      entry.outputSize != null &&
                      (() => {
                        const out = entry.outputSize!;
                        const ratio =
                          entry.inputSize != null && entry.inputSize > 0
                            ? (1 - out / entry.inputSize) * 100
                            : null;
                        return (
                          <span className={ratio != null && ratio < 0 ? "text-warning" : "text-green-400/80"}>
                            {ratio != null
                              ? `${ratio >= 0 ? "↓" : "↑"}${Math.abs(ratio).toFixed(1)}% ${formatFileSizeCompact(out)}`
                              : formatFileSizeCompact(out)}
                          </span>
                        );
                      })()}
                    {entry.status === "Completed" && entry.vmafScore != null && (
                      <span className="font-medium text-green-400">VMAF {entry.vmafScore.toFixed(1)}</span>
                    )}
                    {entry.outputPath && (
                      <span className="truncate">→ {entry.outputPath}</span>
                    )}
                  </div>
                  {entry.error && (
                    <p className="mt-1 truncate text-[13px] text-red-400">
                      {entry.error}
                    </p>
                  )}
                </div>
                <button
                  onClick={() => handleDelete(entry.id)}
                  disabled={deleting === entry.id}
                  title="删除该条历史"
                  className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-destructive/20 hover:text-destructive disabled:opacity-50"
                >
                  {deleting === entry.id ? (
                    <Loader2 className="h-4 w-4 animate-spin" />
                  ) : (
                    <Trash2 className="h-4 w-4" />
                  )}
                </button>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
