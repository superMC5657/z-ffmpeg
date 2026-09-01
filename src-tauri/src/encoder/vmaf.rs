//! VMAF 质量评估：对压缩后的视频相对原始视频打分。
//!
//! 全片逐帧 VMAF 代价过高，这里采用「均匀时间抽样」策略：
//! 从整个视频时长内均匀取出 N 段（默认 4 段 × 5 秒）分别计算 VMAF，
//! 取各段平均作为整体得分。短于 20 秒的视频自动缩短段长以避免重叠。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::encoder::engine;
use crate::error::{AppError, AppResult};
use crate::ffmpeg;

/// 默认采样段数与每段时长（秒）
pub const DEFAULT_SEGMENTS: usize = 4;
pub const DEFAULT_SEGMENT_SECONDS: f64 = 5.0;

/// 一段 VMAF 采样
#[derive(Debug, Clone)]
pub struct VmafSegment {
    /// 起点（相对视频开头，秒）
    pub start: f64,
    /// 时长（秒）
    pub length: f64,
}

/// 一次 VMAF 计算的结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VmafResult {
    pub average_score: f64,
    pub segment_scores: Vec<f64>,
    pub segment_count: usize,
}

/// 按视频时长规划采样段：起点均匀分布在时间轴上，段长在时长不足时缩短，
/// 保证各段互不重叠且不超出视频结尾。时长过短（不足两帧可采样）返回空。
pub fn plan_segments(duration: f64, count: usize, max_len: f64) -> Vec<VmafSegment> {
    if duration <= 0.0 || count == 0 || duration < 0.5 {
        return vec![];
    }
    let n = count as f64;
    // 段长取 min(max_len, duration/2n)：最后一段终点 = duration*(n-0.5)/n < duration，
    // 保证不越界。不加额外下限（下限会破坏该保证）。
    let len = max_len.min(duration / (2.0 * n));
    (0..count)
        .map(|i| {
            let start = duration * (i as f64) / n;
            VmafSegment { start, length: len }
        })
        .collect()
}

/// 检测当前 ffmpeg 是否编译了 libvmaf 滤镜（`ffmpeg -filters`）。
pub fn ffmpeg_supports_vmaf() -> bool {
    let Ok(ffmpeg_path) = ffmpeg::get_ffmpeg_path().ok_or(AppError::FfmpegNotFound) else {
        return false;
    };
    let Ok(output) = ffmpeg::hidden_command(ffmpeg_path)
        .args(["-hide_banner", "-filters"])
        .output()
    else {
        return false;
    };
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines().any(|l| l.contains("libvmaf"))
}

/// 视频信息（VMAF 计算所需的最小集）
struct VideoInfo {
    duration: f64,
    width: u32,
    height: u32,
    fps: f64,
}

/// 用 ffprobe 读取视频基础信息
fn probe_video_info(path: &str) -> AppResult<VideoInfo> {
    let json = engine::probe_file(path)?;
    let format = json
        .get("format")
        .ok_or_else(|| AppError::Ffmpeg("No format info".into()))?;
    let duration = format
        .get("duration")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);

    let video = json
        .get("streams")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.iter().find(|s| s.get("codec_type").and_then(|t| t.as_str()) == Some("video")));

    let (width, height, fps) = match video {
        Some(v) => (
            v.get("width").and_then(|w| w.as_u64()).unwrap_or(0) as u32,
            v.get("height").and_then(|h| h.as_u64()).unwrap_or(0) as u32,
            parse_fraction(v.get("r_frame_rate").and_then(|r| r.as_str()).unwrap_or("")),
        ),
        None => (0, 0, 0.0),
    };

    if duration <= 0.0 {
        return Err(AppError::Ffmpeg(format!(
            "Cannot read duration of {} (ffprobe returned 0)",
            path
        )));
    }
    Ok(VideoInfo { duration, width, height, fps })
}

/// 解析 "30000/1001" 形式的帧率
fn parse_fraction(s: &str) -> f64 {
    let mut it = s.split('/');
    let (Some(num), Some(den)) = (it.next(), it.next()) else {
        return 0.0;
    };
    let (Ok(num), Ok(den)) = (num.trim().parse::<f64>(), den.trim().parse::<f64>()) else {
        return 0.0;
    };
    if den <= 0.0 {
        0.0
    } else {
        num / den
    }
}

