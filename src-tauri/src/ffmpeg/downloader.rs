//! FFmpeg auto-download: fetch a Windows build, extract ffmpeg.exe /
//! ffprobe.exe into {app_data_dir}/ffmpeg, verify it, and refresh the
//! cached status.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use parking_lot::Mutex;
use tauri::Emitter;

use crate::error::{AppError, AppResult};
use crate::ffmpeg::library::{self, FfmpegStatus};

/// FFmpeg 下载源列表，按优先级依次尝试：
/// 国内可达的 GitHub 加速代理（实测快 30+ 倍）优先，官方源最后兜底。
/// 若代理失效，把不可用的条目删掉或换成新的加速代理即可。
const FFMPEG_SOURCES: &[&str] = &[
    "https://gh-proxy.com/https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip",
    "https://ghfast.top/https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip",
    "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip",
];

/// Guards against concurrent download invocations (e.g. page remounts).
static DOWNLOAD_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Download & install FFmpeg into {app_data_dir}/ffmpeg, then refresh the
/// cached status. Emits `ffmpeg://download-progress` (f64, 0-100) while
/// downloading so the UI can show a progress bar.
pub async fn download_ffmpeg(
    app: &tauri::AppHandle,
    status_lock: &Mutex<FfmpegStatus>,
) -> AppResult<FfmpegStatus> {
    if !cfg!(windows) {
        return Err(AppError::Ffmpeg(
            "自动下载目前仅支持 Windows，请手动安装 FFmpeg 并加入 PATH。".into(),
        ));
    }

    // Serialize downloads so two calls can't clobber the same zip path.
    let _lock = DOWNLOAD_LOCK.lock().await;

    let install_dir = library::local_install_dir(&crate::get_data_dir(app));
    std::fs::create_dir_all(&install_dir)?;

    let zip_path = install_dir.join("ffmpeg-download.zip");
    let temp_dir = install_dir.join(".ffmpeg-extract");

    // 整个"逐源安装 → 读取状态"流程全部在 blocking 线程上执行：
    // 任何一步(含 ffmpeg 子进程验证)都不会卡住 async runtime。
    // 仅把纯内存操作(更新缓存/状态锁)留在 async 线程做。
    let app_clone = app.clone();
    let install_clone = install_dir.clone();
    let zip_dl = zip_path.clone();
    let temp_dl = temp_dir.clone();
    let result = tauri::async_runtime::spawn_blocking(
        move || -> AppResult<(PathBuf, PathBuf, FfmpegStatus)> {
            // 逐源完整安装：每个源走"下载 → 解压 → 移动 → 验证"全流程，
            // 任一步失败(含代理返回 200 错误页导致 zip 无效)都换下一个源。
            install_ffmpeg_from_sources(
                &app_clone,
                &zip_dl,
                &temp_dl,
                &install_clone,
            )?;

            let ffmpeg_final = install_clone.join("ffmpeg.exe");
            let ffprobe_final = install_clone.join("ffprobe.exe");
            let status = library::get_status_from_paths(
                ffmpeg_final.clone(),
                Some(ffprobe_final.clone()),
            )?;
            Ok((ffmpeg_final, ffprobe_final, status))
        },
    )
    .await
    .map_err(|e| AppError::Internal(format!("FFmpeg 安装任务异常终止: {e}")))?;

    // Best-effort cleanup of the zip and temp dir on every path,
    // BEFORE unpacking the result — an Err from `result?` below must
    // not skip this cleanup.
    let _ = std::fs::remove_file(&zip_path);
    let _ = std::fs::remove_dir_all(&temp_dir);

    let (ffmpeg_path, ffprobe_path, status) = result?;
    library::set_installed_paths(ffmpeg_path, Some(ffprobe_path));
    *status_lock.lock() = status.clone();

    Ok(status)
}

/// Replace `dest` with `src` (std::fs::rename fails if `dest` exists on Windows).
fn replace_file(src: &Path, dest: &Path) -> AppResult<()> {
    if dest.exists() {
        std::fs::remove_file(dest)?;
    }
    std::fs::rename(src, dest)?;
    Ok(())
}

