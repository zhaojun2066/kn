# 设计文档深度审查：`2026-06-16-remote-control-design.md`

> 审查日期: 2026-06-17 | 审查方法: 交叉对照实际代码库

## 审查范围

| 材料 | 路径 | 规模 |
|------|------|------|
| 设计文档 | `docs/superpowers/specs/2026-06-16-remote-control-design.md` | 2025 行 |
| 已知问题清单 | `docs/superpowers/specs/2026-06-16-remote-control-design-issues.md` | 324 行 (27 项全部标记 ✅ 已修复) |
| 前次审查 | `.claude/plans/2026-06-16-remote-control-design-md-revi-spicy-piglet.md` | 254 行 |
| **实施计划 × 8** | `docs/superpowers/plans/2026-06-16-*.md` | ~200KB |
| 实际 PTY 代码 | `desktop/src-tauri/src/pty.rs` | 336 行 |
| 实际 Shell Wrapper | `shell/ai-profile.sh` | 419 行 |
| 实际依赖配置 | `desktop/src-tauri/Cargo.toml` | 35 行 |
| Desktop 架构 | `desktop/CLAUDE.md` | 全文 |

## 审查结论

前次审查发现的 27 个问题已在设计文档中正确修复。本次深度审查以实际代码为基准进行交叉验证，新发现 **35 个问题**，其中 **5 个阻塞性**、**12 个高优先级**。

---

## 一、阻塞性问题 (BLOCKER) — 不解决无法开始编码

### ~~B-1: Agent 二进制无处可放——缺少 Cargo workspace~~ ✅ 已决策

**Plan 原方案**：在 `desktop/src-tauri/Cargo.toml` 中加 `[[bin]]`，Agent 代码放 `src/agent/`，通过 `crate::` 引用同 crate 模块。此方案可行，`[[bin]]` 与 Tauri lib 共存。

**审查意见**：`[[bin]]` 方案存在代价——Agent 二进制链接 Tauri 依赖（体积增大）、`reqwest` feature 冲突（见 P1-2）、未来 Agent 独立发布需反向拆 workspace。

**决策**：**现在就建 workspace**。创建 repo 根 `Cargo.toml` [workspace]，将公共代码 (`commands.rs`, `profile_cmd.rs`, `fingerprint.rs`, `PtyOutputSink` trait) 提取到 `common/` crate。`agent/` 和 `desktop/src-tauri/` 各自依赖 `kn-common`。
- 好处：依赖隔离、独立测试、后续重构零成本
- 代价：约 3 小时一次性工作量
- 升级/启动/发布：无影响（构建命令从 `cd desktop && cargo build` 改为 repo 根 `cargo build`）

**设计文档 §3.2.2、§3.2.4 已更新**为 workspace 方案。**Agent Phase 1 Task 1 已重写**。

### ~~B-2: Desktop-Agent 间 IPC 协议完全未定义~~ ✅ 已修复

**决策**：新增 §3.2.9 IPC 协议定义，包含：
- Socket 路径 `~/.kn/agent/ipc.sock`，权限 `0600`
- JSON-line 帧协议，请求-响应模式
- 完整消息类型表：14 条 Desktop→Agent request + 7 条 Agent→Desktop push/response
- 流式输出语义：`attach` 订阅 → 持续推送 `output` → `detach` 取消
- 心跳保活：Agent 每 60s 发 `heartbeat`，Desktop 侧 120s 超时判定离线
- 重连策略：Desktop 每 2s 重试
- 7 种错误码枚举
- 与 WSS 协议的关系说明

### ~~B-3: `_ai_direct` 回退函数在代码中不存在~~ ✅ 已修复

**决策**：§3.2.8 已重写，明确 `_ai_direct` 的定义方式——将现有 `ai()` 函数重命名，保留全部原有逻辑（profile 选择链、Claude/Codex workaround、fzf 交互等），零改动。新的 `ai()` 包装函数先尝试 Agent IPC 路由，失败则回退 `_ai_direct "$@"`。

### ~~B-4: `kn agent --new` CLI 子命令不存在~~ ✅ 已修复

