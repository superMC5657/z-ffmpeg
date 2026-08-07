import { useCallback, useRef, useState } from "react";
import { Upload, FolderOpen } from "lucide-react";
import { useEncoderStore } from "@/store/encoderStore";
import { useToastStore } from "@/store/toastStore";
import { open } from "@tauri-apps/plugin-dialog";

const FORMATS = ["MP4", "MKV", "WebM", "MOV", "AVI", "FLV", "TS"];

export default function FileSelector() {
  const addFiles = useEncoderStore((s) => s.addFiles);
  const [isDragOver, setIsDragOver] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  const handleClick = async () => {
    // Use Tauri dialog for native file picker
    try {
      const selected = await open({
        multiple: true,
        filters: [
          {
            name: "视频文件",
            extensions: ["mp4", "mkv", "webm", "mov", "avi", "wmv", "flv", "m4v", "ts"],
          },
          { name: "所有文件", extensions: ["*"] },
        ],
      });
      if (selected) {
        const paths = Array.isArray(selected) ? selected : [selected];
        await addFiles(paths as string[]);
        useToastStore.getState().showToast(
          `已添加 ${paths.length} 个文件`,
          "success"
        );
      }
    } catch {
      // Fallback to HTML input
      inputRef.current?.click();
    }
  };

  const handleFileChange = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const files = e.target.files;
    if (!files) return;
    const paths: string[] = [];
    for (let i = 0; i < files.length; i++) {
      // @ts-expect-error - webkitRelativePath may exist
      paths.push(files[i].path || files[i].name);
    }
    if (paths.length > 0) {
      await addFiles(paths);
      useToastStore.getState().showToast(`已添加 ${paths.length} 个文件`, "success");
    }
  };

  const handleDrop = useCallback(
    async (e: React.DragEvent) => {
      e.preventDefault();
      setIsDragOver(false);
      // Tauri's drag-drop provides file paths via dataTransfer
      // For browser env, we use File API
      const files = e.dataTransfer.files;
      if (files.length > 0) {
        const paths: string[] = [];
        for (let i = 0; i < files.length; i++) {
          // @ts-expect-error - path may exist in Tauri
          paths.push(files[i].path || files[i].name);
        }
        if (paths.length > 0) {
          await addFiles(paths);
          useToastStore.getState().showToast(
            `已添加 ${paths.length} 个文件`,
            "success"
          );
        }
      }
    },
    [addFiles]
  );

  return (
    <div
      onDragOver={(e) => {
        e.preventDefault();
        setIsDragOver(true);
      }}
      onDragLeave={() => setIsDragOver(false)}
      onDrop={handleDrop}
      onClick={handleClick}
      className={`flex cursor-pointer flex-col items-center justify-center rounded-xl border-2 border-dashed p-10 transition-all duration-200 ${
        isDragOver
          ? "scale-[1.01] border-primary bg-primary/5"
          : "border-muted-foreground/25 hover:border-primary/40 hover:bg-accent/20"
      }`}
    >
      <input
        ref={inputRef}
        type="file"
        accept="video/*"
        multiple
        className="hidden"
        onChange={handleFileChange}
      />
      <div className="flex flex-col items-center gap-3">
        <div
          className={`flex h-14 w-14 items-center justify-center rounded-2xl bg-gradient-brand shadow-lg shadow-primary/25 transition-transform duration-200 ${
            isDragOver ? "scale-110" : ""
          }`}
        >
          {isDragOver ? (
            <Upload className="h-6 w-6 text-white" />
          ) : (
            <FolderOpen className="h-6 w-6 text-white" />
          )}
        </div>
        <div className="text-center">
          <p className="text-sm font-medium">
            {isDragOver ? "释放以添加文件" : "拖拽视频文件到此处"}
          </p>
          <p className="mt-1 text-[13px] text-muted-foreground">
            或点击浏览文件 · 支持常见视频格式
          </p>
        </div>
        <div className="flex flex-wrap justify-center gap-1.5">
          {FORMATS.map((f) => (
            <span
              key={f}
              className="rounded-md border border-border bg-accent/60 px-2 py-0.5 text-[13px] font-medium text-muted-foreground"
            >
              {f}
            </span>
          ))}
        </div>
      </div>
    </div>
  );
}
