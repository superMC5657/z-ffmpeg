import EncoderPanel from "@/components/encoder/EncoderPanel";

export default function EncoderPage() {
  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between pb-1">
        <div>
          <h1 className="text-[22px] font-bold tracking-tight text-foreground">
            视频转码工作台
          </h1>
          <p className="mt-0.5 text-[13px] text-secondary">
            添加视频文件，配置编码引擎与画质参数，批量并行转码
          </p>
        </div>
      </div>
      <EncoderPanel />
    </div>
  );
}