**决策**：§3.2.5 新增"CLI 子命令"小节，定义完整的 `kn agent` 命令族：
- `kn agent status|bind|sessions|--new|attach|kill|reset-crash-count`
- 实现方式：`kn-agent` 二进制自身支持 CLI 模式——带参数时连接 Agent IPC (`~/.kn/agent/ipc.sock`)，发送请求后返回
- Daemon 模式和 CLI 模式共用同一二进制，通过参数自动区分
- `bin/profile` 保持不变，只做 profile CRUD
- Phase 2 与 IPC Server 一同实现

### ~~B-5: Claude `--settings` / Codex `auth.json` workaround 在 Rust 侧无实现~~ ✅ 已修复

**决策**：§3.2.4 新增"CLI Tool 启动前的预处理"表，明确每种 tool 的 Agent Rust 实现方式：
- **Claude**：Agent spawn 前生成 temp JSON → PTY 命令追加 `--settings` → EOF 后清理
- **Codex**：Agent spawn 前 backup → write auth.json → EOF 后 restore
- **qoderclicn**：无需额外处理，注入 env vars 直接 spawn
- 封装在 `session.rs` 的 PTY spawn 流程中，`profile_cmd.rs` 只负责读 env vars
- `.ai-profile` 文件遍历由 shell 层在 `_ai_direct` 中保留（B-3 已决定 profile 选择不回退到 Rust），不影响 Agent

---

## 二、高优先级问题 (HIGH)

### ~~H-1: 约 12 个消息类型在正文中引用但未列入 §4.3 协议表~~ ✅ 已修复

**决策**：§4.3 消息类型表已更新（inbound 11→12 条，outbound 11→16 条），新增 WSS 消息类型：
- inbound: `resize_pty`, `lock_session`, `write_file`, `read_output_log`
- outbound: `state_change`, `profile_update`, `agent_error`, `current_state`, `missed_messages`
- IPC 专属消息 (`agent_upgrading`, `drain_status`) 已在 §3.2.9 定义，不再重复列入 §4.3
- 表头加了说明，区分 WSS 和 IPC 消息类型

### ~~H-2: APNs device token 无存储位置~~ ✅ 已修复

**决策**：§3.1.3 新增 `kn_push_token` 表（user_id + device_token + is_active），一个用户可有多台 iOS 设备。§3.1.4 新增 Redis key `push:token:{user_id}` (SET) 用于推送时快速查找。token 生命周期：iOS App 注册时写入/UPSERT，APNs 返回 410 时标记 is_active=false。

### ~~H-3: device_token 在 WebSocket URL query string 中裸奔~~ ✅ 已修复

**决策**：§4.1 改为 HTTP `Authorization` header 鉴权（Bearer token）。三个客户端一致：Agent 用 `Authorization: Bearer <device_token>` + `X-KN-Machine-Id`，iOS 用 `Authorization: Bearer <access_token>` + `X-KN-Role: ios`。Nginx 默认不记录 HTTP header，token 不泄露。计划文件同步：agent-phase1 ws_client、cloud-phase1 WsHandler、ios-phase1 WebSocketClient 均已改用 header 鉴权。

### ~~H-4: 消息协议无版本号字段~~ ✅ 已修复

**决策**：§4.2 新增"消息格式与版本协商"，`connected` 消息携带 `protocol_version`。客户端检查：服务端版本高于已方最高版本→断开提示升级。版本号规则：新增消息类型不变，修改/删除已有类型 +1。当前 protocol_version = 1。计划文件同步：agent-phase1 Task 5 加版本检查，cloud-phase1 WsHandler 发送 `protocol_version:1`。

### ~~H-5: crash_count 机制未列入任何实施 Phase~~ ✅ 审查遗漏，实际已覆盖

Agent Phase 1 Task 3（state.rs）已实现 StateMachine 的 crash_count 原子计数 + `in_safe_mode()` 判断。Task 6.5 专门实现了 crash_count 文件持久化（启动时 +1 写入磁盘 → 60s 正常运行后归零）。设计文档 §3.2.5 的 crash 退避算法已完整覆盖，无需额外补充。

### ~~H-6: `session_interrupted` 状态不在 `kn_session.status` 枚举中~~ ✅ 已修复

**决策**：不改 DB 枚举。`session_interrupted` 和 `failed` 是同一个事件的不同用途——DB 用 `failed` 记录终态，WSS 消息用 `session_interrupted` 带恢复上下文（last_input/cwd/tool/profile）帮用户重试。§3.1.3 已补充注释说明两者关系。

