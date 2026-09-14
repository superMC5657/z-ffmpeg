# 统一日志（logging）

本地为主、不上报。Rust 侧经 `tauri-plugin-log` v2 落盘，前端经 `src/lib/z-log.ts` 转发到同一份日志文件。实现入口：`src-tauri/src/z_log.rs`，插件装配与命令注册见 `src-tauri/src/lib.rs`。

## 日志目录（OS 路径）

`z_log::log_dir()`：优先 `app_log_dir()`，失败回退 `app_data_dir()/logs`，目录不存在则创建，每次进入时跑一次 `prune`。

| OS | 目录 |
|---|---|
| Windows | `%APPDATA%\com.zffmpeg.app\logs`（identifier 跟随 `tauri.conf.json`） |
| macOS | `~/Library/Logs/com.zffmpeg.app` |
| Linux | `~/.local/share/com.zffmpeg.app/logs`（XDG 下的 app log dir） |
| 回退 | `{app_data_dir}/logs`，极端情况 `./logs` |

## dev / release 矩阵

| | dev（debug_assertions） | release |
|---|---|---|
| Target | `LogDir` + `Stdout` + `Webview` | 仅 `LogDir` |
| root level | `Debug` | `Info` |
| `zffmpeg_lib::encoder` / `zffmpeg::encoder` | `Debug` | `Info` |
| `reqwest` / `hyper` / `tungstenite` | `Warn` | `Warn` |
| 前端 console 镜像 | `attachConsole()`（仅 DEV，失败不阻塞启动） | 无 |
| 单文件滚动 | 20MB（`MAX_FILE_SIZE`，`KeepAll` 按日期重命名旧文件） | 同左 |
| 总量水位 + 保留期 | 7 天 / 20MB（`prune`，见下） | 同左 |

前端 `zlog.debug` 在 release 下会被 root `Info` 阈值过滤，无需前端再判环境。

## 编码器噪音控制

噪音源是 ffmpeg 子进程的 stderr + `-progress pipe:1` 每帧报告。控制手段：

1. `level_for("zffmpeg_lib::encoder" / "zffmpeg::encoder", Info@release / Debug@dev)`：双前缀都压住（crate 名为 `zffmpeg_lib`，兼容 `zffmpeg::encoder` 写法），release 下进度类 `Debug/Trace` 不落盘。
2. 进度循环零 log：`encoder/engine.rs` 的 stdout 解析循环内无任何 `log!`；stderr 由独立线程消费，只保留最后 50 行（`STDERR_TAIL_LINES`）用于失败诊断，不逐行打 log。
3. 生命周期点位（仅这 6 处，均为 `target = "zffmpeg_lib::encoder"`）：
   - `info` encode started（含 `job_id` + basename `file`，不记全路径）
   - `info` encode completed（含 `size_bytes` + `elapsed`）
   - `info` encode failed（含 `exit_code` + `elapsed`；完整 stderr 尾部只进 UI 事件载荷，不进日志）
   - `warn` encode cancelled ×3（spawn 前 / spawn 后注册窗口 / 运行中；含 `job_id` + `elapsed`）
4. 进度事件（`encode://progress`）走 Tauri emit → store，不走日志。

禁止在进度循环加 log；新增编码日志必须落在上述生命周期点位上。

## 脱敏（redact）

`z_log::redact()` 纯手写、无新依赖，只在导出诊断包时对文件内容执行（原始落盘文件不改写）：

- 邮箱 → `[redacted-email]`（字符边界安全，中文日志不受损）
- `token[:=] <值>` / `code[:=] <值>` → `[redacted]`（大小写不敏感）
- `C:\Users\<名>\` / `/home/<名>/` / `/Users/<名>/` → `[user]`

单测锁定：`redact_masks_email_token_and_username`、`redact_keeps_chinese_text_intact`。

## 导出诊断包

- `zlog_get_dir`：返回日志目录绝对路径（“打开日志目录”按钮用）。
- `zlog_export_bundle`：把 `*.log` 脱敏后打包为系统 temp 下 `zffmpeg-logs-<timestamp>.zip`，附 `bundle-info.txt`（版本 + 导出时间）；无 log 时包内放 `README.txt` 占位。返回 zip 绝对路径。

## 修剪策略（prune）

只管 `*.log`：先删 mtime 超 7 天的文件，再按 mtime 从旧到新删到总量 ≤ 20MB。单测锁定：`prune_keeps_fresh_logs_and_ignores_db`。

## panic hook

`install_panic_hook()` 在 `run()` 最早安装：以 `target "zffmpeg::panic"` 记一条 `error`（位置 + payload），再调用默认 hook。release 无 console，全靠落盘。

## 前端（`src/lib/z-log.ts`）

- `initZLog()`（`src/main.tsx` 调用一次）：DEV 下 `attachConsole()`；注册 `window.onerror` / `onunhandledrejection` 转 `error()` 落盘。均为低频路径，直接转发、无批处理队列。
- `zlog.{debug,info,warn,error}`：直接透传 `@tauri-apps/plugin-log`（经 IPC 落到 Rust 日志文件）。
- `getLogDir()` / `exportLogBundle()`：invoke 对应 Rust 命令。Capability 依赖 `log:default`（`src-tauri/capabilities/default.json`）。

## 不做声明

- 不上报：无 tracing / sentry / 远程日志上报；诊断只支持本地导出 zip（analytics 会话上报是另一条链路，与日志无关）。
- 禁 sqlite：日志模块不碰业务库 `queue.db` / `presets.db`（prune 与导出包均只匹配 `*.log`）。
- 禁动 release 体积优化：`Cargo.toml` 的 `custom-protocol` feature、`opt-level="z"` / `lto="fat"` / `codegen-units=1` / `strip` / `panic="abort"` 与日志无关，不得借日志改动触碰。

## 相关文件

- `src-tauri/src/z_log.rs`（插件构建、prune、redact、panic hook、commands、单测）
- `src-tauri/src/lib.rs`（`install_panic_hook` + `plugin(z_log::init())` + `zlog_get_dir` / `zlog_export_bundle` 注册）
- `src-tauri/src/encoder/engine.rs`（6 处生命周期 log，进度循环零 log）
- `src/lib/z-log.ts`、`src/main.tsx`（前端接入）
- `src-tauri/capabilities/default.json`（`log:default`）
