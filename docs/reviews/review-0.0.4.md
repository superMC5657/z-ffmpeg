# 代码审查 — z-ffmpeg 0.0.4（`6f2a2ee..b2fe0f9`）

审查了 VMAF 评分与输出体积预估功能提交（按任务计算 VMAF，支持全量/采样两种模式并持久化到 DB；入队时预估体积；队列页与历史页 UI 接线）。补丁可以编译，新增 Rust 测试全部通过，前端类型检查通过，现有行为无回归；以下两个发现是新功能中的非阻塞功能缺口，而非回归或阻塞性问题。

## 发现的问题

### [P2] VMAF 计算只能解析仍在内存储队列中的任务 — `src-tauri/src/commands/vmaf.rs:30-32`

`compute_vmaf` 通过 `QueueManager::get_job_paths` 解析输入/输出路径，而该方法只搜索内存中的 `jobs` 双端队列（`src-tauri/src/queue/manager.rs:363-369`）。已完成的任务只会在当前会话中加载进内存：`load_jobs` 只恢复 Pending/Encoding/Paused 状态的行（`manager.rs:118-125`），而 `clear_completed` / 删除已完成任务会把它从内存移除，却保留 DB 行。在这些情况下 `compute_vmaf` 会报“任务不存在”，尽管输入/输出路径仍保留在 `jobs` 表中；而 History 页（直读 DB 并展示 VMAF）没有任何计算入口，因此重启后过去的编码任务永远无法再评分。在 `get_job_paths` 中回退到 `history()` 的行（或直接从 DB 解析路径）即可填补这一缺口。注意 `set_vmaf_score`（`manager.rs:372-378`）同样只更新内存中的任务，因此修复时还必须为仅存在于 DB 的任务持久化得分（例如按 id 执行 UPDATE），否则计算结果会被静默丢弃。

### [P3] ABR 体积预估没有考虑 `VideoCodec::Copy`，与 CRF/CQP 分支不一致 — `src-tauri/src/encoder/estimate.rs:44`

在 `estimate_output_bytes` 中，`RateControl::Abr` 分支始终把配置的 `bitrate_kbps` 当作视频码率，而 CRF/CQP 分支在 `video_codec` 为 `VideoCodec::Copy` 时都会显式短路、退化为按输入平均码率预估（`estimate.rs:47-49`、`estimate.rs:61-63`）。Copy 视频流会忽略 ABR 码率，因此任何 Copy + ABR 的配置/预设，Pending 阶段的预估都可能偏差很大（实际输出体积基本接近输入）。修复方式是照搬其他分支已有的 Copy 判断。

## 总体评价

该提交为两个功能打下了可靠基础：每次 VMAF 计算使用唯一工作目录（`job_id` + UUID），重算与并发计算互不干扰；段数限制在 1..=32，0 表示全量对比；结果持久化到 DB 并通过 `queue://updated` 通知前端；体积预估在入队时并行执行，探测失败时优雅降级为“不展示预估”，不会阻塞队列。

上述两个问题是下一步需要处理的残余风险：VMAF 的路径解析（以及得分持久化）必须回退到 DB，让已完成任务在重启后、执行 `clear_completed` 之后仍可评分（P2）；ABR 分支需要补上与其它码率控制分支一致的 Copy 短路逻辑（P3）。
