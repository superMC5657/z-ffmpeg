# 统一日志（logging）

本地为主、不上报。Rust 侧经 `tauri-plugin-log` v2 落盘，前端经 `src/lib/z-log.ts` 转发到同一份日志文件。实现入口：`src-tauri/src/z_log.rs`，插件装配与命令注册见 `src-tauri/src/lib.rs`。

## 日志目录（OS 路径）

`z_log::log_dir()`：优先 `app_log_dir()`，失败回退 `app_data_dir()/logs`，目录不存在则创建，初始化时执行一次 `prune` 修剪。

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
| 门槛覆盖 | `ZFFMPEG_LOG` 优先、`RUST_LOG` 兜底（`resolve_level`，非法值穿透到默认） | 同左 |
| `zffmpeg_lib::encoder` | `Debug` | `Info` |
| `reqwest` / `hyper` / `tungstenite` | `Warn` | `Warn` |
| 前端 console 镜像 | `attachConsole()`（仅 DEV，失败不阻塞启动） | 无 |
| 单文件滚动 | 5MB（`MAX_FILE_SIZE`，`KeepSome(5)` 保留 5 个轮转兄弟） | 同左 |
| 总量水位 + 保留期 | 14 天 / 25MB（`prune`） | 同左 |

前端 `zlog.debug` 在 release 下会被 root `Info` 阈值过滤，无需前端重复判断环境。

## 编码器噪音控制

噪音源是 ffmpeg 子进程的 stderr + `-progress pipe:1` 每帧报告。控制手段：

1. `level_for("zffmpeg_lib::encoder", Info@release / Debug@dev)`：release 下进度类 `Debug/Trace` 不落盘。
2. 进度循环零 log：`encoder/engine.rs` 的 stdout 解析循环内无任何 `log!`；stderr 由独立线程消费，保留尾部 50 行（`STDERR_TAIL_LINES`）用于失败诊断，不逐行打 log。
3. 关键生命周期点位（`target = "zffmpeg_lib::encoder"`）：
   - `error` encode ffmpeg not found（含 `job_id` + basename `file`）
   - `info` encode started（含 `job_id` + basename `file`）
   - `info` encode completed（含 `job_id` + 输出字节数 + `elapsed` 秒）
   - `error` encode failed（含 `job_id` + `exit_code` + `elapsed` + stderr 尾部单行摘要）
   - `warn` encode cancelled（含 `job_id` + 已耗时）
4. 进度事件（`encode://progress`）走 Tauri emit → store，不走日志。

## 授权事件（`license/manager.rs`）

只记事件不记敏感值：激活码、email、JWT、deviceId 一律不进日志。

- `info` license activated（激活成功）
- `warn` license activate failed: {code} / network failed（激活失败）
- `warn` license activate token verify failed（服务端令牌本地验签不通过）
- `debug` license verify ok（24h 周期续验成功，release 不可见）
- `warn` license verify token check failed（续验新令牌本地验签失败）
- `warn` license verify offline, using grace period（网络失败走离线宽限期）
- `warn` license revoked ({code}), credentials removed（吊销并清除本地凭证）
- `debug` license verify deferred: {code}（其他服务端错误，保留凭证等重激活）
- `info` license deactivated（注销成功）/ `warn` 注销失败
- `error` license periodic verify task aborted（后台任务异常终止）

## 队列事件（`queue/manager.rs`）

- `info` queue enqueued（含 `job_id` + basename `file`）
- `warn` queue retry（手动重进队列；含 `job_id` + basename + 失败原因首行）
- `warn` queue cancelled（Pending 状态取消；含 `job_id`）
- `error` queue failed（任务终态转 `Failed`；含 `job_id` + basename + 失败原因首行）
- `error` queue db open/init failed（`queue.db` 打开或建表失败）
- `debug` queue load jobs skipped（`load_jobs` 恢复失败，release 不可见）

## FFmpeg 本体（`ffmpeg/library.rs` + `commands/system.rs`）

- `info` ffmpeg detected {bundled|external} version（含来源 + 版本号）/ `info` ffmpeg missing（未检测到；`debug` 记 basename）
- `error` ffmpeg download failed（下载失败摘要）
- `error` 下载源切换 / 校验失败 / 解包缺二进制（`ffmpeg/downloader.rs` 各记一条，不记 URL）
- `info` ffmpeg downloaded version + elapsed（下载完成；含版本 + 耗时秒）

## 探测 / 预设 / VMAF（`commands/encode.rs` / `preset.rs` / `vmaf.rs`）

- `warn` probe failed（文件不存在 / ffprobe 报错 / 结果解析失败；只记 basename + 原因首行）
- `warn` preset import/export failed（导入导出失败原因首行）
- `warn` vmaf compute failed（计算失败；含 `job_id` + 原因首行）

## 脱敏（redact）

`z_log::redact()` 在写盘前 `format` 内执行（导出诊断包时二次脱敏为纵深）：

- 邮箱 → `[redacted-email]`
- `token[:=] <值>` / `code[:=] <值>` → `[redacted]`
- `C:\Users\<名>\` / `/home/<名>/` / `/Users/<名>/` → `[user]`

## 导出诊断包

- `zlog_get_dir`：返回日志目录绝对路径。
- `zlog_export_bundle`：把 `*.log` 脱敏后打包为系统 temp 下 `zffmpeg-logs-<timestamp>.zip`，附 `bundle-info.txt`（版本 + 导出时间）；无 log 时包内放 `README.txt` 占位。返回 zip 绝对路径。

## 修剪策略（prune）

仅处理 `*.log`：删除 mtime 超过 14 天的文件，当总量超过 25MB 时按 mtime 从旧到新删除至总量 ≤ 25MB。

## panic hook

`install_panic_hook()` 在启动时安装：以 `target "zffmpeg::panic"` 记录 `error` 级别 panic 位置与 payload，再调用默认 hook。

## 前端接入（`src/lib/z-log.ts`）

- `initZLog()`（`src/main.tsx` 初始化）：DEV 下 `attachConsole()`；注册全局 `window.onerror` / `onunhandledrejection` 转发为 `error` 落盘。
- `zlog.{debug,info,warn,error}`：经 IPC 落到 Rust 日志文件。
- `getLogDir()` / `exportLogBundle()`：invoke 对应 Rust 命令。

## 约束边界

- 不做任何远程日志上报（无 Sentry / Tracing 上报），诊断仅支持本地导出 zip。
- 日志修剪与导出仅处理 `*.log` 文件，不触碰业务数据库 `queue.db` 与 `presets.db`。
