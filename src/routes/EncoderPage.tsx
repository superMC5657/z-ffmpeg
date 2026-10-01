import PageHeader from "@/components/layout/PageHeader";
import EncoderPanel from "@/components/encoder/EncoderPanel";

export default function EncoderPage() {
  return (
    <div>
      <PageHeader
        title="视频转码工作台"
        description="添加视频文件，配置编码引擎与画质参数，批量并行转码"
      />
      <EncoderPanel />
    </div>
  );
}
