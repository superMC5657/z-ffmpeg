import { useEffect, useState } from "react";
import { ShieldCheck } from "lucide-react";
import { getAnalyticsEnabled, setAnalyticsEnabled } from "@/lib/tauri";
import { useToastStore } from "@/store/toastStore";
import Card from "@/components/layout/Card";
import AppleSelect from "@/components/layout/AppleSelect";
import { isTauriRuntime } from "@/lib/utils";

export default function PrivacySection() {
  // 埋点上报开关（会话结束时一次性聚合上报，可关闭）
  const [analyticsEnabled, setAnalyticsEnabledState] = useState<boolean | null>(null);
  useEffect(() => {
    if (!isTauriRuntime()) return;
    getAnalyticsEnabled()
      .then(setAnalyticsEnabledState)
      .catch(() => setAnalyticsEnabledState(null));
  }, []);

  const handleAnalyticsToggle = async (enabled: boolean) => {
    try {
      await setAnalyticsEnabled(enabled);
      setAnalyticsEnabledState(enabled);
    } catch (e) {
      useToastStore.getState().showToast(
        `保存失败: ${e instanceof Error ? e.message : String(e)}`,
        "error"
      );
    }
  };

  return (
    <Card title="隐私">
      <div className="flex items-center justify-between gap-4">
        <div className="min-w-0">
          <p className="flex items-center gap-1.5 text-[13px] font-medium">
            <ShieldCheck className="h-3.5 w-3.5 text-tertiary" />
            匿名使用统计
          </p>
          <p className="mt-0.5 text-[12px] leading-5 text-secondary">
            仅在退出时一次性上报本会话的匿名聚合数据（编码次数、编码器分布等），
            不含文件名、路径等任何个人内容。关闭后不再上报。
          </p>
        </div>
        <div className="shrink-0">
          {analyticsEnabled === null ? (
            <span className="text-[12px] text-tertiary">…</span>
          ) : (
            <AppleSelect
              className="w-20"
              value={analyticsEnabled ? "on" : "off"}
              onChange={(e) => handleAnalyticsToggle(e.target.value === "on")}
            >
              <option value="on">开启</option>
              <option value="off">关闭</option>
            </AppleSelect>
          )}
        </div>
      </div>
    </Card>
  );
}
