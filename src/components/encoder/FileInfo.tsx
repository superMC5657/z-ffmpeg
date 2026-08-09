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
    <div className="flex items-center gap-3 rounded-xl border border-border bg-card p-3 shadow-sm">
      <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-primary/10 ring-1 ring-primary/20">
        <FileVideo className="h-5 w-5 text-primary" />
      </div>

      <div className="flex-1 min-w-0">
        <p className="truncate text-sm font-medium">{file.fileName}</p>
        <div className="mt-1 flex flex-wrap gap-1.5">
          {file.probing ? (
            <span className="flex items-center gap-1.5 rounded-md bg-accent px-2 py-0.5 text-[13px] font-medium text-muted-foreground">
              <Loader2 className="h-3 w-3 animate-spin" />
              分析中...
            </span>
          ) : (
            <>
              <span className="rounded-md bg-accent px-2 py-0.5 text-[13px] font-medium text-muted-foreground">
                {formatFileSize(file.fileSize)}
              </span>
              {file.duration != null && (
                <span className="rounded-md bg-accent px-2 py-0.5 text-[13px] font-medium text-muted-foreground">
                  {formatDuration(file.duration)}
                </span>
              )}
              {file.width && file.height && (
                <span className="rounded-md bg-primary/10 px-2 py-0.5 text-[13px] font-medium text-primary">
                  {file.width}×{file.height}
                </span>
              )}
              {file.videoCodec && (
                <span className="rounded-md bg-accent px-2 py-0.5 text-[13px] font-medium uppercase text-muted-foreground">
                  {file.videoCodec}
                </span>
              )}
              {/* 预估输出体积：按当前编码参数推算，参数变化自动刷新 */}
              {estimatedSize != null && (
                <span
                  title="按当前编码参数预估的输出体积（仅供参考）"
                  className="rounded-md bg-primary/10 px-2 py-0.5 text-[13px] font-medium text-primary"
                >
                  预计 {formatFileSizeCompact(estimatedSize)}
                </span>
              )}
              {file.probeError && (
                <span className="rounded-md bg-destructive/15 px-2 py-0.5 text-[13px] font-medium text-destructive">
                  无法解析
                </span>
              )}
            </>
          )}
        </div>
      </div>

      <button
        onClick={(e) => {
          e.stopPropagation();
          removeFile(index);
        }}
        className="flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-destructive/20 hover:text-destructive"
      >
        <X className="h-4 w-4" />
      </button>
    </div>
  );
}
