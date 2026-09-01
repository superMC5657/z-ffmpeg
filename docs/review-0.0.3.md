# Code Review — z-ffmpeg 0.0.3 (`e65aa50..6f2a2ee`)

Reviewed the FFmpeg auto-download feature commit (mirror fallback chain, zip extraction, install into `{data_dir}/z-ffmpeg/ffmpeg`, status refresh, and Settings page UI wiring). The refactor and UI wiring are sound and the code compiles against the locked dependency versions, but the new download feature has real defects in its failure handling: mirror fallback is skipped when extraction fails, and an unrunnable downloaded binary is reported as installed. These are non-blocking for existing functionality but should be fixed before the feature is relied upon.

## Findings

### [P2] Mirror fallback never triggers on extraction/verification failure — `src-tauri/src/ffmpeg/downloader.rs:63-70`

The per-source fallback in `download_zip` only reacts to HTTP/download errors. Once any source returns HTTP 200, `download_from` writes the body to `ffmpeg-download.zip` and `download_zip` returns `Ok`, then `extract_binaries` runs outside the retry loop. If a proxy (the first two sources are GitHub accelerators) returns a 200 error/HTML page or a truncated body that survives as a non-zip file, extraction fails and the whole command aborts — the remaining mirrors, including the gyan.dev fallback, are never tried. Since the source list is explicitly designed as a fallback chain, extraction failure should be treated as a source failure and retried against the next URL (or the zip should be validated before extraction).

### [P2] Download reports "installed" without verifying the binary runs — `src-tauri/src/ffmpeg/downloader.rs:86-90`

After moving the extracted binaries into place, `download_ffmpeg` calls `get_status_from_paths`, which unconditionally sets `available: true` and swallows failures from `ffmpeg -version`/`-encoders` (`.ok()` / `unwrap_or_default()` in `library.rs:102-103`). The command then returns `status: "installed"` and emits `ffmpeg://ready`, so the UI enables encoding and the footer shows "FFmpeg 已就绪" even when the downloaded `ffmpeg.exe` cannot execute (e.g., a proxy-served stub or a blocked binary). The new feature should treat a failed version check as a download failure and surface an error (or fall back to the next mirror) instead of advertising a broken install.

### [P3] Blocking ffmpeg verification runs on the async command thread — `src-tauri/src/ffmpeg/downloader.rs:86-88`

`download_ffmpeg` (an async Tauri command) executes `get_status_from_paths` directly on the async runtime thread at the end of the download. That function synchronously spawns `ffmpeg -version` and `ffmpeg -encoders` subprocesses (`library.rs:122-125`, `library.rs:134-137`), blocking the runtime worker — the same blocking pattern this commit explicitly moved to `spawn_blocking` in `detect_hw_accel` (the comment there says "避免卡住 async runtime"). Wrap the verification call in `tauri::async_runtime::spawn_blocking` as well, so a slow/hung ffmpeg cannot stall all Tauri commands.

## Overall assessment

The commit is a solid foundation for the feature: downloads are serialized by a global lock, extraction happens into a temp dir before binaries are moved into place (so a failed download/extract never leaves a half-installed ffmpeg.exe that would look "installed" on the next app start), progress events drive the Settings page progress bar, the zip/temp dir are cleaned up on every path, and the mirror list is ordered for China-accessible GitHub accelerators with the official source as the final fallback.

The three issues above are the residual risks to address before the feature is relied upon: treat extraction failure as a source failure so the mirror chain actually falls through (P2), fail the download when the installed binary cannot run instead of advertising a broken install (P2), and move the blocking verification off the async runtime thread (P3).
