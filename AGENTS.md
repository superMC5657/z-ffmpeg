# zffmpeg

FFmpeg CLI 驱动的跨平台桌面视频编码应用（Rust + Tauri v2 + React 19 + TS + Tailwind v4）。

## 命令

```bash
pnpm install          # 安装依赖
pnpm tauri dev        # 开发模式（vite :1420）
pnpm tauri build      # 生产构建
cd src-tauri && cargo test   # Rust 单元测试
pnpm exec tsc --noEmit # TS 类型检查（CI 只跑这个）
```

## 架构

- `src-tauri/src/commands/` — Tauri IPC 处理器（encode/queue/preset/system/history），全部在 `lib.rs` 注册。
- `src-tauri/src/encoder/` — 编码引擎：`engine.rs`（spawn_blocking 跑 ffmpeg + 进度解析）、`codec.rs`、`hw_accel.rs`。
- `src-tauri/src/queue/` — `QueueManager`：SQLite 队列、自动推进、并发、重试。DB 在 `{data_dir}/zffmpeg/queue.db`。
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
