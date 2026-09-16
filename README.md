# z-ffmpeg

FFmpeg CLI 驱动的跨平台桌面视频编码应用（Rust + Tauri v2 + React 19 + TypeScript + Tailwind v4）。

## 功能

- **批量编码**：多文件入队，SQLite 持久化队列，自动推进、并发控制、失败重试，应用重启后任务不丢。
- **编码参数**：H.264 / H.265 / AV1 / VP9 / 流复制，CRF / CQP / ABR 码率控制，分辨率、帧率、像素格式、音频编码与码率，ffmpeg 命令预览与导出。
- **硬件加速**：NVENC / QSV / AMF / VAAPI / VideoToolbox 自动检测，一键启用。
- **内置预设**：18 个只读预设（B 站投稿、微信压缩等常见场景）+ 自定义预设，支持 JSON 导入导出。
- **体积预估**：入队前与编码页实时预估输出体积（仅供参考的启发式推算）。
- **VMAF 质量评分**：完成后对输出与原片做全量或采样 VMAF 对比，得分随历史持久化。
- **编码历史**：按状态筛选、文件名搜索、分页浏览，记录压缩率与 VMAF 得分。
- **FFmpeg 自动下载**：未检测到 FFmpeg 时可从镜像（国内加速优先）自动下载安装；队列数据统一存放在应用数据目录（Tauri appDataDir，Windows 为 `%APPDATA%\com.zffmpeg.app`）。
- **自动更新**：内置更新器，国内镜像与 GitHub 双端点。

## 版本与授权

`z-ffmpeg` 绝大多数功能均已开放给**免费版**自由使用，仅保留两项高阶/生产力功能为 **Pro 版**：

| 功能模块 | 功能项 | 免费版 | Pro 专业版 |
|---|---|:---:|:---:|
| **基础编码** | H.264 / H.265 / AV1 / VP9 / 流复制等全格式编码 | ✅ | ✅ |
| | 码率控制（CRF / CQP / ABR）、分辨率、帧率、音频配置 | ✅ | ✅ |
| | 编码输出体积实时预估 | ✅ | ✅ |
| **硬件加速** | NVENC / QSV / AMF / VideoToolbox / VAAPI 自动检测与 GPU 加速 | ✅ | ✅ |
| **队列调度** | SQLite 持久化队列、失败重试、自动推进 | ✅ | ✅ |
| | 最大并发任务数设置（1 ~ 16 并发） | ✅ 自由配置 (1~16) | ✅ 自由配置 (1~16) |
| **预设管理** | 18 款内置预设切换 + 自定义预设创建/修改/删除 | ✅ | ✅ |
| | 预设 JSON 导入与导出 | ✅ | ✅ |
| **FFmpeg 命令** | 实时预览 FFmpeg 命令并**复制到剪贴板** | ✅ | ✅ |
| | 将 FFmpeg 命令直接**导出为脚本文件**（`.bat` / `.sh` 等） | ❌ | ✅ |
| **画质评估** | VMAF 客观质量评分对比（全量 / 采样对比分析） | ❌ | ✅ |

## 开发

```bash
pnpm install          # 安装依赖
pnpm tauri dev        # 开发模式（vite :1430）
pnpm tauri build      # 生产构建
pnpm test             # 前端测试（vitest）
pnpm lint             # eslint
cd src-tauri && cargo test   # Rust 单元测试
```

要求：Node 22+、pnpm 10+、Rust stable（含各平台 WebView 依赖，见 [Tauri 文档](https://tauri.app/start/prerequisites/)）。

## 发布

推送 `v*` tag 触发两条流水线：`ci.yml` 跑类型检查、eslint、vitest、cargo test 质量门；`release-tauri.yml` 并行构建 NSIS 安装包（不等待质量门），发布 GitHub Release 到公开产物仓库 `superMC5657/z-ffmpeg-release`（匿名下载 + gh-proxy 加速），产物含 updater 签名文件与国内镜像 `latest-cn.json`。版本号用 `pnpm bump` 同步。

## 许可

见 [LICENSE](LICENSE)。