/// 逐源完整安装：按优先级依次尝试每个源，每个源走
/// "下载 → 解压 → 移动到位 → 验证可运行"全流程，任一步失败
/// （连接失败、HTTP 错误、传输中断、代理返回 200 错误页导致 zip
/// 无效、二进制无法运行……）都视为该源失败，清理半成品后换下一个
/// 源；全部失败时返回聚合错误（附每个源的原因）。
fn install_ffmpeg_from_sources(
    app: &tauri::AppHandle,
    zip_path: &Path,
    temp_dir: &Path,
    install_dir: &Path,
) -> AppResult<()> {
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(|e| AppError::Ffmpeg(format!("创建下载客户端失败: {e}")))?;

    install_from_sources(FFMPEG_SOURCES, &|url| {
        install_from_url(app, &client, url, zip_path, temp_dir, install_dir)
    }, &|| {
        // 丢弃该源的半成品（zip/临时目录/已移动的二进制），
        // 让下一个源从零开始；已移动的二进制一并回滚，
        // 避免 install 根下残留半安装状态。
        let _ = std::fs::remove_file(zip_path);
        let _ = std::fs::remove_dir_all(temp_dir);
        let _ = std::fs::remove_file(install_dir.join("ffmpeg.exe"));
        let _ = std::fs::remove_file(install_dir.join("ffprobe.exe"));
    })
}

/// 逐源回退的核心循环,与 HTTP/文件系统解耦,便于单元测试注入假安装器。
/// `install_one` 返回 Err 即视为该源失败,调用 `cleanup_source` 清理半成品后
/// 换下一个源;全部失败时返回聚合错误(附每个源的原因)。
fn install_from_sources(
    sources: &[&str],
    install_one: &dyn Fn(&str) -> AppResult<()>,
    cleanup_source: &dyn Fn(),
) -> AppResult<()> {
    let mut errors: Vec<String> = Vec::new();
    for url in sources {
        match install_one(url) {
            Ok(()) => return Ok(()),
            Err(e) => {
                log::warn!("FFmpeg 下载源 {} 失败: {}", url, e);
                errors.push(format!("{url}: {e}"));
                cleanup_source();
            }
        }
    }

    Err(AppError::Ffmpeg(format!(
        "下载 FFmpeg 失败（所有下载源均失败），请重试，或手动下载 FFmpeg 并注册环境变量\n{}",
        errors.join("\n")
    )))
}

/// 单个源的完整安装流程：下载 → 解压 → 移动 → 验证，任一步失败即返回 Err。
fn install_from_url(
    app: &tauri::AppHandle,
    client: &reqwest::blocking::Client,
    url: &str,
    zip_path: &Path,
    temp_dir: &Path,
    install_dir: &Path,
) -> AppResult<()> {
    // 1) 下载
    download_from(client, url, zip_path, app)?;

    // 2) 解压到临时目录
    std::fs::create_dir_all(temp_dir)?;
    let (ffmpeg_tmp, ffprobe_tmp) = extract_binaries(zip_path, temp_dir)?;

    // 3) Move into place (same volume; Windows rename doesn't overwrite).
    //    任一文件移动失败即整体失败,并回滚已移动的文件,
    //    避免 install 根下残留半安装状态。
    let ffmpeg_final = install_dir.join("ffmpeg.exe");
    let ffprobe_final = install_dir.join("ffprobe.exe");
    if let Err(e) = replace_file(&ffmpeg_tmp, &ffmpeg_final) {
        let _ = std::fs::remove_file(&ffmpeg_final);
        return Err(e.into());
    }
    if let Err(e) = replace_file(&ffprobe_tmp, &ffprobe_final) {
        let _ = std::fs::remove_file(&ffmpeg_final);
        let _ = std::fs::remove_file(&ffprobe_final);
        return Err(e.into());
    }

    // 4) 验证最终二进制可运行;失败则报错,由回退循环清理并换下一个源,
    //    绝不向 UI 报告 "installed"。
    verify_binary(&ffmpeg_final)?;
    Ok(())
}

