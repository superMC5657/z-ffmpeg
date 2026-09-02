# 代码审查 — z-ffmpeg 0.0.1（`68a1d43..57191e0`）

审查了完整的 “0.0.1” 重写（后端 `encoder`、`queue`、`preset`、`commands`；前端 `stores`、`routes`、`components`）。已验证 `cargo check`、`tsc --noEmit` 与 `pnpm build` 均通过。

## 发现的问题

### [P2] 使用自定义输出目录时需对输出路径去重 — `src-tauri/src/encoder/engine.rs:256`

`derive_output_path` 总是在所选输出目录内生成 `{文件名}_encoded.{扩展名}`。当添加的两个输入来自不同目录但同名（或重复添加同一文件）时，两个任务会得到相同的输出路径，而 `-y` 参数会让 ffmpeg 静默覆盖前一个任务的输出。`add_to_queue` 与 `build_ffmpeg_commands` 都使用该函数，因此都会受影响。

### [P2] 预设页面的“选择”应真正应用预设 — `src/components/preset/PresetCard.tsx:107`

选择按钮只调用 `selectPreset`，它仅设置预设 store 中的 `selectedPresetId`，并没有把该配置应用到编码器 store。因此在预设页面选中预设后，编码页面的下拉框会显示该预设已被选中，但表单字段（编码器、码率控制、封装格式等）仍是旧值；“添加到队列”使用的还是过期配置。

### [P2] 队列页面的“清除已完成”不应删除历史记录 — `src-tauri/src/queue/manager.rs:157`

`clear_completed` 会从 SQLite 中删除 Completed/Failed/Cancelled 记录，而 `history()` 读取的是同一张表。由于 `dequeue_next` 现在把已完成任务保留在内存队列中（重写时将 `remove` 改为原地更新状态），队列页面会展示这些任务，清除按钮也随之变得可用；点击它会悄悄清掉历史页面显示的所有记录。历史页面另有独立的“清空历史”按钮，说明两者本应相互独立。

### [P3] 对刚启动的任务应保证取消可靠生效 — `src-tauri/src/queue/manager.rs:198`

即使 `engine::cancel_process` 返回 false（此时尚未注册 ffmpeg 子进程——子进程是在 `start_encode` 内部 spawn 之后才插入 `PROCESSES` 的），`cancel_job` 仍会把任务标记为 Cancelled。在这个时间窗口内，编码会在后台继续运行直至完成并写出输出文件，而界面却显示“已取消”。

### [P3] 接线或移除失败队列项上无效的“重试”按钮 — `src/components/queue/QueueItem.tsx:54`

为 `Failed` 任务渲染的 `RotateCw` 按钮没有 `onClick` 处理函数，点击后没有任何反应，却看起来可交互。

### [P3] 若删除并非有意为之，应恢复 MP4 输出的 faststart — `src-tauri/src/encoder/engine.rs:131`

重写删除了旧版本无条件添加的 `-movflags +faststart`，MP4 输出不再进行 faststart 优化。如果是有意删除，建议改为仅对 `MP4`/`MOV` 封装条件性添加。

### [P3] 前端 `EncodeProgress` 类型应与后端负载对齐 — `src/types/index.ts:92`

该类型声明了 `totalFrames`、`etaSeconds`、`timeElapsed`、`outputSizeBytes`，但后端实际发送的是 `totalSizeKb`、`elapsed`（字符串）与 `time`（字符串），见 `src-tauri/src/encoder/progress.rs:10`。UI 读取的字段（`percentage`、`fps`、`speed`、`bitrate`、`fileName`）两侧都有，因此目前不会出错；但若新 UI 代码使用了这些声明的字段，会静默得到 `undefined`。

## 总体评价

核心重写质量扎实：取消已改为进程级（注册表 + kill）、进度解析迁移到 ffmpeg 机器可读的 `-progress` 输出、历史记录由数据库持久化并可跨重启保留，新增的 Rust 单元测试覆盖了队列持久化、预设读写往返与硬件平台过滤。

**测试缺口**

- 缺少对新引擎进度/取消逻辑以及 `derive_output_path`（冲突场景）的测试。
- 缺少前端测试。

**残余风险**

- `process_queue` 的“检查后执行”循环（`can_start` → `dequeue_next` → `inc_active`）在并发调用时可能超发任务（该问题原本就存在，但现在由用户按钮触发）。
- 失败队列任务总是显示误导性的“Output file not created”错误，因为 `start_encode` 的返回值与 stderr 诊断信息都被丢弃了。
