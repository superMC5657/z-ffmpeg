# Code Review — z-ffmpeg 0.0.4 (`6f2a2ee..b2fe0f9`)

Reviewed the VMAF scoring and output-size estimation commit (per-job VMAF compute with full/sampled modes persisted to DB, size estimate computed at enqueue time, and queue/History UI wiring). The patch compiles, all new Rust tests pass, the frontend type-checks, and no existing behavior is broken; the two findings below are non-blocking functional gaps in the new features rather than regressions or blockers.

## Findings

### [P2] VMAF compute only resolves jobs still in the in-memory queue — `src-tauri/src/commands/vmaf.rs:30-32`

`compute_vmaf` resolves input/output paths through `QueueManager::get_job_paths`, which searches only the in-memory `jobs` deque (`src-tauri/src/queue/manager.rs:363-369`). Completed jobs are loaded into memory only for the current session: `load_jobs` restores only Pending/Encoding/Paused rows (`manager.rs:118-125`), and `clear_completed` / removing a finished job drops it from memory while keeping the DB row. In those cases `compute_vmaf` fails with "任务不存在" even though the input/output paths are retained in the `jobs` table, and the History page (which reads the DB and displays VMAF) offers no way to compute a score — so past encodes can never be scored after a restart. Falling back to `history()` rows in `get_job_paths` (or resolving paths from the DB) would close the gap. Note that `set_vmaf_score` (`manager.rs:372-378`) has the same in-memory-only limitation, so the fix must also persist the score for DB-only jobs (e.g., an UPDATE by id), otherwise the computed result would be silently dropped.

### [P3] ABR size estimate ignores `VideoCodec::Copy`, unlike CRF/CQP branches — `src-tauri/src/encoder/estimate.rs:44`

In `estimate_output_bytes`, the `RateControl::Abr` branch always uses the configured `bitrate_kbps` as the video bitrate, while the CRF/CQP branches explicitly short-circuit to the input-bitrate estimate when `video_codec` is `VideoCodec::Copy` (`estimate.rs:47-49`, `estimate.rs:61-63`). A Copy video stream ignores the ABR bitrate, so for any config/preset with Copy + ABR the Pending estimate can be off by a large factor (the output stays near input size). The fix is to mirror the Copy check already present in the other branches.

## Overall assessment

The commit is a solid foundation for both features: each VMAF run uses a unique work directory (`job_id` + UUID) so re-runs and concurrent computations never collide, the segment count is clamped to 1..=32 with 0 meaning full-length comparison, results are persisted to the DB and surfaced via `queue://updated`, and the size estimate runs in parallel at enqueue time with probe failures degrading gracefully to "no estimate" instead of blocking the queue.

The two issues above are the residual risks to address next: the VMAF path resolution (and score persistence) must fall back to the DB so finished jobs remain scoreable across restarts and after `clear_completed` (P2), and the ABR branch needs the same Copy short-circuit the other rate-control branches already have (P3).
