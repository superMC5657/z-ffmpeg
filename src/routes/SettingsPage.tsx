import PageHeader from "@/components/layout/PageHeader";
import Card from "@/components/layout/Card";
import ThemeToggleButton from "@/components/layout/ThemeToggleButton";
import { useSystemStore } from "@/store/systemStore";
import FfmpegSection from "@/components/settings/FfmpegSection";
import QueueSection from "@/components/settings/QueueSection";
import LicenseSection from "@/components/settings/LicenseSection";
import VmafSection from "@/components/settings/VmafSection";
import HwAccelSection from "@/components/settings/HwAccelSection";
import UpdateSection from "@/components/settings/UpdateSection";

// 设置页只是各分区的组装：每个 Card 的数据获取与交互收拢在
// `src/components/settings/` 对应分区内（含各自的 store 订阅与 effect）。
// 注意：骨架屏只等硬件加速检测；FFmpeg 状态由 FfmpegSection 内展示加载态，
// 各分区互不阻塞（此前整页等 FFmpeg + 硬件检测全部完成）。
export default function SettingsPage() {
  const loadingHw = useSystemStore((s) => s.loading);

  if (loadingHw) {
    return (
      <div className="space-y-4">
        <PageHeader title="设置" description="系统信息与应用配置" />
        <div className="space-y-3">
          {[0, 1, 2].map((i) => (
            <div key={i} className="h-24 animate-pulse rounded-[14px] bg-fill/70" />
          ))}
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <PageHeader title="设置" description="系统信息与应用配置" />

      {/* Appearance */}
      <Card title="外观" description="浅色、深色或跟随系统，切换立即生效">
        <div className="flex items-center justify-between gap-4">
          <p className="text-[13px] leading-5 text-secondary">
            浅色、深色或跟随系统，跟随系统时自动适配外观变化。
          </p>
          <ThemeToggleButton />
        </div>
      </Card>

      {/* Queue Settings */}
      <QueueSection />

      {/* License（软糖铺授权） */}
      <LicenseSection />

      {/* VMAF Settings */}
      <VmafSection />

      {/* FFmpeg Status */}
      <FfmpegSection />

      {/* Hardware Accelerators */}
      <HwAccelSection />

      {/* Software Updates */}
      <UpdateSection />
    </div>
  );
}
