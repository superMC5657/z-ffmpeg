import { useEffect } from "react";
import { useQueueStore } from "@/store/queueStore";
import Card from "@/components/layout/Card";
import SettingNumberRow from "@/components/settings/SettingNumberRow";

export default function QueueSection() {
  // 与队列页面共享——此处修改将双向同步
  const maxConcurrent = useQueueStore((s) => s.maxConcurrent);
  const maxConcurrentLoaded = useQueueStore((s) => s.maxConcurrentLoaded);
  const fetchMaxConcurrent = useQueueStore((s) => s.fetchMaxConcurrent);
  const updateMaxConcurrent = useQueueStore((s) => s.updateMaxConcurrent);

  useEffect(() => {
    fetchMaxConcurrent();
  }, [fetchMaxConcurrent]);

  return (
    <Card title="队列设置">
      <SettingNumberRow
        title="最大并发编码任务数"
        description="与队列页同步，任一处修改立即生效并保存。硬件加速下 2-4 个即可占满显卡，软件编码可适当调高（支持 1-16 并发）。"
        value={Math.min(maxConcurrent, 16)}
        loaded={maxConcurrentLoaded}
        min={1}
        max={16}
        onSave={(v) => updateMaxConcurrent(v)}
        getSuccessMessage={(saved) => `队列并发数已更新为 ${saved}`}
      />
    </Card>
  );
}
