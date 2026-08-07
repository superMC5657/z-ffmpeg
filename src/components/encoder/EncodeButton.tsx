import { Play } from "lucide-react";
import { useEncoderStore } from "@/store/encoderStore";
import { useQueueStore } from "@/store/queueStore";
import { useNavigate } from "react-router-dom";
import { useToastStore } from "@/store/toastStore";

export default function EncodeButton() {
  const inputFiles = useEncoderStore((s) => s.inputFiles);
  const buildConfig = useEncoderStore((s) => s.buildConfig);
  const outputDir = useEncoderStore((s) => s.outputDir);
  const clearFiles = useEncoderStore((s) => s.clearFiles);
  const addJobs = useQueueStore((s) => s.addJobs);
  const navigate = useNavigate();
  const hasFiles = inputFiles.length > 0;

  const handleAddToQueue = async () => {
    if (!hasFiles) return;
    const config = buildConfig();
    const paths = inputFiles.map((f) => f.path);
    try {
      await addJobs(paths, config, outputDir || null);
      // 添加成功后清空文件选择,避免下次误把同一批文件再次入队
      clearFiles();
      useToastStore.getState().showToast(
        `已将 ${paths.length} 个任务添加到队列`,
        "success"
      );
      navigate("/queue");
    } catch (err) {
      useToastStore.getState().showToast(
        `添加队列失败: ${err instanceof Error ? err.message : String(err)}`,
        "error"
      );
    }
  };

  return (
    <button
      onClick={handleAddToQueue}
      disabled={!hasFiles}
      className={`flex items-center gap-2 rounded-lg px-8 py-3 text-[14px] font-semibold transition-all ${
        hasFiles
          ? "bg-gradient-brand text-white shadow-lg shadow-primary/40 hover:brightness-110 active:scale-95"
          : "cursor-not-allowed bg-accent text-muted-foreground"
      }`}
    >
      <Play className="h-4 w-4" />
      添加到队列
    </button>
  );
}
