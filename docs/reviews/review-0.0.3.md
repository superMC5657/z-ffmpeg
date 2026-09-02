# 代码审查 — z-ffmpeg 0.0.3（`e65aa50..6f2a2ee`）

审查了 FFmpeg 自动下载功能提交（镜像回退链、zip 解压、安装到 `{data_dir}/z-ffmpeg/ffmpeg`、状态刷新与设置页 UI 接线）。重构与 UI 接线整体可靠，代码也能基于锁定的依赖版本编译；但新下载功能在失败处理上存在真实缺陷：解压失败时镜像回退不会触发，且无法运行的下载二进制会被报告为“已安装”。这些对现有功能不构成阻塞，但在依赖该功能之前应当修复。

## 发现的问题

### [P2] 解压/校验失败时镜像回退永远不会触发 — `src-tauri/src/ffmpeg/downloader.rs:63-70`

`download_zip` 中按源逐个回退的逻辑只对 HTTP/下载错误做出反应。一旦某个源返回 HTTP 200，`download_from` 就会把响应体写入 `ffmpeg-download.zip` 并让 `download_zip` 返回 `Ok`，随后 `extract_binaries` 在重试循环之外执行。如果某个代理（前两个源都是 GitHub 加速代理）返回了 200 错误页/HTML 页，或返回的截断响应恰好不是 zip 文件，解压就会失败并使整个命令中止——剩余的镜像（包括 gyan.dev 兜底源）永远不会被尝试。既然源列表本来就是按回退链设计的，就应该把解压失败视为该源失败并继续尝试下一个 URL（或在解压前先校验 zip）。

### [P2] 下载完成却不验证二进制能否运行，直接报告“已安装” — `src-tauri/src/ffmpeg/downloader.rs:86-90`

把解压出的二进制移动到目标位置后，`download_ffmpeg` 调用 `get_status_from_paths`，后者无条件设置 `available: true`，并吞掉 `ffmpeg -version`/`-encoders` 的失败（`library.rs:102-103` 中的 `.ok()` 与 `unwrap_or_default()`）。随后命令返回 `status: "installed"` 并发出 `ffmpeg://ready`，于是 UI 放开编码、底部栏显示“FFmpeg 已就绪”——即使下载到的 `ffmpeg.exe` 根本无法执行（例如代理返回的占位文件或被拦截的二进制）。新功能应将版本检查失败视为下载失败并上报错误（或回退到下一个镜像），而不是宣传一个损坏的安装。

### [P3] 阻塞式 ffmpeg 校验运行在 async 命令线程上 — `src-tauri/src/ffmpeg/downloader.rs:86-88`

`download_ffmpeg`（异步 Tauri 命令）在下载末尾直接于 async 运行时线程上执行 `get_status_from_paths`。该函数会同步地启动 `ffmpeg -version` 与 `ffmpeg -encoders` 子进程（`library.rs:122-125`、`library.rs:134-137`），阻塞运行时工作线程——这正是本次提交在 `detect_hw_accel` 中明确改为 `spawn_blocking` 的同一类阻塞模式（那里的注释写着“避免卡住 async runtime”）。建议同样把校验调用包进 `tauri::async_runtime::spawn_blocking`，以免一个缓慢/卡死的 ffmpeg 拖住所有 Tauri 命令。

## 总体评价

该提交为功能打下了可靠基础：下载由全局锁串行化；先解压到临时目录、再把二进制移动到目标位置（失败的下载/解压不会留下一个在下一次启动时被误判为“已安装”的半成品 ffmpeg.exe）；进度事件驱动设置页进度条；zip 与临时目录在所有路径上都会尽力清理；镜像列表按国内可达的 GitHub 加速代理排序，官方源兜底。

上述三个问题是依赖该功能前需要处理的残余风险：把解压失败当作源失败，让镜像链真正回退（P2）；安装的二进制无法运行时让下载失败，而不是宣传损坏的安装（P2）；并把阻塞式校验移出 async 运行时线程（P3）。