/// Strictly verify that an ffmpeg binary starts and prints a version line.
/// Unlike `library::get_status_from_paths` (which swallows errors via
/// `.ok()`), this fails the whole install when the binary cannot run —
/// e.g. a proxy-served stub or a blocked executable.
fn verify_binary(ffmpeg_path: &Path) -> AppResult<()> {
    let output = crate::ffmpeg::hidden_command(ffmpeg_path)
        .arg("-version")
        .output()
        .map_err(|e| AppError::Ffmpeg(format!("无法启动 ffmpeg 进程: {e}")))?;
    if !output.status.success() {
        return Err(AppError::Ffmpeg(format!(
            "ffmpeg -version 异常退出 (code {:?})",
            output.status.code()
        )));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.trim().is_empty() {
        return Err(AppError::Ffmpeg("ffmpeg -version 无任何输出".into()));
    }
    Ok(())
}

/// Download a single source into `dest`, emitting progress events.
fn download_from(
    client: &reqwest::blocking::Client,
    url: &str,
    dest: &Path,
    app: &tauri::AppHandle,
) -> AppResult<()> {
    let response = client
        .get(url)
        .send()
        .map_err(|e| AppError::Ffmpeg(format!("连接失败: {e}")))?;

    if !response.status().is_success() {
        return Err(AppError::Ffmpeg(format!(
            "HTTP {}",
            response.status()
        )));
    }

    let total = response.content_length().unwrap_or(0);
    let mut reader = response;
    let mut emit = |pct: f64| {
        let _ = app.emit("ffmpeg://download-progress", pct);
    };
    write_response(&mut reader, total, dest, &mut emit)?;
    Ok(())
}

/// 把 reader 内容写入 dest;已知 total 时按读取字节数回调进度(0-100)。
/// 纯 IO 逻辑、不依赖网络,便于单元测试覆盖写盘与进度计算。
fn write_response(
    reader: &mut dyn Read,
    total: u64,
    dest: &Path,
    emit: &mut dyn FnMut(f64),
) -> AppResult<u64> {
    let mut file = std::fs::File::create(dest)?;
    let mut buf = [0u8; 64 * 1024];
    let mut done: u64 = 0;

    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| AppError::Ffmpeg(format!("读取下载数据失败: {e}")))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        done += n as u64;
        if total > 0 {
            let pct = (done as f64 / total as f64 * 100.0).min(100.0);
            emit(pct);
        }
    }

    if total > 0 {
        emit(100.0);
    }
    Ok(done)
}

