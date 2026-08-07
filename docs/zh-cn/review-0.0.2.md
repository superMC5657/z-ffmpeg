# 代码审查 — zffmpeg 0.0.2（`57191e0..6a76b26`）

审查了 0.0.2 提交。该提交正确修复了之前文档记录的缺口（输出路径去重、预设应用、历史记录保留、重试按钮、faststart、进度类型对齐），新增的 Rust 测试也可靠；但新的取消机制仍留下了一个真实的时间窗口：被取消任务的 ffmpeg 子进程可能永远不会被终止并继续写出输出文件；同时预设应用退化为对格式错误的导入预设进行无防护的字段读取。

## 发现的问题

### [P2] 在探测/启动窗口内取消仍会导致 ffmpeg 继续运行 — `src-tauri/src/encoder/engine.rs:462-481`

新增的启动前取消检查只在 `start_encode` 最开头（`probe_file` 之前）执行一次，而子进程要到 spawn 之后才注册进 `PROCESSES`。如果用户在该检查之后、子进程注册之前取消（探测本身可能耗时数百毫秒），`cancel_job`（manager.rs）虽然会设置标志位，但 `cancel_process` 会因为尚未注册子进程而返回 false，导致 ffmpeg 子进程永远不会被终止。随后 stdout 读取循环在收到第一个进度块时因标志位而退出，关闭管道；ffmpeg 要么在下次写入时因 EPIPE 退出（留下部分输出文件），要么在短编码场景下运行到完成并写出完整输出文件——而 UI 一直显示任务为“已取消”，工作线程的 `wait()` 还占着一个并发名额。因此，点击开始后立即取消的任务仍可能产出输出文件并浪费 CPU。建议在把子进程插入 `PROCESSES` 后立即重新检查 `cancel.load()`（若已设置则 kill），以关闭该窗口；`process_queue` 中 `dequeue_next` 与 `cancel_flags.insert` 之间也存在同类竞态。

### [P3] 对格式错误的预设，applyConfig 会用 undefined 覆盖表单状态 — `src/store/encoderStore.ts:157-172`

`applyConfig` 无条件下读取 `vs.rateControl`、`as_.codec` 与 `as_.bitrateKbps`，而之前的 `PresetSelector` 代码对每个字段都有防护（`if (rc) setRateControl(rc)`、`if (as_?.codec) ...`）。后端 `import_preset` 接受任意 JSON 对象作为预设配置而不做 schema 校验，因此缺少 `videoSettings.rateControl`（或 `audioSettings` 不完整）的预设会把编码器 store 字段设为 `undefined`——若 `videoSettings` 本身缺失则会抛出 TypeError——随后 `buildConfig()`/`addToQueue` 会在 Rust 侧 serde 反序列化时失败。建议保留原有防护（`vs?.rateControl` 等），或在应用前校验预设 schema。

## 总体评价

v0.0.1 文档中记录的问题已全部解决：输出路径会在同一批次内去重（`derive_output_paths_unique`，附测试）；MP4/MOV 输出重新添加了 `-movflags +faststart`（按封装条件判断）；队列页面的“清除已完成”不再删除历史记录；失败队列项有了可用的重试操作；预设选择现在通过 `applyConfig` 应用；`EncodeProgress` 类型也与后端负载对齐。新增的 Rust 单元测试覆盖了去重与 faststart 行为。

上述两个剩余问题即下一步要处理的残余风险：取消/探测启动竞态（P2）以及 `applyConfig` 对格式错误导入预设的无防护读取（P3）。
