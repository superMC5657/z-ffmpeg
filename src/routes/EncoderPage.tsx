import { Video } from "lucide-react";
import EncoderPanel from "@/components/encoder/EncoderPanel";
import PageHeader from "@/components/layout/PageHeader";

export default function EncoderPage() {
  return (
    <div className="mx-auto max-w-5xl space-y-8">
      <PageHeader
        icon={Video}
        title="视频编码"
        description="选择文件，配置编码参数，快速转码视频"
      />
      <EncoderPanel />
    </div>
  );
}