### ~~H-7: `kn_message` 表缺少 `src` 字段~~ ✅ 已修复

**决策**：`kn_message` 表新增 `src VARCHAR(10) NOT NULL DEFAULT 'local'` 列（取值: `ios` / `local` / `desktop`）。§3.1.3 DDL 已更新并补充注释。cloud-phase1 init.sql 同步更新。

### ~~H-8: CI/CD pipeline + DB 迁移脚本零覆盖~~ ✅ 已修复

**决策**：设计文档 Phase 1 新增 CI/CD + systemd 部署任务。cloud-phase1 新增 Task 7：GitHub Actions workflow（push → Maven build → SCP jar → SSH restart systemd）、systemd service 文件（两个独立进程）、DB schema 迁移说明（v1 手动 SQL + CHANGELOG）。

### ~~H-9: 多 WS gateway 实例路由机制未定义~~ ✅ 已修复

**决策**：保留 `ws_node_id` 设计，新增 Redis Pub/Sub 中继机制。每个 Gateway 实例启动时生成唯一 ID，订阅自己的 `ws:relay:{gateway_id}` channel。消息路由流程：同实例直接投递，跨实例通过 Pub/Sub 中继到目标 Gateway。单实例时发布到自身 channel，零额外开销。§3.1.1 补充多实例路由说明，§3.1.4 新增 relay channel，cloud-phase1 WsHandler 实现完整中继逻辑。

### ~~H-10: Agent 二进制嵌入 .app bundle 的构建链不完整~~ ✅ 已修复

**决策**：§3.2.7 补充完整构建链：tauri.conf.json `bundle.resources` 配置、三步构建流程（cargo build → cp → tauri build）、代码签名（同一 Developer ID 证书签所有 Mach-O，`codesign -dvvv` 验证）、公证（一张票据覆盖 bundle 内所有可执行文件，Agent 不需单独公证）。§3.2.6 发布打包路径更新为 workspace 路径。

### ~~H-11: config.yaml 并发访问 — 用了错误的锁~~ ✅ 已修复

**决策**：设计文档 §3.2.4 已明确 `with_write_lock_exclusive()` + `fs2::lock_exclusive()` 跨进程文件锁 + `spawn_blocking` 包装。Agent Phase 1 Task 1 Step 3 补充 config.yaml 跨进程写安全说明，common crate 中 `with_cross_process_lock` 函数被 Desktop 和 Agent 复用，保证同一锁文件。

### ~~H-12: 无测试策略~~ ✅ 已修复

**决策**：Phase 1-4 各阶段新增测试任务——P1: Agent 单元测试 + 云服务单元/集成测试 (JUnit 5 + Testcontainers)；P2: IPC 集成测试 + WSS 协议集成测试；P3: iOS UI 测试 (XCUITest)；P4: 端到端测试 + 异常恢复场景测试 + 性能测试。

---

## 三、中优先级问题 (MEDIUM)

### ~~M-1: Session ID 格式不明确且存在矛盾~~ ✅ 已修复

**决策**：`session_id` 格式统一为 `s_` + 12 位 nanoid（url-safe base62, 62^12≈3.2×10^21），云端生成，全局唯一。§4.2/§3.1.3/§9.2 全部更新。Agent Phase 2 Task 9 本地临时 ID 改为 `s_{random_u64:x}`。

### ~~M-2: iOS 架构缺多个关键模块~~ ✅ 已修复

**决策**：§3.3.1 新增最低 iOS 版本 (17.0)、JavaScript 桥接设计（WKUserContentController + 两个 handler）、键盘避让（`ignoresSafeArea(.keyboard)` + `FitAddon.fit()`）。§7 新增 xterm.js addons 配置（FitAddon、WebLinksAddon、Unicode11Addon）。步骤文件 iOS Phase 1 已覆盖这些实现。

### ~~M-3: Desktop 📡 按钮初始状态存在空白窗口~~ ✅ 已修复

**决策**：初始状态改为"灰点闪烁"（连接中）。5 秒内 IPC 响应 → 切实际状态；5 秒超时 → 切灰色。§3.4.2 图标表新增触发条件列，§3.4.3 流转图补初始状态分支。

### ~~M-4: Agent 升级 vs 窗口显示的时序~~ ✅ 已修复

