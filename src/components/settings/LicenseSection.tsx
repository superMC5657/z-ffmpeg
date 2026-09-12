import { useState } from "react";
import { Crown } from "lucide-react";
import { useToastStore } from "@/store/toastStore";
import { useLicenseStore } from "@/store/licenseStore";
import Card from "@/components/layout/Card";
import { cn } from "@/lib/utils";

export default function LicenseSection() {
  const licenseStatus = useLicenseStore((s) => s.status);
  const licenseWorking = useLicenseStore((s) => s.working);
  const setActivationOpen = useLicenseStore((s) => s.setActivationOpen);
  const deactivateLicenseAction = useLicenseStore((s) => s.deactivate);
  const [confirmingDeactivate, setConfirmingDeactivate] = useState(false);
  const isPro = licenseStatus?.pro === true;

  const handleDeactivate = async () => {
    try {
      await deactivateLicenseAction();
      useToastStore.getState().showToast("已注销激活，本机已停用 Pro 功能", "success");
    } catch (e) {
      useToastStore.getState().showToast(
        `注销失败: ${e instanceof Error ? e.message : String(e)}`,
        "error"
      );
    } finally {
      setConfirmingDeactivate(false);
    }
  };

  return (
    <Card
      title="授权"
      action={
        isPro ? undefined : (
          <button
            onClick={() => setActivationOpen(true)}
            className="flex h-9 items-center gap-1.5 rounded-[9px] bg-accent px-4 text-[13px] font-medium text-on-accent shadow-sm transition-all hover:bg-accent-hover active:scale-[0.98]"
          >
            <Crown className="h-3.5 w-3.5" />
            激活 Pro
          </button>
        )
      }
    >
      <div className="space-y-2.5 text-[13px]">
        <div className="flex items-center gap-2">
          <span
            className={cn(
              "h-2 w-2 rounded-full",
              isPro ? "bg-success" : "bg-tertiary"
            )}
          />
          <span className="font-medium">
            {isPro
              ? licenseStatus?.levelLabel ?? "专业版"
              : "免费版"}
          </span>
          {licenseStatus?.offline && (
            <span className="rounded-full bg-warning/15 px-2 py-0.5 text-[11px] leading-4 text-warning">
              离线宽限期（网络恢复后自动续验）
            </span>
          )}
        </div>
        {isPro && licenseStatus?.expiresAt && (
          <p className="text-secondary">
            授权到期：{new Date(licenseStatus.expiresAt).toLocaleString()}
          </p>
        )}
        {isPro && licenseStatus?.email && (
          <p className="text-secondary">购买邮箱：{licenseStatus.email}</p>
        )}
        {isPro && licenseStatus?.code && (
          <p className="font-mono text-secondary">激活码：{licenseStatus.code}</p>
        )}
        {!isPro && (
          <p className="text-[12px] leading-5 text-secondary">
            免费版包含全部基础编码、硬件加速、并发调度与预设管理功能；VMAF 质量对比、命令导出为脚本文件需要激活 Pro。
          </p>
        )}
        {isPro && (
          <div className="pt-1">
            {confirmingDeactivate ? (
              <div className="flex flex-wrap items-center gap-2">
                <span className="text-[12px] text-secondary">
                  注销将释放一个设备名额，需重新激活才能继续使用。确认注销？
                </span>
                <button
                  onClick={handleDeactivate}
                  disabled={licenseWorking}
                  className="h-8 rounded-[9px] bg-destructive px-3 text-[12px] font-medium text-white transition-colors hover:bg-destructive/90 disabled:opacity-50"
                >
                  {licenseWorking ? "注销中..." : "确认注销"}
                </button>
                <button
                  onClick={() => setConfirmingDeactivate(false)}
                  className="h-8 rounded-[9px] bg-fill px-3 text-[12px] font-medium transition-colors hover:bg-fill-strong"
                >
                  取消
                </button>
              </div>
            ) : (
              <button
                onClick={() => setConfirmingDeactivate(true)}
                className="h-8 rounded-[9px] bg-fill px-3 text-[12px] font-medium text-secondary transition-colors hover:bg-fill-strong hover:text-foreground"
              >
                注销激活（换机前使用）
              </button>
            )}
          </div>
        )}
      </div>
    </Card>
  );
}
