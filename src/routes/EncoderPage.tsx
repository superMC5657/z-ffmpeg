import EncoderPanel from "@/components/encoder/EncoderPanel";
import PageHeader from "@/components/layout/PageHeader";

export default function EncoderPage() {
  return (
    <div>
      <PageHeader
        title="视频编码"
        description="选择文件，配置编码参数，添加到队列转码"
      />
      <EncoderPanel />
    </div>
  );
}