**决策**：§3.4.1 明确 Agent 版本检查和升级在 Rust `lib.rs` `setup()` 阶段（窗口显示前）完成。Agent Phase 3 Task 16 新增 Step 0.5 实现安装/版本比较/原子替换/重启的完整 Rust setup 流程。

### ~~M-5: PTY spawn 失败不在异常矩阵~~ ✅ 已修复

**决策**：§10.1 新增 4 条异常：PTY 分配失败 (`pty_alloc_failed`)、Shell 启动失败 (`shell_spawn_failed`)、AI CLI 未找到 (`cli_not_found`)、config 损坏 (`config_parse_error`)——每条含检测方式、恢复策略、用户体验。Agent Phase 2 Task 15 补充 PTY spawn 错误路径注释。

`session:pending` Redis key 的 TTL 会超时清理，但用户得到的反馈只有超时——不知道具体原因。

### ~~M-6: WSS 消息格式错误处理未规定~~ ✅ 已修复

**决策**：§4.2 新增"消息格式错误处理"表（5 种场景）。cloud-phase1 WsHandler 补全：非 JSON→close(1003)、缺失 type→error_notify、session_not_found→error_notify、session_already_ended→error_notify、二进制→close(1003)。

### ~~M-7: SessionRecord 移除影响 5 个组件~~ ✅ 已修复

**结论**：SessionRecord 不删。它是终端面板的标签页历史（localStorage），和 Agent 管的 AI 会话输出是两个维度，不冲突。设计文档已修正措辞：实时 PTY 读写走 Agent IPC，标签页历史继续 localStorage。`useTerminal.ts` 改动量从"全改"降为"只改 PTY Channel 部分"。

### ~~M-8: "降级策略" 自相矛盾~~ ✅ 已修复

**决策**：§3.4.5 标题改为"Agent 是 AI 会话的统一管理入口"。Desktop 保留完整 `pty.rs` 直接 PTY 能力。Agent 在线走 IPC，Agent 离线走本地 PTY——两条路径不冲突，代码两套都保留。

### ~~M-9: `kn_message` 90 天保留无实现机制~~ ✅ 已修复

**决策**：不加重软删除列，不加分区表。v1 用 `@Scheduled` 定时任务每天凌晨 3 点 `DELETE WHERE created_at < NOW() - 90 DAY`，走已有索引。cloud-phase2 新增 Task 9.5 实现。

### ~~M-10: Input 超频率 → 丢弃却 ack~~ ✅ 已修复

**决策**：丢弃时发 ack `{msg_seq, dropped: true}`，客户端看到立即重发。ack 本身不受限流。限流维度明确为 per WSS 连接，本地 `ConcurrentHashMap` 实现。cloud-phase1 WsHandler 补全限流逻辑和 `incrAndCheck` 辅助方法。

### ~~M-11: 卡密生成→销售→激活流程未闭环~~ ✅ 已修复

**决策**：§3.1.2 新增"卡密全生命周期"三阶段（生成→销售→验证）。生成由 kn 自控 Java 工具输出 INSERT SQL 手动导入，销售由第三方平台纯分销不参与验证，验证由 kn 云端自控查 `kn_redeem_code` 表。步骤文件 Task 11 已有 `GenerateCodes.java`。

---

## 四、低优先级问题 (LOW)

| # | 问题 | 位置 |
|---|------|------|
| ~~L-1~~ | ~~`msg:dedup` TTL 10min~~ → 5min（幂等去重窗口） ✅ | — |
| ~~L-2~~ | `kn_message.direction` + `msg_type` 加 CHECK 约束 ✅ | — |
| ~~L-3~~ | API Key 加密存储：AES-256-GCM + macOS Keychain，§6.5 ✅ | — |
| ~~L-4~~ | PtyOutputSink 已有 on_ready/on_exit/on_error（B-1 时一并修复） ✅ | — |
| ~~L-5~~ | Agent 日志每日翻滚 + 7 天保留 (tracing-appender) ✅ | — |
| ~~L-6~~ | 休眠恢复：pong 超时→90s、failed 判定→30min、重连后 kill(pid,0) 检查存活 ✅ | — |
| ~~L-7~~ | APNs p8 key revoke: 403 降级 + 日志告警，核心功能不受影响 ✅ | — |

---

## 五、实施路线图补充任务

### Phase 1 补充 (基础设施)

