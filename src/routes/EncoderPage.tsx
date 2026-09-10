import EncoderPanel from "@/components/encoder/EncoderPanel";
import PageHeader from "@/components/layout/PageHeader";

export default function EncoderPage() {
  return (
    <div className="space-y-4">
      <PageHeader
        title="工作台"
        description="添加视频文件，配置编码引擎与画质参数，批量并行转码"
      />
      <EncoderPanel />
    </div>
  );
}
