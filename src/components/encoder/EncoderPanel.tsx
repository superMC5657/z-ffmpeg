import BatchFileList from "./BatchFileList";
import UnifiedInspector from "./UnifiedInspector";

export default function EncoderPanel() {
  return (
    <div className="grid grid-cols-1 lg:grid-cols-[minmax(0,1fr)_420px] xl:grid-cols-[minmax(0,1fr)_460px] items-start gap-6">
      {/* 左栏：文件批处理工作区 */}
      <div className="min-w-0">
        <BatchFileList />
      </div>

      {/* 右栏：一体化编码检查器 */}
      <aside className="w-full lg:sticky lg:top-4">
        <UnifiedInspector />
      </aside>
    </div>
  );
}
