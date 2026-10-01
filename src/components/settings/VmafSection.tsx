import { useEffect } from "react";
import { useQueueStore } from "@/store/queueStore";
import Card from "@/components/layout/Card";
import SettingNumberRow from "@/components/settings/SettingNumberRow";

export default function VmafSection() {
  // VMAF 段数设置（0 = 全量对比，N = N 段 × 5 秒均匀采样），队列页计算按钮用
  const vmafSegments = useQueueStore((s) => s.vmafSegments);
  const vmafSegmentsLoaded = useQueueStore((s) => s.vmafSegmentsLoaded);
  const fetchVmafSegments = useQueueStore((s) => s.fetchVmafSegments);
  const updateVmafSegments = useQueueStore((s) => s.updateVmafSegments);

  useEffect(() => {
    fetchVmafSegments();
  }, [fetchVmafSegments]);

  return (
    <Card title="VMAF 质量评估">
      <SettingNumberRow
        title="采样段数"
        description={
          <>
            设为 <b>0</b>：全量对比（逐帧计算，最精确但耗时长）。设为{" "}
            <b>N</b>：均匀取 N 段 × 5 秒计算取平均，几秒到几十秒完成。队列页「VMAF」按钮按此设置计算。
          </>
        }
        value={vmafSegments}
        loaded={vmafSegmentsLoaded}
        min={0}
        max={32}
        onSave={(v) => updateVmafSegments(v)}
        getSuccessMessage={(saved) =>
          saved === 0 ? "VMAF 已切换为全量对比" : `VMAF 采样段数已更新为 ${saved}`
        }
      />
    </Card>
  );
}