/// Extract ffmpeg.exe / ffprobe.exe from the zip into `install_dir`.
/// Only file names are used for matching; output paths are always
/// constructed by us, so zip entries cannot escape the install dir.
fn extract_binaries(zip_path: &Path, install_dir: &Path) -> AppResult<(PathBuf, PathBuf)> {
    let file = std::fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| AppError::Ffmpeg(format!("读取 FFmpeg 压缩包失败: {e}")))?;

    let mut ffmpeg: Option<PathBuf> = None;
    let mut ffprobe: Option<PathBuf> = None;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| AppError::Ffmpeg(format!("读取压缩包条目失败: {e}")))?;
        if entry.is_dir() {
            continue;
        }
        let file_name = Path::new(entry.name())
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();

        let out_name = match file_name.as_str() {
            "ffmpeg.exe" | "ffmpeg" => "ffmpeg.exe",
            "ffprobe.exe" | "ffprobe" => "ffprobe.exe",
            _ => continue,
        };

        let out_path = install_dir.join(out_name);
        let mut out = std::fs::File::create(&out_path)?;
        std::io::copy(&mut entry, &mut out)?;

        if out_name == "ffmpeg.exe" {
            ffmpeg = Some(out_path);
        } else {
            ffprobe = Some(out_path);
        }
    }

    let ffmpeg = ffmpeg
        .ok_or_else(|| AppError::Ffmpeg("FFmpeg 压缩包中未找到 ffmpeg 可执行文件".into()))?;
    let ffprobe = ffprobe
        .ok_or_else(|| AppError::Ffmpeg("FFmpeg 压缩包中未找到 ffprobe 可执行文件".into()))?;

    Ok((ffmpeg, ffprobe))
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use zip::write::SimpleFileOptions;

    /// 用 Stored(不压缩)方式构造 zip,不依赖任何压缩 feature。
    fn make_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let file = std::fs::File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, data) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(data).unwrap();
        }
        writer.finish().unwrap();
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "z-ffmpeg-test-{}-{}",
            tag,
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn read_bytes(path: &Path) -> Vec<u8> {
        std::fs::read(path).unwrap()
    }

    // ---- extract_binaries ----

    #[test]
    fn extract_binaries_extracts_ffmpeg_and_ffprobe() {
        let dir = temp_dir("extract-ok");
        let zip_path = dir.join("ffmpeg.zip");
        let out_dir = dir.join("out");
        std::fs::create_dir_all(&out_dir).unwrap();

        make_zip(
            &zip_path,
            &[
                ("ffmpeg-master-latest-win64-gpl/bin/ffmpeg.exe", b"FFMPEGBIN"),
                ("ffmpeg-master-latest-win64-gpl/bin/ffprobe.exe", b"FFPROBEBIN"),
                ("ffmpeg-master-latest-win64-gpl/README.txt", b"ignore me"),
            ],
        );

        let (ffmpeg, ffprobe) = extract_binaries(&zip_path, &out_dir).unwrap();
        assert_eq!(ffmpeg, out_dir.join("ffmpeg.exe"));
        assert_eq!(ffprobe, out_dir.join("ffprobe.exe"));
        assert_eq!(read_bytes(&ffmpeg), b"FFMPEGBIN");
        assert_eq!(read_bytes(&ffprobe), b"FFPROBEBIN");
        // 无关文件不被提取
        assert!(!out_dir.join("README.txt").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn extract_binaries_fails_when_ffprobe_missing() {
        let dir = temp_dir("extract-noprobe");
        let zip_path = dir.join("ffmpeg.zip");
        let out_dir = dir.join("out");
        std::fs::create_dir_all(&out_dir).unwrap();

        make_zip(&zip_path, &[("bin/ffmpeg.exe", b"FFMPEGBIN")]);

        let err = extract_binaries(&zip_path, &out_dir).unwrap_err();
        assert!(err.to_string().contains("ffprobe"), "got: {err}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn extract_binaries_fails_on_corrupt_zip() {
        let dir = temp_dir("extract-corrupt");
        let zip_path = dir.join("ffmpeg.zip");
        let out_dir = dir.join("out");
        std::fs::create_dir_all(&out_dir).unwrap();
        std::fs::write(&zip_path, b"this is not a zip file at all").unwrap();

        let err = extract_binaries(&zip_path, &out_dir).unwrap_err();
        assert!(err.to_string().contains("压缩包"), "got: {err}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn extract_binaries_ignores_path_traversal_entries() {
        let dir = temp_dir("extract-traversal");
        let zip_path = dir.join("ffmpeg.zip");
        // 恶意条目名带 ../,只取 basename,输出始终落在 out_dir 内
        let out_dir = dir.join("out");
        std::fs::create_dir_all(&out_dir).unwrap();

        make_zip(
            &zip_path,
            &[
                ("../../evil/ffmpeg.exe", b"EVIL"),
                ("../../evil/ffprobe.exe", b"PROBE"),
            ],
        );

        let (ffmpeg, _) = extract_binaries(&zip_path, &out_dir).unwrap();
        assert_eq!(ffmpeg, out_dir.join("ffmpeg.exe"));
        assert_eq!(read_bytes(&ffmpeg), b"EVIL");
        // 没有文件逃出 out_dir
        assert!(!dir.join("evil").exists());
        assert!(!dir.join("ffmpeg.exe").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- replace_file ----

    #[test]
    fn replace_file_overwrites_existing_dest() {
        let dir = temp_dir("replace-overwrite");
        let src = dir.join("src.exe");
        let dest = dir.join("dest.exe");
        std::fs::write(&src, b"new").unwrap();
        std::fs::write(&dest, b"old").unwrap();

        replace_file(&src, &dest).unwrap();
        assert_eq!(read_bytes(&dest), b"new");
        assert!(!src.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn replace_file_fails_when_source_missing() {
        let dir = temp_dir("replace-missing");
        let src = dir.join("nope.exe");
        let dest = dir.join("dest.exe");

        assert!(replace_file(&src, &dest).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- verify_binary ----

    #[test]
    fn verify_binary_fails_on_missing_path() {
        let dir = temp_dir("verify-missing");
        let err = verify_binary(&dir.join("no-such-ffmpeg.exe")).unwrap_err();
        assert!(err.to_string().contains("无法启动"), "got: {err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- write_response ----

    #[test]
    fn write_response_writes_bytes_and_emits_progress() {
        let dir = temp_dir("write-response");
        let dest = dir.join("out.bin");
        let data = b"0123456789abcdef";
        let mut reader = Cursor::new(data.to_vec());
        let mut progress: Vec<f64> = Vec::new();

        let written = write_response(
            &mut reader,
            data.len() as u64,
            &dest,
            &mut |pct| progress.push(pct),
        )
        .unwrap();

        assert_eq!(written, data.len() as u64);
        assert_eq!(read_bytes(&dest), data);
        // total 已知:最后一个进度必须是 100
        assert_eq!(*progress.last().unwrap(), 100.0);
        assert!(progress.windows(2).all(|w| w[0] <= w[1]));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_response_without_total_skips_progress() {
        let dir = temp_dir("write-no-total");
        let dest = dir.join("out.bin");
        let mut reader = Cursor::new(b"hello".to_vec());
        let mut progress: Vec<f64> = Vec::new();

        write_response(&mut reader, 0, &dest, &mut |pct| progress.push(pct)).unwrap();
        assert_eq!(read_bytes(&dest), b"hello");
        assert!(progress.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- install_from_sources (逐源回退) ----

    #[test]
    fn sources_fall_back_to_next_on_failure() {
        let dir = temp_dir("sources-fallback");
        let dest = dir.join("ffmpeg-download.zip");
        let calls: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

        let result = install_from_sources(
            &["http://first", "http://second"],
            &|url| {
                calls.lock().unwrap().push(url.to_string());
                if url == "http://first" {
                    Err(AppError::Ffmpeg("压缩包无效".into()))
                } else {
                    std::fs::write(&dest, b"good").unwrap();
                    Ok(())
                }
            },
            &|| {
                let _ = std::fs::remove_file(&dest);
            },
        );

        assert!(result.is_ok());
        // 按优先级顺序尝试,成功后停止
        assert_eq!(
            *calls.lock().unwrap(),
            vec!["http://first".to_string(), "http://second".to_string()]
        );
        assert_eq!(read_bytes(&dest), b"good");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sources_fail_aggregates_all_errors() {
        let dir = temp_dir("sources-allfail");
        let dest = dir.join("ffmpeg-download.zip");

        let err = install_from_sources(
            &["http://a", "http://b"],
            &|_| Err(AppError::Ffmpeg("网络错误".into())),
            &|| {},
        )
        .unwrap_err();

        let msg = err.to_string();
        assert!(msg.contains("http://a"), "got: {msg}");
        assert!(msg.contains("http://b"), "got: {msg}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sources_clean_partial_file_between_sources() {
        let dir = temp_dir("sources-cleanup");
        let dest = dir.join("ffmpeg-download.zip");

        // 第一个源写了一半就失败;回退循环必须清掉半截文件,
        // 第二个源从干净状态重新写
        install_from_sources(
            &["http://a", "http://b"],
            &|url| {
                if url == "http://a" {
                    std::fs::write(&dest, b"partial").unwrap();
                    Err(AppError::Ffmpeg("传输中断".into()))
                } else {
                    std::fs::write(&dest, b"complete").unwrap();
                    Ok(())
                }
            },
            &|| {
                let _ = std::fs::remove_file(&dest);
            },
        )
        .unwrap();

        assert_eq!(read_bytes(&dest), b"complete");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sources_retry_always_starts_from_highest_priority() {
        let dir = temp_dir("sources-retry");
        let dest = dir.join("ffmpeg-download.zip");
        let attempts: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

        // 每次调用 install_from_sources 都从第一个源开始尝试
        for _ in 0..2 {
            let result = install_from_sources(
                &["http://first", "http://second"],
                &|url| {
                    attempts.lock().unwrap().push(url.to_string());
                    if url == "http://first" {
                        Err(AppError::Ffmpeg("失败".into()))
                    } else {
                        std::fs::write(&dest, b"ok").unwrap();
                        Ok(())
                    }
                },
                &|| {},
            );
            assert!(result.is_ok());
        }

        let all = attempts.lock().unwrap();
        // 两次重试都以 highest-priority 源开头
        assert_eq!(all[0], "http://first");
        assert_eq!(all[2], "http://first");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
