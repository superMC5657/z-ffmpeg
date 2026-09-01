# Code Review — z-ffmpeg 0.0.1 (`68a1d43..57191e0`)

Reviewed the full "0.0.1" rewrite (backend `encoder`, `queue`, `preset`, `commands`; frontend `stores`, `routes`, `components`). Verified `cargo check`, `tsc --noEmit`, and `pnpm build` all pass.

## Findings

### [P2] Deduplicate output paths when a custom output directory is used — `src-tauri/src/encoder/engine.rs:256`

`derive_output_path` always produces `{stem}_encoded.{ext}` inside the chosen output dir. Adding two inputs with the same basename from different folders (or the same file twice) yields the same output path for both jobs, and the `-y` flag makes ffmpeg silently overwrite the first result. This affects both `add_to_queue` and `build_ffmpeg_commands`, which share this function.

### [P2] Make the Presets page "选择" actually apply the preset — `src/components/preset/PresetCard.tsx:107`

The select button only calls `selectPreset`, which sets `selectedPresetId` in the preset store. Nothing applies that config to the encoder store, so after selecting a preset on the Presets page, the Encoder page's dropdown shows it as selected while the form fields (codec, rate control, container, etc.) still hold the old values; "添加到队列" then uses stale settings.

### [P2] Stop the queue page's "清除已完成" from deleting History records — `src-tauri/src/queue/manager.rs:157`

`clear_completed` deletes Completed/Failed/Cancelled rows from SQLite, and `history()` reads from that same table. Because `dequeue_next` now keeps finished jobs in the in-memory queue (the rewrite changed `remove` to in-place status update), the queue page displays them and the clear button is newly active; clicking it silently wipes the records shown on the separate History page, whose own "清空历史" button implies they should be independent.

### [P3] Make cancel reliable for jobs that are starting — `src-tauri/src/queue/manager.rs:198`

`cancel_job` marks the job Cancelled even when `engine::cancel_process` returns false (no ffmpeg child registered yet — the child is only inserted into `PROCESSES` inside `start_encode` after spawn). In that window the encode keeps running to completion in the background and writes the output file while the UI shows "已取消".

### [P3] Wire up or remove the no-op Retry button on failed queue items — `src/components/queue/QueueItem.tsx:54`

The `RotateCw` button rendered for `Failed` jobs has no `onClick` handler, so clicking it does nothing despite looking interactive.

### [P3] Restore faststart for MP4 outputs if the removal was unintentional — `src-tauri/src/encoder/engine.rs:131`

The rewrite dropped the unconditional `-movflags +faststart` the previous version applied; MP4 outputs are no longer faststart-optimized. If the drop was deliberate, consider applying it conditionally for `MP4`/`MOV` containers only.

### [P3] Align the frontend `EncodeProgress` type with the backend payload — `src/types/index.ts:92`

The type declares `totalFrames`, `etaSeconds`, `timeElapsed`, `outputSizeBytes`, but the backend emits `totalSizeKb`, `elapsed` (string), and `time` (string) from `src-tauri/src/encoder/progress.rs:10`. Fields the UI reads (`percentage`, `fps`, `speed`, `bitrate`, `fileName`) exist on both sides, so nothing breaks today, but the drift will silently produce `undefined` if new UI code uses the declared fields.

## Overall assessment

The core rewrite is solid: cancellation is now process-level (registry + kill), progress parsing moved to ffmpeg's machine-readable `-progress`, history is DB-backed and survives restarts, and the added Rust unit tests cover queue persistence, preset round-trips, and HW-platform gating.

**Test gaps**

- No tests for the new engine progress/cancel logic or `derive_output_path` (collision case).
- No frontend tests.

**Residual risks**

- `process_queue`'s check-then-act loop (`can_start` → `dequeue_next` → `inc_active`) can over-spawn when invoked concurrently (pre-existing, but now triggered by a user button).
- Failed queue jobs always surface the misleading "Output file not created" error because `start_encode`'s result and stderr diagnostics are discarded.