/// 构造单段 VMAF 计算的 ffmpeg 参数。
///
/// 两路输入先 fps/scale/format 对齐到参考视频的规格，再交给 libvmaf。
/// 注意：输入侧 `-ss` 是关键帧 seek，两文件 GOP 结构不同时可能错位少量帧，
/// 对 VMAF 略有影响，作为采样取舍接受（输出侧 seek 更精确但显著更慢）。
/// `seg` 为 `None` 表示全量对比（不截段，整片一次打分）。
/// `log_path` 只传纯文件名，工作目录由调用方设为唯一的临时目录（`current_dir`），
/// 避免 Windows 盘符冒号/空格等被 lavfi 选项解析器误判。
/// `n_threads`：libvmaf 的线程数。实测 ffmpeg 构建里 `n_threads=0`（auto）
/// 不生效会退化为单线程（5 倍速度差），必须显式传值。
fn build_segment_args(
    reference: &str,
    distorted: &str,
    seg: Option<&VmafSegment>,
    info: &VideoInfo,
    log_file_name: &str,
    n_threads: usize,
) -> Vec<String> {
    let mut args = vec!["-y".to_string()];
    if let Some(seg) = seg {
        args.extend(["-ss".into(), format!("{:.3}", seg.start), "-t".into(), format!("{:.3}", seg.length)]);
    }
    args.push("-i".into());
    args.push(reference.into());
    if let Some(seg) = seg {
        args.extend(["-ss".into(), format!("{:.3}", seg.start), "-t".into(), format!("{:.3}", seg.length)]);
    }
    args.push("-i".into());
    args.push(distorted.into());

    let w = info.width.max(2);
    let h = info.height.max(2);
    let fps = if info.fps > 0.0 { format!("{:.3}", info.fps) } else { "30.0".to_string() };
    let filter = format!(
        "[0:v]fps={fps},scale={w}:{h}:flags=bicubic,format=yuv420p[ref];\
         [1:v]fps={fps},scale={w}:{h}:flags=bicubic,format=yuv420p[main];\
         [main][ref]libvmaf=log_path={log}:log_fmt=json:n_threads={threads}",
        log = log_file_name,
        threads = n_threads.max(1),
    );
    args.extend(["-lavfi".into(), filter, "-f".into(), "null".into(), "-".into()]);
    args
}

/// 解析 libvmaf 的 JSON 日志，取 pooled_metrics.vmaf.mean（兼容旧版 "VMAF score"）。
fn parse_vmaf_log(text: &str) -> Option<f64> {
    let v: Value = serde_json::from_str(text).ok()?;
    // 新版格式：{"pooled_metrics": {"vmaf": {"mean": 92.3, ...}}}
    if let Some(mean) = v
        .get("pooled_metrics")
        .and_then(|m| m.get("vmaf"))
        .and_then(|m| m.get("mean"))
        .and_then(|m| m.as_f64())
    {
        return Some(mean);
    }
    // 旧版格式：{"VMAF score": 92.3}
    v.get("VMAF score").and_then(|s| s.as_f64())
}

/// 计算单段的 VMAF 得分（阻塞，调用方应处于 spawn_blocking 上下文）。
/// ffmpeg 的 cwd 设为唯一的 `work_dir`，日志写相对文件名，避免路径转义问题。
fn run_segment(
    ffmpeg_path: &Path,
    reference: &str,
    distorted: &str,
    seg: Option<&VmafSegment>,
    info: &VideoInfo,
    work_dir: &Path,
    log_file_name: &str,
    n_threads: usize,
) -> AppResult<f64> {
    let args = build_segment_args(reference, distorted, seg, info, log_file_name, n_threads);

    let output = ffmpeg::hidden_command(ffmpeg_path)
        .current_dir(work_dir)
        .args(&args)
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| AppError::Ffmpeg(format!("Failed to run ffmpeg VMAF: {}", e)))?;

    if !output.status.success() {
        // 取 stderr 末尾几行作为错误线索
        let stderr = String::from_utf8_lossy(&output.stderr);
        let tail: Vec<&str> = stderr.lines().rev().take(5).collect::<Vec<_>>();
        let mut tail = tail;
        tail.reverse();
        let where_at = match seg {
            Some(s) => format!("@{:.1}s", s.start),
            None => "(全片)".to_string(),
        };
        return Err(AppError::Ffmpeg(format!(
            "VMAF segment {} failed: {}",
            where_at,
            tail.join(" | ")
        )));
    }

    let log_text = std::fs::read_to_string(work_dir.join(log_file_name))
        .map_err(|_| AppError::Ffmpeg("VMAF log file missing".into()))?;
    let where_at = match seg {
        Some(s) => format!("@{:.1}s", s.start),
        None => "(全片)".to_string(),
    };
    parse_vmaf_log(&log_text)
        .ok_or_else(|| AppError::Ffmpeg(format!("Cannot parse VMAF log {}", where_at)))
}

