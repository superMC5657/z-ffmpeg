import type { ReactNode } from "react";
import { useState } from "react";
import { Loader2 } from "lucide-react";
import AppleInput from "@/components/layout/AppleInput";
import { useToastStore, showErrorToast } from "@/store/toastStore";
import { cn } from "@/lib/utils";

export interface SettingNumberRowProps {
  /** 设置项主标题 */
  title: ReactNode;
  /** 设置项说明文案 */
  description?: ReactNode;
  /** 当前数值 */
  value: number;
  /** 数据是否已从存储层加载完成 */
  loaded?: boolean;
  /** 允许的最小值 */
  min?: number;
  /** 允许的最大值 */
  max?: number;
  /** 步长 */
  step?: number;
  /** 是否禁用输入 */
  disabled?: boolean;
  /** 外层容器自定义样式 */
  className?: string;
  /** 数字输入框自定义样式 */
  inputClassName?: string;
  /** 保存回调 */
  onSave: (value: number) => Promise<number | void>;
  /** 保存成功时的 Toast 提示内容（不提供则不弹 Toast） */
  getSuccessMessage?: (saved: number) => string;
  /** 保存失败时的 Toast 提示前缀 */
  errorMessage?: string;
}

export default function SettingNumberRow({
  title,
  description,
  value,
  loaded = true,
  min,
  max,
  step,
  disabled = false,
  className,
  inputClassName,
  onSave,
  getSuccessMessage,
  errorMessage = "保存失败",
}: SettingNumberRowProps) {
  const [saving, setSaving] = useState(false);

  const handleChange = async (newVal: number) => {
    setSaving(true);
    try {
      const saved = await onSave(newVal);
      const savedNum = typeof saved === "number" ? saved : newVal;
      if (getSuccessMessage) {
        useToastStore.getState().showToast(getSuccessMessage(savedNum), "success");
      }
    } catch (e) {
      showErrorToast(errorMessage, e);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className={cn("flex items-center justify-between gap-4", className)}>
      <div className="min-w-0">
        <p className="text-[13px] font-medium">{title}</p>
        {description && (
          <p className="mt-0.5 text-[12px] leading-5 text-secondary">
            {description}
          </p>
        )}
      </div>
      <div className="flex shrink-0 items-center gap-2">
        <AppleInput
          type="number"
          min={min}
          max={max}
          step={step}
          value={loaded ? value : ""}
          onChange={(e) => {
            const v = parseInt(e.target.value, 10);
            if (!Number.isNaN(v)) {
              if (min != null && v < min) return;
              if (max != null && v > max) return;
              void handleChange(v);
            }
          }}
          placeholder={loaded ? undefined : "…"}
          disabled={disabled || !loaded || saving}
          className={cn("w-16 text-center", inputClassName)}
        />
        {saving && (
          <Loader2 className="h-3.5 w-3.5 animate-spin text-tertiary" />
        )}
      </div>
    </div>
  );
}
