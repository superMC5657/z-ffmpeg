import { useState } from "react";
import { Crown, X, ExternalLink } from "lucide-react";
import AppleInput from "@/components/layout/AppleInput";
import { useLicenseStore } from "@/store/licenseStore";
import { useToastStore } from "@/store/toastStore";
import { isTauriRuntime } from "@/lib/utils";

/**
 * 全局激活对话框：由 licenseStore.activationOpen 驱动，
 * 门控 UI（ProGate / 硬件加速卡片等）点击时拉起，挂在 App 根部。
 */
export default function ActivationDialog() {
  const open = useLicenseStore((s) => s.activationOpen);
  const setOpen = useLicenseStore((s) => s.setActivationOpen);
  const activate = useLicenseStore((s) => s.activate);
  const working = useLicenseStore((s) => s.working);
  const storedCode = useLicenseStore((s) => s.status?.code ?? "");
  const storedEmail = useLicenseStore((s) => s.status?.email ?? "");
  const buyUrl = useLicenseStore((s) => s.status?.buyUrl ?? null);

  const [code, setCode] = useState("");
  const [email, setEmail] = useState("");
  const [error, setError] = useState<string | null>(null);

  const openBuyUrl = async () => {
    if (!buyUrl) return;
    try {
      if (isTauriRuntime()) {
        const { open } = await import("@tauri-apps/plugin-shell");
        await open(buyUrl);
      } else {
        window.open(buyUrl, "_blank");
      }
    } catch {
      useToastStore.getState().showToast("打开购买页失败", "error");
    }
  };

  if (!open) return null;

  const effectiveCode = code || storedCode;
  const effectiveEmail = email || storedEmail;

  const handleActivate = async () => {
    if (working) return;
    setError(null);
    try {
      await activate(effectiveCode.trim(), effectiveEmail.trim());
      useToastStore.getState().showToast("激活成功，已解锁 Pro 功能", "success");
      setCode("");
      setEmail("");
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-overlay backdrop-blur-[2px]"
      onClick={() => setOpen(false)}
    >
      <div
        className="animate-dialog-in w-full max-w-md rounded-[14px] border border-hairline bg-surface p-5 shadow-pop"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="mb-4 flex items-center justify-between">
          <h2 className="flex items-center gap-2 text-[15px] font-semibold">
            <Crown className="h-4 w-4 text-amber-500" />
            激活 Pro 版
          </h2>
          <button
            aria-label="关闭"
            onClick={() => setOpen(false)}
            className="flex h-7 w-7 items-center justify-center rounded-full text-tertiary transition-colors hover:bg-fill-strong hover:text-foreground"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="space-y-3">
          <div>
            <label className="mb-1.5 block text-[12px] font-medium text-secondary">
              激活码
            </label>
            <AppleInput
              value={code || storedCode}
              onChange={(e) => setCode(e.target.value.toUpperCase())}
              placeholder="XXXX-XXXX-XXXX-XXXX"
              className="h-10 w-full font-mono text-[14px] tracking-wider"
              autoFocus
            />
          </div>
          <div>
            <label className="mb-1.5 block text-[12px] font-medium text-secondary">
              购买邮箱
            </label>
            <AppleInput
              type="email"
              value={email || storedEmail}
              onChange={(e) => setEmail(e.target.value)}
              placeholder="下单时填写的邮箱"
              className="h-10 w-full text-[14px]"
            />
          </div>
          <div className="rounded-[9px] bg-fill p-2.5">
            <p className="text-[12px] leading-5 text-secondary">
              激活码与购买邮箱绑定，一个激活码可绑定多台设备。换机前请在原设备「注销激活」释放名额。
              {buyUrl && (
                <>
                  {" "}还没有激活码？
                  <button
                    onClick={openBuyUrl}
                    className="inline-flex items-center gap-0.5 font-medium text-accent hover:underline"
                  >
                    去购买
                    <ExternalLink className="h-3 w-3" />
                  </button>
                </>
              )}
            </p>
          </div>
          {error && (
            <div className="rounded-[9px] bg-destructive/10 p-2.5">
              <p className="text-[12px] leading-5 text-destructive">{error}</p>
            </div>
          )}
        </div>

        <div className="mt-5 flex justify-end gap-2">
          <button
            onClick={() => setOpen(false)}
            className="h-9 rounded-[9px] bg-fill px-4 text-[13px] font-medium text-foreground transition-colors hover:bg-fill-strong active:scale-[0.98]"
          >
            取消
          </button>
          <button
            onClick={handleActivate}
            disabled={!effectiveCode.trim() || !effectiveEmail.trim() || working}
            className={`h-9 rounded-[9px] px-4 text-[13px] font-medium shadow-sm transition-all active:scale-[0.98] disabled:cursor-default disabled:opacity-50 ${
              effectiveCode.trim() && effectiveEmail.trim() && !working
                ? "bg-accent text-on-accent hover:bg-accent-hover"
                : "bg-fill text-tertiary"
            }`}
          >
            {working ? "激活中..." : "激活"}
          </button>
        </div>
      </div>
    </div>
  );
}
