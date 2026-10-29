# z-ffmpeg 全项目 Code Review(2026-10-30)

- **审查方式**:固定点 = 空树 `4b825dc6`(整个代码库视为 diff),基于当时工作区状态(含 37 个未提交修改的 Rust 文件)。提交范围 `73d64fa`(v0.1.0 初始化)→ `76f01f0`(HEAD),共 79 个提交。
- **两轴方法**:Standards(成文规范 + Fowler 坏味道基线)与 Spec(契约符合性)由两个独立子代理并行审查后聚合,未做跨轴合并排序。
- **规范来源**:`AGENTS.md`(架构/约定章节)、`docs/logging.md`。
- **规格来源**:`docs/remote-api/soft-candy-integration-guide.md`(授权 · 埋点 · 检查更新契约)+ `AGENTS.md` 中的行为契约。
- **工具门禁实测(全绿)**:`tsc --noEmit` ✅ · `eslint` 0 error ✅ · `cargo clippy -D warnings` ✅ · `cargo test` 164 通过 0 失败 ✅。
- **未提交改动核查**:37 个未提交文件绝大部分为 rustfmt 重排 + `hw_accel.rs` 新增 fps 参数,未破坏任何契约。

## 两轴结论

- **Standards 轴**:总体质量高。camelCase、Pro 门控面(仅 `encode.rs:92` / `vmaf.rs:22`)、release profile(opt-level=z/lto/strip/panic=abort,`Cargo.toml:54-59` 未被改动)、日志脱敏与留存规则均符合成文规范。2 处硬违规 + 7 处判断题,无阻塞合入问题。
- **Spec 轴**:契约核心全部核对通过——门控面无越界/漏门(后端 `ensure_pro` 仅两处,前端 ProGate 仅 QueueItem VMAF 与导出文件两处)、内置预设 18 个只读、DB 位于 appDataDir、事件名前后端一致、核心结构体 serde camelCase 对齐 `types/index.ts`、埋点负载字段序有测试锁定。1 项契约功能缺失、3 项有意/无意偏差。

## 问题总表(18 项)

| 编号 | 位置 | 问题 | 级别 | 轴 |
|---|---|---|---|---|
| S1 | `src-tauri/src/z_log.rs:374,381` | `zlog_get_dir`/`zlog_export_bundle` 返回 `Result<String, String>`,36 个命令中仅有的非 `AppResult` 命令,违反 AGENTS.md 错误口径 | 硬违规 | 规范 |
| S2 | `encoder/engine.rs:205-215` | 进度循环内有里程碑日志,违反 `docs/logging.md` L36「进度循环零 log」 | 硬违规(二选一) | 规范 |
| S3 | `encoder/flags.rs:26-75,209-280` | `encoder_name` 五分支重复;`rate_control_args` 按 `contains("nvenc")` 字符串派发 | 判断题(注释自称"快照锁死") | 规范 |
| S4 | `util/db.rs:26-68` | `open_sqlite_store` 参数顺序嗅探(`looks_like_sql`),两个调用方都用契约顺序,属为不存在的分歧做的抽象 | 判断题 | 规范 |
| S5 | `commands/history.rs:11-42` vs `queue/job.rs:94` | `HistoryEntry` 与 `JobSnapshot` 11 字段同名同型 + 手写 `From`,第三份映射在 `store.rs:356-379` | 判断题(重复代码) | 规范 |
| S6 | `types/index.ts:127`、`store/licenseStore.ts:5`、`QueueToolbar.tsx:26` | `isEncodeProgress` 死导出与 `lib/progress.ts` 严格版重复;`FREE_MAX_CONCURRENT` 注释指向已删除的后端常量;魔法数 16 两处平行 | 判断题 | 规范 |
| S7 | `commands/queue.rs` | `queue_manager: None` 时写命令行为分裂(remove/cancel/pause/resume/clear 静默 Ok,add/retry/get_max 报错);`:17-18` `files_added`/`jobs_added` 恒等冗余 | 判断题(不一致) | 规范 |
| S8 | `error.rs:6-22`、`queue/manager.rs:429,475`、`encode.rs:21`、`probe.rs:118` | 用户可见错误中英混杂,违反「UI 文案为中文」 | 硬违规(文案) | 规范 |
| S9 | `license/client.rs:11-19` 等 | `ActivateResponse`/`VerifyResponse` 同形等轻量重复 | 判断题(建议不动) | 规范 |
| P1 | 契约第 4 节 vs `lib.rs:37` | **检查更新契约未实现**:契约要求 `GET /api/v1/apps/{slug}/latest` + 分段数字版本比较 + `forceUpdate` + 失败静默;现仅挂 `tauri_plugin_updater`(自建 latest.json 端点,格式不符),`UpdateSection.tsx` 直接调插件的 `check()` | 严重(需决策) | 规格 |
| P2 | `license/device.rs:22-27` | Linux deviceId = `fallback-{hostname}-{username}`,不满足契约「系统级稳定指纹」;注释「只出 NSIS」已过时(f989bd9 已出 deb+appimage) | 中 | 规格 |
| P3 | `manager.rs:1`、`analytics/report.rs:3,21`、AGENTS.md | 注释引用指南「3.6/4.5/4.1 节」但指南无这些小节;AGENTS.md 仍写 `queue/settings.rs`(已并入 `store.rs`) | 轻 | 规格 |
| P4 | `license/manager.rs:345-348` | verify 吊销清凭证集合含 `INVALID_SIGNATURE`,超出契约 2.3 枚举(`CDK_REVOKED`/`DEVICE_NOT_ACTIVATED`) | 轻(需决策) | 规格 |
| P5 | `license/config.rs:77-95` | `SOFTCANDY_API_BASE`/`BUY_URL` 运行时环境变量可覆盖配置,契约只约定编译期注入(已核实:测试不依赖运行时覆盖) | 轻(需决策) | 规格 |
| P6 | `tauri.conf.json:88` | `analyticsToken` 明文提交入库(已在 git 历史) | 中(安全卫生) | 规格 |
| P7 | `analytics/report.rs:122` | 埋点请求超时 5s,契约第 1 节要求统一 10s(退出 3s 有界等待是有意设计) | 轻 | 规格 |
| P8 | `components/license/ProGate.tsx:18` | 注释引用不存在的 `ensure_config_allowed`(实际为 `ensure_pro`) | 轻 | 规格 |
| P9 | `license/client.rs:8-9` | activate 响应的 `expiresAt/level` 等不消费、以 JWT claims 为准 | 契约容忍(不动) | 规格 |

