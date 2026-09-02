# 代码审查 — z-ffmpeg 0.1.0（`cd91b32..5f2561e`）

审查了编码页输出体积预览提交（「编码选择页面显示预测视频大小」）。核心的预览预估功能是可靠的：队列与预览两条路径共用同一套推算核心，ABR 的 serde 重命名修复了真实存在的前后端字段不匹配，全部 Rust 单元测试通过，`tsc --noEmit` 也通过。以下两个发现属于明确标注为「仅供参考」的启发式算法中的边界精度问题，不阻塞本次改动。

## 发现的问题

### [P3] 音频码率回退误用容器总码率，导致 Copy 预估虚高 — `src-tauri/src/encoder/engine.rs:457-473`

在 `parse_probe_result` 中，当音频流存在但 ffprobe 省略其 `bit_rate` 时，新增的 `audio_bitrate` 回退逻辑会用**容器总码率**（`size * 8 / duration`）代替，而容器总码率包含视频码率（例如实际总码率 8 Mbps，音频却只有约 192 kbps）。只要用户选择音频 `Copy`，这个值就会进入 `estimate_output_bytes_from_info`，导致新的编码页预览对这类文件把输出体积高估约 3–50 倍（部分 MKV/TS/OGG 以及 AVI 中的 PCM 音轨不报告音频流 `bit_rate`）。`audio_stream_kbps`（`src-tauri/src/encoder/estimate.rs:203`）中对应的启发式逻辑早于本提交，但预览界面是新的；减去视频流码率或对回退值设上限，可以避免把整个容器当成音频。

### [P3] 缩放因子取第一个视频流，可能是内嵌封面图 — `src-tauri/src/encoder/estimate.rs:48-54`

`estimate_output_bytes` 为新增的 CRF/CQP 缩放从 `codec_type == "video"` 的第一个流中取输入分辨率/帧率。对于带内嵌封面（`disposition.attached_pic=1` 的视频流，ffprobe 常将其列在首位）的 MKV 文件，这里取到的是封面的尺寸而不是主视频。当用户设置输出分辨率时，`output_scale_factor` 会拿极小的封面面积计算 `out_area / in_area` 并 clamp 到 4.0，把预估最多放大 4 倍；预览路径同样继承了 `parse_probe_result` 中「取第一个视频流」的错误尺寸（`src-tauri/src/encoder/engine.rs:411-413`）。在这两处跳过 `attached_pic` 流（或选取面积最大的视频流），即可让缩放始终作用于真正的视频。

## 总体评价

该提交为编码页预览打下了可靠基础：队列（Pending）与预览两条路径共用同一个 `estimate_bytes` 核心，ABR 的 serde 重命名让 `RateControl::Abr` 与前端实际发送的 camelCase 载荷对齐，且基于 probe（`estimate_output_bytes`）与基于 `FileInfo`（`estimate_output_bytes_from_info`）的两条预估路径在信息缺失时都会优雅降级为「不展示预估」。上述两个问题都是明确标注为「仅供参考」的启发式算法中遗留的边界风险：音频回退在容器省略流级 `bit_rate` 时会把 Copy 音频高估约 3–50 倍，缩放因子在带封面的 MKV 上可能把封面当作主视频（最多放大 4 倍）。两者都不阻塞本次改动，但给音频回退减去/设上限、并跳过 `attached_pic` 流，会让这些文件上的预估更可信。
