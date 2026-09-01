# z-ffmpeg

FFmpeg CLI 驱动的跨平台桌面视频编码应用（Rust + Tauri v2 + React 19 + TS + Tailwind v4）。

## 命令

```bash
pnpm install          # 安装依赖
pnpm tauri dev        # 开发模式（vite :1420）
pnpm tauri build      # 生产构建
pnpm test             # 前端 vitest 测试
pnpm lint             # eslint（0 error 才能合入，警告可保留）
cd src-tauri && cargo test   # Rust 单元测试
pnpm exec tsc --noEmit # TS 类型检查
```

CI（tag 推送时）跑 tsc + eslint + vitest + cargo test；发布流水线 `release-tauri.yml`（同样 tag 触发）与 CI 并行跑，发布不等待质量门。

## 架构

- `src-tauri/src/commands/` — Tauri IPC 处理器（encode/queue/preset/system/history），全部在 `lib.rs` 注册。
- `src-tauri/src/encoder/` — 编码引擎：`engine.rs`（ffmpeg 子进程生命周期 + 进度循环 + 取消）、`args.rs`（参数/输出路径构建）、`probe.rs`（ffprobe 探测 + FileInfo 解析）、`progress.rs`（进度结构与 -progress 解析）、`codec.rs`、`hw_accel.rs`、`estimate.rs`（体积预估）、`vmaf.rs`。
- `src-tauri/src/queue/` — `QueueManager`：SQLite 队列、自动推进、并发、重试；`settings.rs` 为 settings 表存储层。DB 在 `{app_data_dir}/queue.db`（Tauri appDataDir，Windows = `%APPDATA%\com.zffmpeg.app`）。
- `src-tauri/src/preset/` — 内置预设（`commands/preset.rs`，18 个只读）+ 自定义预设（presets.db，JSON 导入导出）。
- `src-tauri/src/ffmpeg/` — `library.rs` PATH 检测、`mod.rs` 的 `hidden_command()`（Windows 隐藏控制台窗口）。
- 前端：`src/routes/` 5 页面、`src/store/`（Zustand）、`src/hooks/useEncodeEvents.ts`、`src/lib/tauri.ts`。

## 约定

- 命令返回 `AppResult<T>`；错误序列化为字符串供前端直接展示。
- Rust `pub` 结构体统一 `#[serde(rename_all = "camelCase")]`，对齐 `src/types/index.ts`。
- 进度事件走 Tauri emit → store；UI 文案为中文；release profile 为体积优化，勿改。

## Agent skills

### Issue tracker

Issues 存放在 GitHub Issues（使用 `gh` CLI）。见 `docs/agents/issue-tracker.md`。

### Triage labels

五个 canonical triage labels：`needs-triage`、`needs-info`、`ready-for-agent`、`ready-for-human`、`wontfix`。见 `docs/agents/triage-labels.md`。

### Domain docs

Single-context layout：repo 根目录一个 `CONTEXT.md` + `docs/adr/`。见 `docs/agents/domain.md`。