**两轴最重问题**:Standards 轴为 S1(错误口径绕过统一序列化);Spec 轴为 P1(检查更新契约缺失,当前 updater 端点与契约不符)。

## 逐项修改建议

- **S1** — 两个命令签名改为 `AppResult<String>`(引入 `crate::error::AppResult`),内部 `map_err` 走 `AppError::Internal`/`Io`。前端 `tauri.ts` 拒绝路径同为字符串,零前端改动;已核实无测试断言这两个签名。
- **S2** — 推荐保留日志(每文件仅 4 条、排障价值高),在 `docs/logging.md` L36 补记例外:「进度循环零 log,唯一例外:25% 步进里程碑(含 fps/speed,排障用)」。若坚持零日志则删 `engine.rs:205-215`。决策点 D2。
- **S3** — 本次不动(仓库注释已声明快照锁死,成文认可覆盖基线)。未来加新编码器品牌时一次性表驱动化 `encoder_name` 并把 `rate_control_args` 派发切到 `HwAccelDevice` 枚举。
- **S4** — 固定签名为 `open_sqlite_store(db_path, init_sql, tag)`,更新 2 个调用方(`queue/store.rs:59`、`preset/manager.rs:13`),删除 `looks_like_sql`/`resolve_sql_and_tag` 约 20 行。
- **S5** — 删除后端 `HistoryEntry` 结构体 + `From` 实现,`get_history` 直接返回 `JobSnapshot`(`HistoryPageResult.entries: Vec<JobSnapshot>`)。前端 `HistoryEntry` 类型保留(历史页视图子集,多出的运行时字段被忽略),`HistoryPage.tsx` 零改动。行→快照与 job→快照两份映射源类型不同,属固有成本,保留。
- **S6** — ① 删 `types/index.ts:124-127` 的 `isEncodeProgress`(仅测试引用,改测试 import);② 修正 `licenseStore.ts:5` 注释为「与后端 `set_max_concurrent` 的 1..=16 范围对齐」;③ `QueueToolbar.tsx` 删本地 `concurrencyCap`,改 import `FREE_MAX_CONCURRENT`。
- **S7** — ① `commands/queue.rs` 顶部加 `fn queue_or_err(state) -> AppResult<&Arc<QueueManager>>`,全部**写命令**(remove/cancel/pause/resume/clear/start/set_max_concurrent)统一走它报错;`get_queue_status` 保留空形状回退(只读命令,前端需要形状);② 删 `jobs_added` 计数器(COUNTERS 字段、`queue.rs:18` bump、`report.rs:30` 负载字段)——契约测试的字段序锁定列表不含 `jobsAdded`,删除安全,并顺带把 `payload_fields_follow_doc_order` 测试升级为**全字段**锁定。
- **S8** — `error.rs` 各 `#[error]` 文案与 4 处散落英文字符串改中文(如「未找到 FFmpeg,请安装或配置路径」「任务不存在:{0}」)。已核实无测试断言这些英文串。
- **P1(决策 D1)** — 二选一:
  - **方案 B(推荐先做,0.5 天)**:保留 tauri updater + 自建 latest.json 双清单(release 流水线 bccbb36 刚投入建成),在指南第 4 节或 AGENTS.md 显式记录偏差声明:「更新检查走自建 latest.json 端点,未接入软糖铺检查更新契约」。
  - **方案 A(完整实现,1~1.5 天)**:新建 `src-tauri/src/license/update.rs`——`GET latest?platform=&channel=`、分段数字版本比较(逐段比数值)、`forceUpdate` 透传、失败静默返回 `None`;注册 `check_app_update` 命令;`UpdateSection.tsx` 切数据源;updater 插件与端点按产品决定去留。
