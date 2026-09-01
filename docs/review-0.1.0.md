# Code Review — z-ffmpeg 0.1.0 (`cd91b32..5f2561e`)

Reviewed the encode-page output-size preview commit ("编码选择页面显示预测视频大小"). The core preview-estimate feature is sound: the queue and preview paths share the same computation core, the ABR serde rename fixes a real frontend/backend mismatch, all Rust unit tests pass, and `tsc --noEmit` succeeds. The two findings below are edge-case accuracy issues in a heuristic explicitly labeled "仅供参考" and do not block the change.

## Findings

### [P3] Audio-bitrate fallback uses container total bitrate, inflating Copy estimates — `src-tauri/src/encoder/engine.rs:457-473`

In `parse_probe_result`, when the audio stream exists but ffprobe omits its `bit_rate`, the new `audio_bitrate` fallback substitutes the **container total** bitrate (`size * 8 / duration`), which includes the video bitrate (e.g. 8 Mbps total instead of ~192 kbps audio). This value feeds `estimate_output_bytes_from_info` whenever the user selects audio `Copy`, so the new encode-page preview can overstate the output by roughly 3–50× for such files (some MKV/TS/OGG and PCM-in-AVI sources don't report audio stream `bit_rate`). The mirrored heuristic in `audio_stream_kbps` (`src-tauri/src/encoder/estimate.rs:203`) predates this commit, but the preview surface is new; subtracting the video stream bitrate or capping the fallback would avoid treating the whole container as audio.

### [P3] Scale factor uses first video stream, which may be attached cover art — `src-tauri/src/encoder/estimate.rs:48-54`

`estimate_output_bytes` derives the input resolution/fps for the new CRF/CQP scale from the first stream with `codec_type == "video"`. For MKV files with an attached cover picture (a video stream with `disposition.attached_pic=1`, often listed first by ffprobe), this picks the cover image's dimensions instead of the main video. When the user sets an output resolution, `output_scale_factor` then computes `out_area / in_area` against the tiny cover area and clamps it to 4.0, inflating the estimate by up to 4×; the preview path inherits the same wrong dimensions from `parse_probe_result`'s first-video-stream lookup (`src-tauri/src/encoder/engine.rs:411-413`). Skipping streams with `attached_pic` (or selecting the largest-dimension video stream) in both lookups would keep the scale on the real video.

## Overall assessment

The commit is a solid foundation for the encode-page preview: the queue (Pending) and preview paths share the same `estimate_bytes` core, the ABR serde rename aligns `RateControl::Abr` with the camelCase payload the frontend actually sends, and both probe-driven (`estimate_output_bytes`) and `FileInfo`-driven (`estimate_output_bytes_from_info`) estimation degrade gracefully to "no estimate" when information is missing. The two issues above are residual edge-case risks in a heuristic explicitly labeled "仅供参考": the audio fallback can overstate Copy audio by roughly 3–50× on containers that omit stream `bit_rate`, and the scale factor can read attached cover art as the main video (up to 4× inflation) on MKV files with covers. Neither blocks the change, but subtracting or capping the audio fallback and skipping `attached_pic` streams would make the estimate trustworthy for those files.