/// 保留两位小数
fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// RAII 临时目录守卫：无论成功/失败/panic 都清理本次计算的工作目录。
struct WorkDirGuard(PathBuf);

impl Drop for WorkDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// 对参考/失真视频对执行采样 VMAF 计算（阻塞）。
///
/// `segments == 0` 表示全量对比（整片一次打分，耗时随视频时长线性增长）；
/// 否则为均匀采样：`segments` 段 × `segment_len` 秒，取平均分。
/// `work_id` 用于隔离每次计算的临时目录（并发计算互不干扰）；
/// 结束后只清理本次调用自己的目录。
pub fn compute_vmaf_sampled(
    reference: &str,
    distorted: &str,
    segments: usize,
    segment_len: f64,
    work_id: &str,
) -> AppResult<VmafResult> {
    if !ffmpeg_supports_vmaf() {
        return Err(AppError::Ffmpeg(
            "当前 ffmpeg 未编译 libvmaf 滤镜，无法计算 VMAF 得分".into(),
        ));
    }
    let ffmpeg_path = ffmpeg::get_ffmpeg_path()
        .ok_or(AppError::FfmpegNotFound)?;

    // 探测参考视频规格（失真视频可能已缩放/改帧率，一律对齐到参考）
    let info = probe_video_info(reference)?;

    let work_dir = std::env::temp_dir().join("z-ffmpeg_vmaf").join(work_id);
    std::fs::create_dir_all(&work_dir)
        .map_err(|e| AppError::Ffmpeg(format!("创建 VMAF 临时目录失败: {}", e)))?;
    // 任何退出路径（含错误）都会清理临时目录
    let _guard = WorkDirGuard(work_dir.clone());

    // libvmaf 线程数：实测 ffmpeg 构建里 n_threads=0（auto）不生效会退化为
    // 单线程（约 5 倍速度差），必须显式传核数。
    let total_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(1, 16);

    // 全量模式：整片一次 libvmaf，吃满全部线程
    if segments == 0 {
        let log_file_name = "full.json".to_string();
        let score = run_segment(
            &ffmpeg_path,
            reference,
            distorted,
            None,
            &info,
            &work_dir,
            &log_file_name,
            total_threads,
        )?;
        return Ok(VmafResult {
            average_score: round2(score),
            segment_scores: vec![round2(score)],
            segment_count: 1,
        });
    }

    let segments = plan_segments(info.duration, segments, segment_len);
    if segments.is_empty() {
        return Err(AppError::Ffmpeg(format!(
            "视频时长过短（{:.1}s），无法进行 VMAF 采样",
            info.duration
        )));
    }

    // 并行段数：多段同时跑各自 ffmpeg 进程，摊满多核；每进程分到的线程数
    // = 核数 / 并行段数，避免进程间线程 oversubscribe。
    let parallel = segments.len().min(total_threads).min(8);
    let per_threads = (total_threads / parallel).max(1);

    let mut scores: Vec<f64> = Vec::with_capacity(segments.len());
    // 按批次并行：每批 parallel 段并发，批次间串行（段数多时避免进程风暴）
    for batch in segments.chunks(parallel) {
        // 捕获引用（Copy），避免 move 所有权
        let ffmpeg_path_ref = &ffmpeg_path;
        let info_ref = &info;
        let work_dir_ref = &work_dir;
        let batch_results = std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(batch.len());
            for (i, seg) in batch.iter().enumerate() {
                // 日志文件名按全局索引唯一，批内批外均不冲突
                let log_file_name = format!("seg_{}.json", scores.len() + i);
                handles.push(scope.spawn(move || {
                    run_segment(
                        ffmpeg_path_ref,
                        reference,
                        distorted,
                        Some(seg),
                        info_ref,
                        work_dir_ref,
                        &log_file_name,
                        per_threads,
                    )
                }));
            }
            let mut results = Vec::with_capacity(handles.len());
            for h in handles {
                results.push(h.join().unwrap_or_else(|_| {
                    Err(AppError::Ffmpeg("VMAF 段计算线程异常终止".into()))
                })?);
            }
            Ok::<Vec<f64>, AppError>(results)
        });
        scores.extend(batch_results?);
    }

    let average = scores.iter().sum::<f64>() / scores.len() as f64;
    Ok(VmafResult {
        average_score: round2(average),
        segment_scores: scores.iter().map(|s| round2(*s)).collect(),
        segment_count: scores.len(),
    })
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_normal_video_uses_four_five_second_segments() {
        let segs = plan_segments(120.0, 4, 5.0);
        assert_eq!(segs.len(), 4);
        for s in &segs {
            assert_eq!(s.length, 5.0);
        }
        // 均匀分布：0 / 1/4 / 2/4 / 3/4
        assert_eq!(segs[0].start, 0.0);
        assert!((segs[1].start - 30.0).abs() < 1e-6);
        assert!((segs[2].start - 60.0).abs() < 1e-6);
        assert!((segs[3].start - 90.0).abs() < 1e-6);
        // 不超出结尾
        assert!(segs[3].start + segs[3].length <= 120.0 + 1e-6);
    }

    #[test]
    fn plan_short_video_shrinks_segment_length() {
        // 10 秒视频：段长应缩短（min(5, 10/8)=1.25）且不重叠
        let segs = plan_segments(10.0, 4, 5.0);
        assert_eq!(segs.len(), 4);
        for s in &segs {
            assert!(s.length <= 1.25 + 1e-6);
            assert!(s.start + s.length <= 10.0 + 1e-6);
        }
        // 段间不重叠
        for w in segs.windows(2) {
            assert!(w[0].start + w[0].length <= w[1].start + 1e-6);
        }
    }

    #[test]
    fn plan_tiny_video_still_produces_segments() {
        let segs = plan_segments(2.0, 4, 5.0);
        assert_eq!(segs.len(), 4);
        assert!(segs.iter().all(|s| s.start + s.length <= 2.0 + 1e-6));
    }

    #[test]
    fn plan_invalid_duration_is_empty() {
        assert!(plan_segments(0.0, 4, 5.0).is_empty());
        assert!(plan_segments(-1.0, 4, 5.0).is_empty());
        // 过短视频（不足 0.5s）无法可靠采样
        assert!(plan_segments(0.3, 4, 5.0).is_empty());
    }

    #[test]
    fn parse_fraction_handles_common_forms() {
        assert!((parse_fraction("30000/1001") - 29.97).abs() < 0.01);
        assert_eq!(parse_fraction("30/1"), 30.0);
        assert_eq!(parse_fraction(""), 0.0);
        assert_eq!(parse_fraction("30"), 0.0);
    }

    #[test]
    fn parse_vmaf_log_new_and_old_formats() {
        let new = r#"{"frames":[],"pooled_metrics":{"vmaf":{"min":88.1,"max":95.2,"mean":92.34}}}"#;
        assert!((parse_vmaf_log(new).unwrap() - 92.34).abs() < 1e-6);

        let old = r#"{"VMAF score": 90.5}"#;
        assert!((parse_vmaf_log(old).unwrap() - 90.5).abs() < 1e-6);

        assert!(parse_vmaf_log("not json").is_none());
    }

    #[test]
    fn build_args_full_mode_omits_seek() {
        let info = VideoInfo { duration: 100.0, width: 1920, height: 1080, fps: 30.0 };
        let seg = VmafSegment { start: 25.0, length: 5.0 };

        // 采样模式包含 -ss/-t 且显式 n_threads
        let sampled = build_segment_args("a.mp4", "b.mp4", Some(&seg), &info, "seg_1.json", 8);
        assert!(sampled.iter().any(|a| a == "-ss"));
        assert!(sampled.iter().any(|a| a == "-t"));
        assert!(sampled.iter().any(|a| a.contains("n_threads=8")));

        // 全量模式不截段
        let full = build_segment_args("a.mp4", "b.mp4", None, &info, "full.json", 4);
        assert!(!full.iter().any(|a| a == "-ss"));
        assert!(!full.iter().any(|a| a == "-t"));
        // 两个输入都在，且 n_threads 至少为 1
        assert_eq!(full.iter().filter(|a| *a == "-i").count(), 2);
        assert!(full.iter().any(|a| a.contains("n_threads=")));
    }

    #[test]
    fn round2_rounds_half_up() {
        // .x5 边界 f64 可能双向舍入，断言落在两个合法值之一
        assert!([92.34, 92.35].contains(&round2(92.345)));
        assert!((round2(92.344) - 92.34).abs() < 1e-6);
    }
}