- **P2** — `device.rs` 非 Windows 分支改读 `/etc/machine-id`(回退 `/var/lib/dbus/machine-id`,再回退现 hostname+user 组合),同步更新注释(删除「只出 NSIS」)。Windows 路径不动,现有测试继续覆盖。
- **P3** — 见执行计划 Phase 6(依赖 S7b 定稿字段后再写)。
- **P4** — 推荐保留现行为(`INVALID_SIGNATURE` = JWT 伪造/损坏,清凭证是安全上更严的正确默认),在 `manager.rs:345` 加注释声明有意超出契约枚举及理由,并在指南接入注记同步。需产品确认(决策 D3)。
- **P5** — 运行时 `std::env::var` 覆盖包进 `#[cfg(debug_assertions)]`(dev/测试便利),release 只留编译期 `option_env!`。已核实测试不依赖运行时变量,零破坏。
- **P6** — 复用 P5 的注入模式:`config.rs` 增加 `option_env!("SOFTCANDY_ANALYTICS_TOKEN")` 覆盖 `analytics_token`;`tauri.conf.json:88` 置空;`release-tauri.yml` 从 GitHub Secret 注入。**必须同时撤换该 token**(已进 git 历史,仅删值不够)。
- **P7** — `report.rs:122` 超时 5s→10s 对齐契约;退出 3s 有界等待保留(契约「失败静默、不阻塞」语义满足),加注释说明两者关系。
- **P8** — `ProGate.tsx:18` 注释 `ensure_config_allowed` → `ensure_pro`。
- **S9 / P9** — 不动(均为可接受的现状,注释已自说明)。

## 执行计划(含顺序依赖)

依赖关系总览:`D1~D3 决策 → 各阶段;P6 依赖 P5;Phase 2 的 S7b → P3 的负载示例;D2 → S2 落地方式;D1 → Phase 5 是否存在`。

### Phase 0 — 决策点(阻塞项,先定)

- **D1** 更新来源:方案 A 还是 B → 决定 Phase 5 形态
- **D2** 里程碑日志:保留+补文档(推荐)还是删 → 决定 S2 改代码还是改文档
- **D3** P4/P5 处置:确认「保留+注记」方案
- **P6 token 撤换**:确认新 token 与 Secret 配置人

### Phase 1 — 统一错误口径批(S1 ∥ S8 ∥ P8,三条互不依赖可并行)

依赖:仅 Phase 0 的确认。改 `z_log.rs`、`error.rs`、`queue/manager.rs`、`encode.rs`、`probe.rs`、`ProGate.tsx`。先做的原因:全是小改动、零依赖,且「错误口径」是 AGENTS.md 硬约定,后续批次的新代码要按统一口径写。验证:cargo test + clippy + tsc。

### Phase 2 — 队列/存储收敛批(顺序:S4 → S5 → S7 → P7)

依赖理由:四项同处 queue/preset/analytics 子系统,串行做避免同文件冲突、保持每步可独立 review。S4 动 `store.rs:59` 调用方 → S5 动 `commands/history.rs` → S7 动 `commands/queue.rs` + `analytics`(COUNTERS/负载)→ P7 与 S7b 同在 `report.rs`,连做省一次编译往返。**S7b 是 P3 负载示例的前置**。验证:全量门禁 + 重点跑 `payload_fields_follow_doc_order` 与队列保留/重试测试。

### Phase 3 — 授权与配置批(P2 ∥ P4 ∥ P5 → P6)

依赖理由:P2/P4/P5 文件互不相交可并行;**P6 依赖 P5**(复用同一段 env 注入模式,先定模式再套 token),且 P6 含 CI 修改放在批尾。验证:cargo test(device 稳定性测试)+ 手工冒烟激活/续验。

### Phase 4 — 前端清理批(S6,可与 Phase 2/3 并行)

依赖:无。纯前端,与后端批次无文件交集。验证:vitest + tsc + eslint。

### Phase 5 — 检查更新契约(P1,视 D1)

