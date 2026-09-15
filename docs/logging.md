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
3. 生命周期点位（已接入，均为 `target = "zffmpeg_lib::encoder"`，行号见末节）：
   - `error` encode ffmpeg not found（含 `job_id` + basename `file`）
   - `info` encode started（含 `job_id` + basename `file`，不记完整 args 与全路径）
   - `info` encode completed（含 `job_id` + 输出字节数 + `elapsed` 秒）
   - `error` encode failed（含 `job_id` + `exit_code` + `elapsed` + stderr 尾部；尾部复用 `STDERR_TAIL_LINES=50` 截尾变量拼为单行，不 dump 全文；完整尾部只进 UI 事件载荷）
   - `warn` encode cancelled ×3（同一文案加时机词 `stage=pre-spawn / post-spawn / running`；含 `job_id` + 已耗时）
4. 进度事件（`encode://progress`）走 Tauri emit → store，不走日志。

禁止在进度循环加 log；新增编码日志必须落在上述生命周期点位上。

## 授权事件（`license/manager.rs`）

只记事件不记值：激活码 / email / JWT / deviceId 一律不进日志。`ApiError` 只记 `code`（固定枚举），不记 `message`（服务端自由文本）；`Network` 只记静态文案与 reqwest 错误（POST 传参，URL 无凭据）。

- `info` license activated（激活成功）
- `warn` license activate failed: {code} / network failed（激活失败）
- `warn` license activate token verify failed（服务端令牌本地验签不过）
- `debug` license verify ok（24h 周期续验成功，release 不可见）
- `warn` license verify token check failed（续验新令牌本地验签不过，返回 Err 给调用方）
- `warn` license verify offline, using grace period（网络失败走离线宽限期）
- `warn` license revoked ({code}), credentials removed（吊销删凭证）
- `debug` license verify deferred: {code}（其他服务端错误，保留凭证等重激活）
- `info` license deactivated（注销成功）/ `warn` 注销失败（同上只记 code）
- `error` license periodic verify task aborted（后台任务本身异常终止）

## 队列事件（`queue/manager.rs`）

本项目无自动重试：失败即终态 `Failed`，重进队列只支持用户手动 `retry_job`。

- `info` queue enqueued（含 `job_id` + basename `file`；编码配置摘要只 `debug`）
- `warn` queue retry（手动重进队列；含 `job_id` + basename + 上次失败原因首行，无“第几次”计数）
- `warn` queue cancelled（仅 Pending 取消，未进 engine 的盲区补记；含 `job_id`；Running 取消由 engine 侧 `stage=pre-spawn / post-spawn / running` 记，不双记；点位 `queue/manager.rs:254`）
- `error` queue failed（任务终态转 `Failed`；含 `job_id` + basename + 失败原因首行）
- `error` queue db open/init failed（`new()` 打开 `queue.db` / 建表失败，先 log 再返回 `Err`）
- `debug` queue load jobs skipped（`load_jobs` 恢复失败，release 不可见）

## FFmpeg 本体（`ffmpeg/library.rs` + `commands/system.rs`）

- `info` ffmpeg detected {bundled|external} version（含来源 + 版本号；完整路径只 `debug`）/ `info` ffmpeg missing（启动未检出）
- `error` ffmpeg download failed（下载失败；取聚合错误首行 = 通用头行，多源明细 URL + 各源原因只回 UI 不进日志）
- `info` ffmpeg downloaded version + elapsed（下载完成；含版本 + 耗时秒）

## 探测 / 预设 / VMAF（`commands/encode.rs` / `preset.rs` / `vmaf.rs`）

- `warn` probe failed（文件不存在 / ffprobe 报错 / 结果解析失败；只记 basename + 原因首行，不记全路径）
- `warn` preset import/export failed（导入 JSON 解析 / 结构校验 / 入库失败、导出目标缺失 / 写盘失败；只记原因首行，不记 JSON 原文与目标全路径）
- `warn` vmaf compute failed（任务缺失 / 文件缺失 / 分段计算失败；含 `job_id` + 原因首行，不记输入输出路径；duration 读不到分支只记 basename，见 `encoder/vmaf.rs:105-115`）

消息一律英文小写前缀模块词，只记摘要不记大段文本；单条 stderr/原因截断（尾部 50 行或首行封顶）。

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
- `src-tauri/src/encoder/engine.rs`（7 处生命周期打点：109/123/170/192/296/310/349，进度循环零 log）
- `src-tauri/src/queue/manager.rs`（队列打点：39/68/109/177-178/254/282/316）
- `src-tauri/src/encoder/vmaf.rs`（duration 分支 basename：105-115）
- `src-tauri/src/ffmpeg/library.rs`（启动检测打点：68-71）
- `src-tauri/src/commands/system.rs`（下载完成/失败：105/130）
- `src-tauri/src/commands/encode.rs`（探测失败：46/59/68）
- `src-tauri/src/commands/preset.rs`（导入导出失败：172/201/223，其中 223 为闭包、覆盖 6 处导入失败路径）
- `src-tauri/src/commands/vmaf.rs`（计算失败：40/46/76/83）
- `src-tauri/src/license/manager.rs`（授权事件打点，见上节）
- `src/lib/z-log.ts`、`src/main.tsx`（前端接入）
- `src-tauri/capabilities/default.json`（`log:default`）
