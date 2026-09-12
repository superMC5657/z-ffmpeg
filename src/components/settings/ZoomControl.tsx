import { useEffect, useState } from "react";
import SegmentedControl from "@/components/layout/SegmentedControl";
import { getStoredZoom, setZoomLevel, ZOOM_OPTIONS, type ZoomLevel } from "@/lib/zoom";

/** 界面缩放分段控件（90% / 100% / 110% / 125%） */
export default function ZoomControl() {
  const [zoom, setZoom] = useState<ZoomLevel>(() => getStoredZoom());

  useEffect(() => {
    // 监听 storage 事件同步多处或重新挂载
    const onStorage = (e: StorageEvent) => {
      if (e.key === "z-ffmpeg-ui-zoom") setZoom(getStoredZoom());
    };
    window.addEventListener("storage", onStorage);
    return () => window.removeEventListener("storage", onStorage);
  }, []);

  const handleZoomChange = (value: ZoomLevel) => {
    setZoom(value);
    setZoomLevel(value);
  };

  return (
    <SegmentedControl<ZoomLevel>
      value={zoom}
      onChange={handleZoomChange}
      options={ZOOM_OPTIONS.map((o) => ({ value: o.value, label: o.label }))}
    />
  );
}
