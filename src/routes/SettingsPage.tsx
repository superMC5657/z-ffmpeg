import PageHeader from "@/components/layout/PageHeader";
import Card from "@/components/layout/Card";
import ThemeToggleButton from "@/components/layout/ThemeToggleButton";
import ZoomControl from "@/components/settings/ZoomControl";
import { useSystemStore } from "@/store/systemStore";
import LicenseSection from "@/components/settings/LicenseSection";
import QueueSection from "@/components/settings/QueueSection";
import VmafSection from "@/components/settings/VmafSection";
import FfmpegSection from "@/components/settings/FfmpegSection";
import HwAccelSection from "@/components/settings/HwAccelSection";
import UpdateSection from "@/components/settings/UpdateSection";

// 设置页展示顺序：
// 1. 最上方：外观偏好 + 软件授权
// 2. 配置参数：队列设置 + VMAF 采样评估（紧密相邻）
// 3. 底层环境：FFmpeg 状态 + 硬件加速器检测
// 4. 最下方：检查更新与版本信息
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
    <div className="space-y-6">
      <PageHeader title="设置" description="系统信息与应用配置" />

      {/* 1. 外观与授权（放在最上面） */}
      <section className="space-y-3">
        <h2 className="px-1 text-[12px] font-semibold tracking-wider text-tertiary uppercase">
          外观与授权
        </h2>
        {/* Appearance & Zoom */}
        <Card title="外观与显示" description="界面配色与显示缩放比例">
          <div className="space-y-3.5">
            <div className="flex items-center justify-between gap-4">
              <div className="min-w-0">
                <p className="text-[13px] font-medium">主题模式</p>
                <p className="mt-0.5 text-[12px] leading-5 text-secondary">
                  浅色、深色或跟随系统，跟随系统时自动适配外观变化。
                </p>
              </div>
              <ThemeToggleButton />
            </div>

            <div className="flex items-center justify-between gap-4 border-t border-hairline pt-3.5">
              <div className="min-w-0">
                <p className="text-[13px] font-medium">界面缩放</p>
                <p className="mt-0.5 text-[12px] leading-5 text-secondary">
                  调节界面显示与字体比例大小。已禁用原生 Ctrl +/- 快捷键改由此处统一控制。
                </p>
              </div>
              <ZoomControl />
            </div>
          </div>
        </Card>

        {/* License（软糖铺授权） */}
        <LicenseSection />
      </section>

      {/* 2. 配置参数（队列设置与 VMAF 质量评估） */}
      <section className="space-y-3">
        <h2 className="px-1 text-[12px] font-semibold tracking-wider text-tertiary uppercase">
          配置参数
        </h2>
        <QueueSection />
        <VmafSection />
      </section>

      {/* 3. FFmpeg 相关状态 */}
      <section className="space-y-3">
        <h2 className="px-1 text-[12px] font-semibold tracking-wider text-tertiary uppercase">
          FFmpeg 相关状态
        </h2>
        <FfmpegSection />
        <HwAccelSection />
      </section>

      {/* 4. 检查更新（放在最下面） */}
      <section className="space-y-3">
        <h2 className="px-1 text-[12px] font-semibold tracking-wider text-tertiary uppercase">
          检查更新
        </h2>
        <UpdateSection />
      </section>
    </div>
  );
}
