import { useEffect, useState } from "react";
import { Loader2 } from "lucide-react";
import { useQueueStore } from "@/store/queueStore";
import { useToastStore } from "@/store/toastStore";
import { useLicenseStore, FREE_MAX_CONCURRENT } from "@/store/licenseStore";
import Card from "@/components/layout/Card";
import AppleInput from "@/components/layout/AppleInput";

export default function QueueSection() {
  // Shared with Queue page — editing here syncs there and vice versa
  const maxConcurrent = useQueueStore((s) => s.maxConcurrent);
  const maxConcurrentLoaded = useQueueStore((s) => s.maxConcurrentLoaded);
  const fetchMaxConcurrent = useQueueStore((s) => s.fetchMaxConcurrent);
  const updateMaxConcurrent = useQueueStore((s) => s.updateMaxConcurrent);
  const [savingConcurrent, setSavingConcurrent] = useState(false);

  const isPro = useLicenseStore((s) => s.status?.pro === true);

  useEffect(() => {
    fetchMaxConcurrent();
  }, [fetchMaxConcurrent]);

  const handleConcurrentChange = async (value: number) => {
    setSavingConcurrent(true);
    try {
      const saved = await updateMaxConcurrent(value);
      useToastStore.getState().showToast(
        `队列并发数已更新为 ${saved}`,
        "success"
      );
    } catch (e) {
      useToastStore.getState().showToast(
        `保存失败: ${e instanceof Error ? e.message : String(e)}`,
        "error"
      );
    } finally {
      setSavingConcurrent(false);
    }
  };

  return (
    <Card title="队列设置">
      <div className="flex items-center justify-between gap-4">
        <div className="min-w-0">
          <p className="text-[13px] font-medium">最大并发编码任务数</p>
          <p className="mt-0.5 text-[12px] leading-5 text-secondary">
            与队列页同步，任一处修改立即生效并保存。硬件加速下 2-4
            个即可占满显卡，软件编码可适当调高。
            {!isPro && <>免费版上限 {FREE_MAX_CONCURRENT}，Pro 版可解锁 1-16。</>}
          </p>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <AppleInput
            type="number"
            min={1}
            max={isPro ? 16 : FREE_MAX_CONCURRENT}
            value={maxConcurrentLoaded ? Math.min(maxConcurrent, isPro ? 16 : FREE_MAX_CONCURRENT) : ""}
            onChange={(e) => {
              const cap = isPro ? 16 : FREE_MAX_CONCURRENT;
              const v = parseInt(e.target.value);
              if (!Number.isNaN(v) && v >= 1 && v <= cap) {
                handleConcurrentChange(v);
              }
            }}
            placeholder={maxConcurrentLoaded ? undefined : "…"}
            disabled={!maxConcurrentLoaded || savingConcurrent}
            className="w-16 text-center"
          />
          {savingConcurrent && (
            <Loader2 className="h-3.5 w-3.5 animate-spin text-tertiary" />
          )}
        </div>
      </div>
    </Card>
  );
}
