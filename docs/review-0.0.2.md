# Code Review — z-ffmpeg 0.0.2 (`57191e0..6a76b26`)

Reviewed the 0.0.2 commit. The commit correctly addresses the previously documented gaps (output-path dedup, preset application, history retention, retry button, faststart, progress-type alignment) and the added Rust tests are sound, but the new cancellation mechanism leaves a real window where a cancelled job's ffmpeg child is never killed and still writes output, and the preset application regressed to unguarded field reads for malformed imported presets.

## Findings

### [P2] Cancel during probe/spawn window still leaves ffmpeg running — `src-tauri/src/encoder/engine.rs:462-481`

The new pre-spawn cancellation check only runs once, at the very top of `start_encode` (before `probe_file`), and the child is only registered in `PROCESSES` after spawn. If the user cancels after that check but before the child is registered (the probe itself can take hundreds of ms), `cancel_job` (manager.rs) sets the flag but `cancel_process` returns false because no child is registered yet, so the ffmpeg child is never killed. The stdout read loop then breaks on the first progress block, closing the pipe; ffmpeg either dies on the next EPIPE (leaving a partial output file) or, for short encodes, runs to completion and writes the full output file — all while the UI shows the job as Cancelled and the worker's `wait()` blocks a concurrency slot. A job cancelled right after clicking start can therefore still produce an output file and waste CPU. Re-check `cancel.load()` immediately after inserting the child into `PROCESSES` (and kill it if set) to close this window; the same class of race exists between `dequeue_next` and `cancel_flags.insert` in `process_queue`.

### [P3] applyConfig overwrites form state with undefined for malformed presets — `src/store/encoderStore.ts:157-172`

`applyConfig` reads `vs.rateControl`, `as_.codec` and `as_.bitrateKbps` unconditionally, whereas the previous `PresetSelector` code guarded each field (`if (rc) setRateControl(rc)`, `if (as_?.codec) ...`). The backend `import_preset` accepts any JSON object as a preset config without schema validation, so a preset missing `videoSettings.rateControl` (or with a partial `audioSettings`) will set the encoder store fields to `undefined` — or throw a TypeError if `videoSettings` itself is absent — and the next `buildConfig()`/`addToQueue` will fail serde deserialization on the Rust side. Consider preserving the old guards (`vs?.rateControl` etc.) or validating the preset schema before applying.

## Overall assessment

The previously documented v0.0.1 issues are all resolved: output paths are deduplicated within a batch (`derive_output_paths_unique` with tests), MP4/MOV outputs get `-movflags +faststart` again (container-conditional), the queue page's "清除已完成" no longer deletes History records, failed queue items have a working retry action, preset selection now applies through `applyConfig`, and the `EncodeProgress` type was aligned with the backend payload. The new Rust unit tests cover the dedup and faststart behavior.

The two remaining issues above are the residual risks to address next: the cancel/probe-spawn race (P2) and the unguarded `applyConfig` reads for malformed imported presets (P3).
