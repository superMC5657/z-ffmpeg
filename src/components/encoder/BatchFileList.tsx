import { useCallback, useRef, useState } from "react";
import {
  Plus,
  Trash2,
  FileVideo,
  Loader2,
  X,
  UploadCloud,
  HardDrive,
  CheckCircle2,
} from "lucide-react";
import { useEncoderStore } from "@/store/encoderStore";
import { useToastStore } from "@/store/toastStore";
import { open } from "@tauri-apps/plugin-dialog";
import {
  formatFileSize,
  formatDuration,
  formatFileSizeCompact,
  formatBitrate,
  cn,
} from "@/lib/utils";

const SUPPORTED_FORMATS = [
  "MP4",
  "MKV",
  "WebM",
  "MOV",
  "AVI",
  "WMV",
  "FLV",
  "TS",
];

export default function BatchFileList() {
  const inputFiles = useEncoderStore((s) => s.inputFiles);
  const addFiles = useEncoderStore((s) => s.addFiles);
  const removeFile = useEncoderStore((s) => s.removeFile);
  const clearFiles = useEncoderStore((s) => s.clearFiles);
  const estimatedSizes = useEncoderStore((s) => s.estimatedSizes);

  const [isDragOver, setIsDragOver] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const handlePickFiles = async () => {
    try {
      const selected = await open({
        multiple: true,
        filters: [
          {
            name: "视频文件",
            extensions: [
              "mp4",
              "mkv",
              "webm",
              "mov",
              "avi",
              "wmv",
              "flv",
              "m4v",
              "ts",
            ],
          },
          { name: "所有文件", extensions: ["*"] },
        ],
      });
      if (selected) {
        const paths = Array.isArray(selected) ? selected : [selected];
        await addFiles(paths as string[]);
        useToastStore
          .getState()
          .showToast(`已添加 ${paths.length} 个视频文件`, "success");
      }
    } catch {
      fileInputRef.current?.click();
    }
  };

  const handleHtmlFileChange = async (
    e: React.ChangeEvent<HTMLInputElement>
  ) => {
    const files = e.target.files;
    if (!files || files.length === 0) return;
    const paths: string[] = [];
    for (let i = 0; i < files.length; i++) {
      // @ts-expect-error - path property in Electron/Tauri
      paths.push(files[i].path || files[i].name);
    }
    await addFiles(paths);
    useToastStore
      .getState()
      .showToast(`已添加 ${paths.length} 个视频文件`, "success");
  };

  const handleDrop = useCallback(
    async (e: React.DragEvent) => {
      e.preventDefault();
      setIsDragOver(false);
      const files = e.dataTransfer.files;
      if (files.length > 0) {
        const paths: string[] = [];
        for (let i = 0; i < files.length; i++) {
          // @ts-expect-error - path in webview
          paths.push(files[i].path || files[i].name);
        }
        await addFiles(paths);
        useToastStore
          .getState()
          .showToast(`已添加 ${paths.length} 个视频文件`, "success");
      }
    },
    [addFiles]
  );

  const totalInputSize = inputFiles.reduce((acc, f) => acc + (f.fileSize || 0), 0);
  const totalEstimatedSize = Object.values(estimatedSizes).reduce<number>(
    (a, b) => a + (b ?? 0),
    0
  );

  return (
    <div
      onDragOver={(e) => {
        e.preventDefault();
        setIsDragOver(true);
      }}
      onDragLeave={() => setIsDragOver(false)}
      onDrop={handleDrop}
      className={cn(
        "flex flex-col rounded-2xl border transition-all duration-200 bg-surface/70 backdrop-blur-md shadow-card overflow-hidden",
        isDragOver
          ? "border-accent bg-accent/[0.04] ring-2 ring-accent/20"
          : "border-hairline"
      )}
    >
      <input
        ref={fileInputRef}
        type="file"
        accept="video/*"
        multiple
        className="hidden"
        onChange={handleHtmlFileChange}
      />

      {/* 列表头部操作栏 */}
      <div className="flex items-center justify-between border-b border-hairline px-5 py-3.5 bg-fill/25">
        <div className="flex items-center gap-2.5">
          <span className="text-[15px] font-bold tracking-tight text-foreground">
            待转码清单
          </span>
          {inputFiles.length > 0 && (
            <span className="rounded-full bg-accent/15 px-2.5 py-0.5 text-[12px] font-semibold text-accent tabular-nums">
              {inputFiles.length} 个文件
            </span>
          )}
        </div>

        <div className="flex items-center gap-2">
          {inputFiles.length > 0 && (
            <button
              onClick={clearFiles}
              className="flex h-8 items-center gap-1.5 rounded-lg px-2.5 text-[13px] text-secondary transition-colors hover:bg-destructive/10 hover:text-destructive"
              title="清空全部待处理文件"
            >
              <Trash2 className="h-4 w-4" />
              <span>清空全部</span>
            </button>
          )}
          <button
            onClick={handlePickFiles}
            className="flex h-8 items-center gap-1.5 rounded-lg bg-accent px-3 text-[13px] font-semibold text-on-accent shadow-xs transition-transform active:scale-95 hover:bg-accent-hover"
          >
            <Plus className="h-4 w-4" strokeWidth={2.4} />
            <span>添加视频</span>
          </button>
        </div>
      </div>

      {/* 内容区域：空状态 vs 文件卡片列表 */}
      <div className="flex-1 p-4">
        {inputFiles.length === 0 ? (
          /* 空状态：大幅面拖拽投送区 */
          <div
            onClick={handlePickFiles}
            className={cn(
              "flex flex-col items-center justify-center rounded-xl border border-dashed py-20 px-8 text-center cursor-pointer transition-all duration-200",
              isDragOver
                ? "border-accent bg-accent/[0.08]"
                : "border-hairline/80 bg-fill/20 hover:border-accent/50 hover:bg-fill/40"
            )}
          >
            <div className="flex h-16 w-16 items-center justify-center rounded-2xl bg-accent/15 text-accent shadow-sm mb-4 transition-transform duration-200 group-hover:scale-105">
              <UploadCloud className="h-8 w-8" strokeWidth={1.9} />
            </div>
            <h3 className="text-[16px] font-bold text-foreground">
              拖拽视频文件到此处，或点击浏览添加
            </h3>
            <p className="mt-1.5 text-[13px] text-secondary max-w-md leading-relaxed">
              支持单视频转换与多视频批量并行编码，自动解析分辨率、帧率与体积预估
            </p>
            <div className="mt-5 flex flex-wrap justify-center gap-2 max-w-lg">
              {SUPPORTED_FORMATS.map((fmt) => (
                <span
                  key={fmt}
                  className="rounded-lg border border-hairline/80 bg-surface px-2.5 py-1 text-[12px] font-medium text-tertiary"
                >
                  {fmt}
                </span>
              ))}
            </div>
          </div>
        ) : (
          /* 文件项列表 */
          <div className="space-y-2.5 max-h-[520px] overflow-y-auto pr-0.5">
            {inputFiles.map((file, index) => {
              const estimatedSize = estimatedSizes[file.path];
              const ratio =
                file.fileSize > 0 && estimatedSize != null
                  ? Math.round(((estimatedSize - file.fileSize) / file.fileSize) * 100)
                  : null;
              const effectiveBitrate =
                file.bitrate ||
                (file.duration && file.fileSize > 0
                  ? Math.round((file.fileSize * 8) / file.duration)
                  : null);

              return (
                <div
                  key={file.path}
                  className="group relative flex items-center gap-3.5 rounded-xl border border-hairline/80 bg-fill/40 p-3.5 transition-all hover:bg-fill/70 hover:border-hairline"
                >
                  {/* 视频图标 / 时长 */}
                  <div className="relative flex h-12 w-12 shrink-0 items-center justify-center rounded-xl bg-accent/10 text-accent">
                    <FileVideo className="h-6 w-6" />
                    {file.duration != null && (
                      <span className="absolute -bottom-1 -right-1 rounded bg-surface/90 px-1.5 py-0.2 text-[10px] font-semibold text-secondary shadow-xs tabular-nums ring-1 ring-hairline">
                        {formatDuration(file.duration)}
                      </span>
                    )}
                  </div>

                  {/* 视频元数据与预估信息 */}
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <p
                        className="truncate text-[14px] font-semibold text-foreground leading-tight"
                        title={file.path}
                      >
                        {file.fileName}
                      </p>
                    </div>

                    <div className="mt-1.5 flex flex-wrap items-center gap-x-2.5 gap-y-1 text-[12px] text-secondary">
                      {file.probing ? (
                        <span className="flex items-center gap-1.5 text-accent font-medium">
                          <Loader2 className="h-3.5 w-3.5 animate-spin" />
                          正在解析视频元数据…
                        </span>
                      ) : (
                        <>
                          <span className="tabular-nums font-medium text-foreground/90">
                            {formatFileSize(file.fileSize)}
                          </span>

                          {file.width && file.height && (
                            <>
                              <span className="text-tertiary">·</span>
                              <span className="tabular-nums font-medium">
                                {file.width}×{file.height}
                              </span>
                            </>
                          )}

                          {file.videoCodec && (
                            <>
                              <span className="text-tertiary">·</span>
                              <span className="uppercase font-mono font-medium">
                                {file.videoCodec}
                              </span>
                            </>
                          )}

                          {file.frameRate && (
                            <>
                              <span className="text-tertiary">·</span>
                              <span className="tabular-nums">
                                {Math.round(file.frameRate)} fps
                              </span>
                            </>
                          )}

                          {effectiveBitrate != null && effectiveBitrate > 0 && (
                            <>
                              <span className="text-tertiary">·</span>
                              <span
                                className="tabular-nums font-medium text-foreground/80"
                                title={`源视频平均码率：${Math.round(effectiveBitrate / 1000)} kbps`}
                              >
                                {formatBitrate(Math.round(effectiveBitrate / 1000))}
                              </span>
                            </>
                          )}

                          {/* 预估输出体积胶囊 */}
                          {estimatedSize != null && (
                            <span
                              title="根据当前编码器与质量参数计算出的预期体积"
                              className={cn(
                                "ml-1.5 inline-flex items-center gap-1 rounded-md px-2 py-0.5 text-[11px] font-bold tabular-nums",
                                ratio !== null && ratio < 0
                                  ? "bg-success/15 text-success"
                                  : "bg-accent/15 text-accent"
                              )}
                            >
                              预计 {formatFileSizeCompact(estimatedSize)}
                              {ratio !== null && (
                                <span className="opacity-80 font-normal">
                                  ({ratio > 0 ? `+${ratio}%` : `${ratio}%`})
                                </span>
                              )}
                            </span>
                          )}

                          {file.probeError && (
                            <span className="text-destructive font-semibold">
                              无法解析
                            </span>
                          )}
                        </>
                      )}
                    </div>
                  </div>

                  {/* 移除当前项按钮 */}
                  <button
                    aria-label={`移除 ${file.fileName}`}
                    onClick={() => removeFile(index)}
                    className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg text-tertiary opacity-40 transition-all hover:bg-destructive/10 hover:text-destructive hover:opacity-100 group-hover:opacity-100"
                  >
                    <X className="h-4.5 w-4.5" />
                  </button>
                </div>
              );
            })}

            {/* 底部投送提示条 */}
            <div
              onClick={handlePickFiles}
              className="flex items-center justify-center gap-2 rounded-xl border border-dashed border-hairline/70 py-3 text-[13px] text-secondary cursor-pointer hover:border-accent/40 hover:text-foreground transition-colors"
            >
              <Plus className="h-4 w-4" />
              <span>拖入更多视频文件，或点击继续添加</span>
            </div>
          </div>
        )}
      </div>

      {/* 底部汇总统计条 */}
      {inputFiles.length > 0 && (
        <div className="flex items-center justify-between border-t border-hairline px-5 py-3 bg-fill/20 text-[12px] text-secondary">
          <div className="flex items-center gap-2 tabular-nums">
            <HardDrive className="h-4 w-4 text-tertiary" />
            <span>
              原始总体积:{" "}
              <strong className="text-foreground font-semibold">
                {formatFileSize(totalInputSize)}
              </strong>
            </span>
          </div>
          {totalEstimatedSize > 0 && (
            <div className="flex items-center gap-1.5 tabular-nums">
              <CheckCircle2 className="h-4 w-4 text-success" />
              <span>
                预计总体积:{" "}
                <strong className="text-success font-bold">
                  ≈ {formatFileSizeCompact(totalEstimatedSize)}
                </strong>
              </span>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