- [ ] **工作区搭建**: 创建根 `Cargo.toml` workspace + Agent binary crate + `kn-common` 共享库
- [ ] **新依赖**: tokio, tokio-tungstenite, machine_id, futures-util, url
- [ ] **DB DDL SQL**: 5 张基础表 + 1 张 push_token 表 + DDL 脚本
- [ ] **Redis 初始化**: 验证 key schema / TTL / maxmemory-policy 配置
- [ ] **CI/CD**: GitHub Actions workflow for kn-cloud (build → test → docker build → push)
- [ ] **本地开发环境**: Docker Compose (MySQL + Redis + API + WS)
- [ ] **测试框架**: JUnit 5 + Testcontainers (Java), vitest (前端)
- [ ] **错误码枚举**: 定义所有 `error_notify.code` 值

### Phase 2 补充 (核心功能)

- [ ] **crash_count 机制**: crash_count 文件读写 + safe_mode + 60s 稳定窗口 + reset 命令
- [ ] **消息序列号**: seq 单调递增 + msg:dedup 去重 + ack 确认 + last_ack_seq 重连恢复
- [ ] **Session checkpoint**: 每 30s 原子写 checkpoint.json (含截断约束)
- [ ] **APNs 服务端**: Pushy 库集成 + push_token 表 + 推送触发逻辑
- [ ] **Claude `--settings` workaround**: Agent 内生成临时 settings JSON → 传参 → 清理
- [ ] **Codex `auth.json` swap**: Agent 内备份 → 覆盖 → 启动 → EOF → 恢复
- [ ] **`.ai-profile` 遍历**: 从 cwd 向上查找 `.ai-profile` 文件
- [ ] **Agent 侧消息限流**: start_session 10/min, input 20/s, ctrl 5/s
- [ ] **`_ai_direct` 回退函数**: 复制当前 `ai()` 逻辑为 fallback 路径
- [ ] **`kn agent` CLI 子命令**: Python 或 Rust 实现

### Phase 3 补充 (iOS UI)

- [ ] **JS bridge**: WKUserContentController + WKScriptMessageHandler
- [ ] **键盘避让**: ignoresSafeArea(.keyboard) + 键盘通知
- [ ] **iOS 最低版本**: 明确指定 (推荐 iOS 16+)
- [ ] **xterm.js addons**: fit + webLinks + search + unicode11
- [ ] **Background task**: BGTaskScheduler / beginBackgroundTask
- [ ] **Certificate pinning**: TrustKit 或 URLSession delegate 实现
- [ ] **APNs 注册**: AppDelegate 中注册 + 上传 token 到云端
- [ ] **App 资源**: icon / launch screen / asset catalog

### Phase 4 补充 (集成与打磨)

- [ ] **端到端集成测试**: iOS → Cloud → Agent → PTY → 输出回传
- [ ] **负载测试**: 并发 WSS 连接 / session 创建 / 消息吞吐
- [ ] **安全审计**: device_token 泄露面 / 输入注入 / 权限边界
- [ ] **API 文档**: OpenAPI/Swagger for REST endpoints
- [ ] **DB 备份脚本**: mysqldump + 定时任务
- [ ] **崩溃上报**: Sentry (Rust Agent) + Crashlytics (iOS)
- [ ] **Agent 版本上报**: WSS 连接时上报 agent_version，云端记录到 kn_device

---

## 六、实施计划审查 (8 个 plan 文件交叉验证)

### 6.1 实施计划总览

| Plan 文件 | Task 数 | 关键缺口 |
|-----------|---------|---------|
| `agent-phase1.md` (29KB) | 7.5 tasks | 非 workspace 方案增二进制体积; `reqwest` feature 冲突 |
| `agent-phase2.md` (24KB) | 8 tasks | ChannelSink 类型错误; P2/P3 任务归属矛盾 |
| `agent-phase3.md` (10KB) | 3 tasks | `Deno.connect` 在 Tauri WebView 不可用; tauri.conf.json 语法错 |
| `cloud-phase1.md` (38KB) | 6 tasks | 消息中继是空 stub; 缺 push_token 表; 拼写错误 |
| `cloud-phase2.md` (14KB) | 3 tasks | 到期不杀 WSS; APNs JWT 是 stub |
| `ios-phase1.md` (28KB) | 8 tasks | 重连 bug; ANSI 转义脆弱; bindDevice 方法缺失 |
| `ios-phase2.md` (11KB) | 3 tasks | push payload key 与云端不匹配 |
| `execution-guide.md` (9KB) | — | 执行顺序合理，但 skill 使用指南需要更新 |