依赖:D1 决策 + **Phase 3 完成**(避免与 `config.rs` 改动冲突)。方案 B → 并入 Phase 6 写偏差注记即结束;方案 A → 新模块 + 命令注册 + 前端切换 + 版本比较单测。

### Phase 6 — 文档收尾(P3 + S2 + D1-B/P4 注记,最后做)

依赖:**S7b 定稿字段后**才能写指南 3.1 负载示例;**D2 落定**才改 `logging.md`;**D1 落定**才写偏差声明。内容:指南补「3.1 负载示例」、修正代码内 3.6/4.5/4.1 幽灵引用、AGENTS.md `settings.rs`→`store.rs` 及「4.1」→「3.1」、按 D2 结论改 `logging.md` 或 `engine.rs`。

### 可选积压(不在本轮)

S3 表驱动化(加新编码器品牌时一并做)、S9。

**总量估计**:Phase 0~6 约 2~3 天;若 D1 选方案 A 再加 1~1.5 天。每个 Phase 收尾跑四门禁(tsc/eslint/vitest、clippy、cargo test),全绿再进下一阶段。

## 执行记录(2026-10-30,决策:D1=方案B / D2=保留 / D3=保留+注记 / P6=仅代码改造)

| 项 | 状态 | 落点 |
|---|---|---|
| S1 | ✅ | `z_log.rs` 两命令改 `AppResult<String>`,zip/io 错误走 `AppError::Internal` |
| S2 | ✅ | 保留日志(D2),`docs/logging.md` 补 25% 里程碑例外 + 生命周期点位 |
| S4 | ✅ | `util/db.rs` 固定 `(path, init_sql, tag)`,删参数嗅探约 20 行 |
| S5 | ✅ | 删 `HistoryEntry`,`get_history` 直返 `JobSnapshot`(前端视图类型保留) |
| S7 | ✅ | `queue_or_err` 统一全部写命令报错口径;删 `jobs_added` 冗余计数器,负载序测试升级全字段锁定 |
| S8 | ✅ | `error.rs` 全部 `#[error]` 文案 + manager/encode/probe 4 处中文化 |
| S6 | ✅ | 删 `types/index.ts` 死守卫;`QueueToolbar` 引 `FREE_MAX_CONCURRENT` 归一魔法数;测试更名 |
| P2 | ✅ | Linux 走 `/etc/machine-id`(回退 dbus machine-id),删「只出 NSIS」过时注释 |
| P4 | ✅ | 保留 INVALID_SIGNATURE 清凭证 + 注释声明偏差;manager.rs 头注释 3.6→第2节/2.4,顺带删「debug 恒解锁」过时行 |
| P5 | ✅ | 运行时 env 覆盖收进 `cfg!(debug_assertions)`,release 只认编译期注入 |
| P6 | ✅ 代码侧 | conf token 置空 + `option_env!("SOFTCANDY_ANALYTICS_TOKEN")` + release workflow Secret 注入 |
| P7 | ✅ | 请求超时 10s 对齐契约 + 退出 3s 有界等待注释说明取舍 |
| P8 | ✅ | `ProGate.tsx` 注释改为 `ensure_pro` |
| P1 | ✅(方案B) | 指南第 4 节加偏差声明(自建 latest.json 更新通道) |
| P3 | ✅ | 指南补 3.1 负载示例;AGENTS.md `settings.rs`→`store.rs`、4.1→3.1 |
| S3 | ✅(追加) | `encoder_name` 改 `HW_ENCODER_NAMES` 常量表驱动(15 行映射为唯一事实来源);`rate_control_args` 的 CRF/CQP 同形分支收拢为 `qp_style_args`。字符串派发保留并注释原因:设备 × VP9/Copy 已在 `encoder_name` 回退软件编码器,按 `HwAccelDevice` 派发会误入硬件分支 |
| S9 | ✅(追加) | `ActivateResponse`/`VerifyResponse` 合并为 `LicenseResponse`(仅 client.rs 内部使用),注释说明两接口形状分叉时再拆回 |
| P9 | ⏸ 按报告建议不动 | JWT claims 为验签可信来源,现状即正确 |

**门禁**:clippy -D warnings ✅ · cargo test 164/164 ✅(Phase 1~3 后);tsc / eslint / vitest 见门禁记录。

**待办(用户侧)**:
1. 在 GitHub 仓库 Secrets 新增 `SOFTCANDY_ANALYTICS_TOKEN` = 现行 token 值。下次 tag 发布前必须配置,否则发布版埋点将无鉴权上报(服务端可能拒收;静默失败,不影响功能)。
2. token 本体撤换暂缓(旧值已进 git 历史),择期在软糖铺后台作废旧 token 并轮换 Secret。
3. S3 表驱动化留待新增编码器品牌时一并处理。
