import { useEffect, useState } from "react";
import { Loader2 } from "lucide-react";
import { useQueueStore } from "@/store/queueStore";
import { useToastStore } from "@/store/toastStore";
import Card from "@/components/layout/Card";
import AppleInput from "@/components/layout/AppleInput";

export default function VmafSection() {
  // VMAF 段数设置（0 = 全量对比，N = N 段 × 5 秒均匀采样），队列页计算按钮用
  const vmafSegments = useQueueStore((s) => s.vmafSegments);
  const vmafSegmentsLoaded = useQueueStore((s) => s.vmafSegmentsLoaded);
  const fetchVmafSegments = useQueueStore((s) => s.fetchVmafSegments);
  const updateVmafSegments = useQueueStore((s) => s.updateVmafSegments);
  const [savingVmaf, setSavingVmaf] = useState(false);

  useEffect(() => {
    fetchVmafSegments();
  }, []);

  const handleVmafSegmentsChange = async (value: number) => {
    setSavingVmaf(true);
    try {
      const saved = await updateVmafSegments(value);
      useToastStore.getState().showToast(
        saved === 0 ? "VMAF 已切换为全量对比" : `VMAF 采样段数已更新为 ${saved}`,
        "success"
      );
    } catch (e) {
      useToastStore.getState().showToast(
        `保存失败: ${e instanceof Error ? e.message : String(e)}`,
        "error"
      );
    } finally {
      setSavingVmaf(false);
    }
  };

  return (
    <Card title="VMAF 质量评估">
      <div className="flex items-center justify-between gap-4">
        <div className="min-w-0">
          <p className="text-[13px] font-medium">采样段数</p>
          <p className="mt-0.5 text-[12px] leading-5 text-secondary">
            设为 <b>0</b>：全量对比（逐帧计算，最精确但耗时长）。设为{" "}
            <b>N</b>：均匀取 N 段 × 5 秒计算取平均，几秒到几十秒完成。队列页「VMAF」按钮按此设置计算。
          </p>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <AppleInput
            type="number"
            min={0}
            max={32}
            value={vmafSegmentsLoaded ? vmafSegments : ""}
            onChange={(e) => {
              const v = parseInt(e.target.value);
              if (!Number.isNaN(v) && v >= 0 && v <= 32) {
                handleVmafSegmentsChange(v);
              }
            }}
            placeholder={vmafSegmentsLoaded ? undefined : "…"}
            disabled={!vmafSegmentsLoaded || savingVmaf}
            className="w-16 text-center"
          />
          {savingVmaf && (
            <Loader2 className="h-3.5 w-3.5 animate-spin text-tertiary" />
          )}
        </div>
      </div>
    </Card>
  );
}