### 6.2 Agent Phase 1 — 具体问题

#### P1-1: `[[bin]]` 非 workspace 方案导致二进制膨胀

**现状**: Task 1 将 `[[bin]] name = "kn-agent"` 直接加在 `desktop/src-tauri/Cargo.toml` 中，Agent 代码放 `src/agent/`。Agent 用 `crate::commands::find_binary` 引用同 crate 模块。

**问题**: Agent 二进制会链接所有 Tauri 依赖 (tauri, tauri-plugin-shell, tauri-plugin-dialog, tauri-plugin-fs, tauri-plugin-updater 等)，这些库 Agent 根本用不到。最终 `kn-agent` 二进制可能膨胀到几十 MB。

**缓解**: v1 可接受（功能优先），但应标注为已知技术债务——后续应提取 workspace 公共库。

#### P1-2: `reqwest` feature 冲突

**Plan**: Task 5 WSS client 用 `tokio-tungstenite`（正确），但 Task 12 Step 1 的 `bind-init` HTTP 调用用了 `reqwest::Client::new().post(...).send().await` ——这是 async reqwest API。

**当前 Cargo.toml**: `reqwest = { version = "0.12", features = ["blocking"] }` ——只开了 blocking feature，不支持 `.await`。

**修复**: 要么给 reqwest 加上非 blocking feature，要么 bind-init 改用 `reqwest::blocking::Client` 在 `tokio::task::spawn_blocking` 中调用。

#### P1-3: `PtyEvent::Data` 是 tuple variant 不是 struct variant

**Plan**: Agent P2 Task 8 的 `ChannelSink` 实现写:
```rust
self.channel.send(PtyEvent::Data { text: text.to_string() })
```

**实际代码** (`pty.rs:12`): `Data(String)` ——是 tuple variant，不是 struct variant。

**正确写法**: `PtyEvent::Data(text.to_string())`

#### P1-4: Task 13/14 归属 Phase 矛盾

**Agent P2** Task 13 实现 InputMerger + OutputFan-out，Task 14 实现 checkpoint 写入。但 P2 完成检查点写"尚未实现（Phase 3）: InputMerger + OutputFan-out, Session checkpoint"。Task 自己说实现了，checklist 说没实现——矛盾。实际应在 P2 完成。

#### P1-5: `find_binary` 返回类型问题

**Plan**: Agent P2 Task 15 调 `find_binary(names).ok_or_else(...)`。但当前 `commands.rs` 中 `find_binary` 的函数签名是 `pub fn find_binary(name: &str) -> Option<String>` ——只接受单个 `&str`，不接受 `&[&str]` 数组。

### 6.3 Agent Phase 3 — 具体问题

#### P3-1: `Deno.connect` 在 Tauri WebView 中不可用 🔴

**Plan**: Task 16 `useAgent.ts` 用 `Deno.connect({ transport: 'unix', path: SOCKET_PATH })` 直连 Unix Socket。

**问题**: `Deno` 是 Deno 运行时 API，不存在于 Tauri WebView（浏览器环境）。Tauri 前端 JS 无法直接访问 Unix Socket。**这段代码完全不可运行。**

**修复**: Desktop 必须通过 Tauri invoke 命令走 Rust 侧连接 Agent IPC。需要新增 Rust command：
```rust
#[tauri::command]
async fn agent_ipc_call(method: String, params: Option<serde_json::Value>)
  -> Result<serde_json::Value, String>
```
由 Rust 侧连接 `~/.kn/agent/ipc.sock` 并转发请求。

#### P3-2: `tauri.conf.json` resources 语法错误

**Plan**: Task 17 写:
```json
"resources": { "resources/kn-agent": "." }
```

Tauri v2 的 `bundle.resources` 接受的是**数组**或通配符模式，不是对象映射:
```json
"resources": ["resources/kn-agent"]
```
或
```json
"resources": { "../resources/kn-agent": "./" }
```
具体取决于 target 目录结构。需对照 Tauri v2 文档修正。

#### P3-3: E2E 测试依赖 nc 但未标注

