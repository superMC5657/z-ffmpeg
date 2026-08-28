import { X, FileVideo, Loader2 } from "lucide-react";
import type { FileInfo as FileInfoType } from "@/types";
import { useEncoderStore } from "@/store/encoderStore";
import { formatFileSize, formatDuration, formatFileSizeCompact } from "@/lib/utils";

interface FileInfoProps {
  file: FileInfoType;
  index: number;
}

export default function FileInfo({ file, index }: FileInfoProps) {
  const removeFile = useEncoderStore((s) => s.removeFile);
  const estimatedSize = useEncoderStore((s) => s.estimatedSizes[file.path]);

  return (
    <div className="group flex items-center gap-3 rounded-[10px] bg-fill/70 p-2.5 transition-colors hover:bg-fill">
      <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-accent/10">
        <FileVideo className="h-4.5 w-4.5 text-accent" />
      </div>

      <div className="min-w-0 flex-1">
        <p className="truncate text-[13px] font-medium leading-5">{file.fileName}</p>
        <div className="mt-0.5 flex flex-wrap items-center gap-x-2 gap-y-0.5 text-[11px] text-secondary">
          {file.probing ? (
            <span className="flex items-center gap-1">
              <Loader2 className="h-3 w-3 animate-spin" />
              分析中…
            </span>
          ) : (
            <>
              <span className="tabular-nums">{formatFileSize(file.fileSize)}</span>
              {file.duration != null && (
                <>
                  <span className="text-tertiary">·</span>
                  <span className="tabular-nums">{formatDuration(file.duration)}</span>
                </>
              )}
              {file.videoCodec && (
                <>
                  <span className="text-tertiary">·</span>
                  <span className="uppercase">{file.videoCodec}</span>
                </>
              )}
              {file.width && file.height && (
                <>
                  <span className="text-tertiary">·</span>
                  <span className="tabular-nums">{file.width}×{file.height}</span>
                </>
              )}
              {/* 预估输出体积：按当前编码参数推算，参数变化自动刷新 */}
              {estimatedSize != null && (
                <span
                  title="按当前编码参数预估的输出体积（仅供参考）"
                  className="font-medium text-accent"
                >
                  预计 {formatFileSizeCompact(estimatedSize)}
                </span>
              )}
              {file.probeError && (
                <span className="font-medium text-destructive">无法解析</span>
              )}
            </>
          )}
        </div>
      </div>

      <button
        aria-label={`移除 ${file.fileName}`}
        onClick={(e) => {
          e.stopPropagation();
          removeFile(index);
        }}
        className="flex h-6 w-6 shrink-0 items-center justify-center rounded-md text-tertiary opacity-0 transition-all hover:bg-destructive/10 hover:text-destructive group-hover:opacity-100"
      >
        <X className="h-3.5 w-3.5" />
      </button>
    </div>
  );
}