Task 18 测试脚本用 `nc -U ~/.kn/agent/ipc.sock`，macOS 自带 nc 支持 `-U`。ok。但测试在 CI 容器中无 macOS，应标注为"本地手动运行"。

### 6.4 Cloud Phase 1 — 具体问题

#### C1-1: 入口类拼写错误

**Plan**: Task 1 `KnCloudApiApplication.java` 写 `package arking dev.kn.cloud.api;`

`arking` 是噪声字符。应为 `package dev.kn.cloud.api;`

#### C1-2: WSS 消息中继是空 stub 🔴

**Plan**: Task 5 `KnWsHandler.handleTextMessage()`:
```java
@Override
protected void handleTextMessage(WebSocketSession session, TextMessage message) {
    String payload = message.getPayload();
    // 简化：直接转发给对应 device 的 Agent session
    // 实际实现需解析 JSON 中的 session_id，查 session 表获取 device_id
}
```

**问题**: Cloud P1 声称交付"消息中继"，但核心路由逻辑完全未实现。Agent→iOS 和 iOS→Agent 的消息转发在 P1 结束时不可用。这意味着 Agent P2 虽然能连 WSS，但无法与 iOS 通信。

**修复**: Task 5 应补充完整的消息路由伪代码（至少包含 `session_id → device_id → WebSocketSession` 的查表转发路径）。

#### C1-3: `handleBindingConnect` 方法缺失

**Plan**: Task 5 `afterConnectionEstablished` 调了 `handleBindingConnect(session, params)` 但该方法体未定义。临时 WSS 连接（`?code=xxx`）的验证逻辑缺失。

#### C1-4: WS 服务端口号配置缺失

Nginx 配了 `upstream kn_ws { server 127.0.0.1:8081; }`，但 `kn-cloud-ws` 的 `application.yml` 没有写端口。需明确 `server.port=8081`。

#### C1-5: init.sql 缺设计所需的表和列

与设计文档交叉对比，init.sql 缺少:
- `kn_push_token` 表（审查 §H-2）
- `kn_message` 表缺少 `src` 列（审查 §H-7）
- `kn_device.device_token` 应允许 NULL（解绑时清空），当前 DDL 是 `VARCHAR(512) UNIQUE`，允许 NULL
- `kn_session.status` 缺少 `interrupted` 枚举值（审查 §H-6）

### 6.5 Cloud Phase 2 — 具体问题

#### C2-1: 会员到期后不杀 WSS 连接 🔴

**Plan**: Task 9 `MembershipScheduler` 在缓冲期过后将 user status 标为 `expired`，将 session 标为 `failed`。但没有任何代码断开该用户关联 Agent 的 WSS 连接。

**设计文档 §3.1.2**: "24h 缓冲期过后：云端强制断开 Agent WSS → 所有活跃 session 终止"

**影响**: 到期用户的 Agent 仍然连着 WSS，仍然可以创建新 session（如果 Agent 不主动检查 user status）。`canCreateSession` 只在 `start_session` 消息到达时调用——但如果 Agent 侧缓存了之前的连接，用户仍能发消息。

**修复**: `MembershipScheduler` 需要调 `WsHandler.kickDevice(deviceId)` 关闭对应 WebSocket。

#### C2-2: APNs JWT 生成是 stub

**Plan**: Task 10 `generateApnsJwt()` 返回 `"APNS_JWT_TOKEN"`。实际实现需要:
1. 解析 p8 格式的椭圆曲线私钥（需要 BouncyCastle 或 JDK 内置 `java.security.KeyFactory`）
2. 用 ES256 算法签 JWT（jjwt 库支持，但需要先加载 p8 key）
3. JWT payload 包含 `iss`(team_id), `iat`(now), `head`(key_id)

这是非平凡的实现，不是简单的 TODO。应标注依赖库或具体实现路径。

#### C2-3: push payload key 与 iOS 侧不一致

**Cloud**: `ApnsService` 只发 `aps.alert`（标准 APNs payload），没有自定义 `kn_type` key。
**iOS Phase 2**: `PushManager.handleNotification` 期望 `userInfo["kn_type"]` 来判断推送类型。

两边的 payload 格式不匹配。云端的 `send()` 方法需要在 payload 中添加自定义字段 `kn_type`。

### 6.6 iOS Phase 1 — 具体问题

#### I1-1: WSS 重连逻辑 bug

**Plan**: Task 3 `WebSocketClient.scheduleReconnect()`:
```swift
Task {
    try? await Task.sleep(nanoseconds: UInt64(delay * 1_000_000_000))
    connect()  // ❌ 无参数调用
}
```

`connect()` 方法要求至少一个参数（`token`/`deviceToken`/`code`）。重连时应保存原始连接参数并在重连时复用。

#### I1-2: JSON-level ping 缺失

**Plan**: Task 3 只通过 `URLSessionWebSocketTask.sendPing` 发 WebSocket 协议层 ping。但设计 §4.3 要求发 `{type: "ping"}` 的 JSON 消息作为**应用层**心跳。`sendPing` 是传输层 PONG，不够——云端需要应用层心跳来判断客户端活性。

#### I1-3: ANSI 转义注入风险

**Plan**: Task 5 `TerminalView.updateUIView`:
```swift
let escaped = ansi.replacingOccurrences(of: "\\", with: "\\\\")
                 .replacingOccurrences(of: "`", with: "\\`")
webView.evaluateJavaScript("window.writeANSI(`\(escaped)`);")
```

ANSI 输出中可能包含 `` ` ``、`$`、`\`、`</script>` 等字符，手动转义极易遗漏。正确做法是用 base64 编码传到 JS 侧再解码:
```swift
let base64 = ansi.data(using: .utf8)!.base64EncodedString()
webView.evaluateJavaScript("window.writeANSIBase64('\(base64)');")
```

#### I1-4: `terminalResize` handler 未注册

**Plan**: Task 5.5 在 JS 侧注册 `terminalResize` handler，但 Task 5 (TerminalView) 中只给 `WKUserContentController` 注册了 `terminalInput` handler。`terminalResize` 未在 Swift 侧 `add(context.coordinator, name: "terminalResize")`。

#### I1-5: `bindDevice` 方法未实现

**Plan**: Task 6 `BindDeviceView` 调 `viewModel.bindDevice(code:)`，但 `DeviceViewModel` 的代码未在 plan 中给出。且绑定确认需要调 `POST /api/v1/device/bind-confirm`（需要 JWT），这部分逻辑缺失。

#### I1-6: Phase 1 检查清单自相矛盾

Phase 1 完成检查点说"已实现: InputAccessoryBar"、"尚未实现: 直通模式 [⚡] 开关 UI"。但 Task 8（同一 Phase）实现了直通模式。检查点与 Task 内容不同步。

#### I1-7: SessionListView 在 Phase 1 引用但 Phase 2 才创建

Task 7 的 `KnApp.swift` 引用了 `SessionListView()`，但这个 View 在 Phase 2 Task 10 才创建。Phase 1 编译不过。

### 6.7 iOS Phase 2 — 具体问题

#### I2-1: push notification type 与云端不匹配

见 §C2-3。云端的 `ApnsService` 和 iOS 的 `PushManager` 需要对齐 `kn_type` 字段名。

#### I2-2: 语音识别 locale 硬编码

`SFSpeechRecognizer(locale: Locale(identifier: "zh-CN"))` 硬编码中文。如果用户用英文 Terminal，识别英文会出错。应从 profile 或系统语言推断，或让用户选择。

### 6.8 实施计划汇总

计划总体质量：**中上**。所有 18 个 plan 层面 bug 已修复。
- P 类 (Agent): P1-2 (reqwest), P1-3/4/5, P3-3 ✅
- C 类 (Cloud): C1-1/4, C2-1/2/3 ✅  
- I 类 (iOS): I1-1/2/3/4/5/6/7, I2-1/2 ✅

---

## 七、验证方法

审查完成后，修复方案应通过以下方式验证:

1. **代码对照**: 逐项确认每个问题在设计文档和计划中是否已修正
2. **编译验证**: `cargo check --workspace --all-targets` 确认所有二进制编译通过
3. **计划逐 task 验证**: 对照上述 P/C/I 编号，逐 task 检查修复
4. **协议矩阵**: 拉一张 Excel: 行=功能, 列=消息类型, 逐格检查覆盖
5. **流程推演**: 按完整用户旅程 (安装→绑定→远程控制→断线→恢复→升级→过期) 逐步推演，检查每个步骤有无协议/状态支撑
6. **异常注入**: 对 §10 异常矩阵逐条做 "what if" 推演，确认处理链路完整
