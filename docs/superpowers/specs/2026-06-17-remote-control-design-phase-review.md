# 远程控制设计文档 & 实施计划 — 阶段审查

> 审查日期: 2026-06-17 | 范围: 设计文档 + 8 个实施计划 + 前两次审查 + 实际代码交叉验证

## 审查范围

| 材料 | 路径 | 状态 |
|------|------|------|
| 设计文档 | `docs/superpowers/specs/2026-06-16-remote-control-design.md` (2504 行) | ✅ 上次审查 27 项已修复 |
| 深度审查 | `docs/superpowers/specs/2026-06-17-remote-control-design-deep-review.md` | ✅ 35 项已修复 |
| 实施计划 × 8 | `docs/superpowers/plans/2026-06-16-*.md` | ⚠️ 见下文 |
| 实际代码 | `desktop/src-tauri/src/{commands,pty,lib}.rs`, `Cargo.toml` | 交叉验证 |

## 一、结论总览

**设计文档质量: A** — 逻辑完备、技术可行、安全措施到位。前两次审查的 62 项问题均已修复。

**实施计划质量: A-** — 原始审查发现 4B+8H+8M+9G 共 29 项问题。经过本轮修复，已全部解决或标注为 v2 技术债务。

**总体判断: 可以进入实施。**

---

## 二、阻塞性问题 (🔴 BLOCKER) — 必须在编码前解决

### B-1: Session ID 双路径不一致 — 本地创建 vs 云端创建

**问题描述**:

设计文档 §4.2 明确: `session_id = s_ + 12位 nanoid，由云端生成，全局唯一`。所有 session 创建都走 "异步确认模式"——云端收到 `start_session` → 生成 session_id → 写入 Redis pending → 转发 Agent。

但实际存在两条 session 创建路径:

| 路径 | 触发方 | 流程 | session_id 来源 |
|------|--------|------|----------------|
| A: iOS 远程 | iOS → Cloud WSS | Cloud 生成 | Cloud |
| B: 本地 Shell | `ai claude xxx` → Agent IPC | Agent 预分配 | **Agent (临时 ID)** |

**矛盾**: 路径 B 的 Agent Phase 2 Task 9 写 "本地预分配临时 ID (云端最终确认后以云端 ID 为准)"。但:
1. 本地 session 的 `session_id` 什么时候替换为云端 ID？
2. shell hook 返回给用户的 `session_id` 是临时 ID，后续 `kn agent attach <id>` 用哪个 ID？
3. 如果 Cloud 不可达，本地 session 就永远带临时 ID？
4. Agent 向 Cloud 上报 `session_created` 时，Cloud 发现 session_id 不是自己生成的怎么办？

**建议**: 设计文档需补充 "本地 Session 的 ID 策略" 小节，明确两条路径的 ID 生命周期。推荐方案:
- **Path B (local)**: Agent 仍生成 ID（格式 `s_` + 12位 nanoid），Cloud 接受 Agent 上报的 ID 并直接写入 MySQL，不再重新生成。全局唯一性由 nanoid 保证（62^12 ≈ 3.2×10^21，单机和多机均无碰撞风险）。
- 在 Cloud 的 `handle_session_created` 中，如果 session_id 已存在 → 返回 error 让 Agent 重新生成。
- 这样可以去掉 "临时 ID → 正式 ID" 的替换逻辑，两条路径统一用 nanoid。

### B-2: Agent Phase 2 Task 8 (pty.rs trait 抽取) 与 Agent Phase 1 Task 1 (common/pty_trait.rs) 重复

**问题描述**:

Agent P1 Task 1 Step 3 已在 `common/src/pty_trait.rs` 中定义了 `PtyOutputSink` trait + `SharedWriter`/`SharedChild`。

Agent P2 Task 8 又在 `desktop/src-tauri/src/pty.rs` 中重复定义一次 `PtyOutputSink` trait，且两个定义**不同**:
- **common 版**: `send(&self, data: &[u8])` + `on_ready/on_exit/on_error` 四个方法 + 默认实现
- **Task 8 版**: `send(&self, data: &[u8])` + `send_event` 两个方法，无默认实现

**影响**: 按 Plan 执行会导致编译错误 (duplicate trait definition) 或 trait 不一致。

**修复**: Agent P2 Task 8 整个重写。应该是:
1. 将 `desktop/src-tauri/src/pty.rs` 的 `drain_utf8_stream` 改为接受 `&impl kn_common::pty_trait::PtyOutputSink`
2. Desktop 侧创建 `ChannelSink` 实现 `PtyOutputSink`，包裹 `Channel<PtyEvent>`（tuple variant 语法修正，见 P1-3）
3. 去掉 Task 8 Step 1 在 `pty.rs` 顶部重复定义 trait 的部分
4. common 版 trait 需要补 `send_event` 方法（或拆到 Desktop 侧）

### B-3: Desktop IPC 桥接路径在 Tauri WebView 中不可行

**问题描述**:

原 Agent P3 Task 16 的前端代码直接用 `Deno.connect({ transport: 'unix', path: SOCKET_PATH })` 连接 Unix Socket。**`Deno` 是 Deno 运行时 API，不存在于 Tauri WebView（浏览器环境）。**

前次审查 P3-1 已将 Step 0 改为 Rust 侧 `agent_ipc` Tauri command 做中转。但以下问题仍需验证:

1. **Tauri invoke 性能**: PTY 输出通过 `invoke('agent_ipc', ...)` 每帧转发，Tauri invoke 的 IPC 开销（JSON 序列化 + 跨进程）可能成为瓶颈。PTY 大量输出时（如 cat 大文件），前端会卡死。
2. **流式输出语义**: 设计文档 §3.2.9 要求 `attach` 后持续推送 `output`。但 Tauri invoke 是请求-响应模式，不支持服务端主动推送。需要额外的 `Channel` 或 `Event` 机制。
3. **回退路径**: 当前 `pty.rs` 通过 Tauri `Channel<PtyEvent>` 直接推送给前端，Agent IPC 路径需要等效的推送机制。

**建议**: Desktop 需要新增一个 Tauri Event: `agent-pty-output`，Agent IPC 收到 PTY 输出后通过 `app_handle.emit("agent-pty-output", payload)` 推给前端。前端通过 `listen("agent-pty-output", ...)` 订阅。

### B-4: iOS SessionListView 在 Phase 1 引用但 Phase 2 才创建

**问题描述**:

iOS Phase 1 Task 7 的 `KnApp.swift` 中引用了 `SessionListView()`:

```swift
TabView {
    TerminalView(...)
    Text("Sessions")  // Phase 1 占位
    DeviceListView()
}
```

但 Tab 结构仍然引用了 Sessions tab。代码中写的是 `Text("Sessions")` 占位，所以能编译通过。**但** TabView 已经有 3 个 tab，其中一个是空的。用户体验不好。

**建议**: Phase 1 的 TabView 只放 Terminal + Devices 两个 tab，Session tab 在 Phase 2 再加。或在 Phase 1 做一个简单的占位视图（"暂无会话"）。

---

## 三、高优先级问题 (🟡 HIGH)

### H-1: `find_binary` 实际签名与 Plan 中不一致

**实际代码** (`commands.rs:640`):
```rust
pub(crate) fn find_binary(names: &[&str]) -> Option<String> {
```
接受 `&[&str]` 切片，不是单个 `&str`。

**Plan Agent P2 Task 15** 中:
```rust
let candidates: &[&str] = match tool {
    "claude" => &["claude"],
    ...
};
for name in candidates {
    if let Some(path) = find_binary(name) { ... }  // ❌ 需要 find_binary(&[name])
}
```

**影响**: 编译失败。调用处需改为 `find_binary(&[name])`，或者 match 分支直接传完整数组 `find_binary(candidates)`。

### H-2: 缺少 `hostname` + `http` crate 依赖声明

**Agent P1 Task 5** (`ws_client.rs`):
- 使用 `http::Request::builder()` → 需在 `agent/Cargo.toml` 加 `http = "1"`
- 使用 `hostname::get()` (Step 4.5) → 需加 `hostname = "0.1"`

**Agent P2 Task 9** (`ipc.rs`):
- 使用 `rand::random::<u64>()` → 需在 `agent/Cargo.toml` 加 `rand = "0.8"`（`rand` 仅在 `common/Cargo.toml` 中声明）

**影响**: 编译失败。Plan 中需补充这些依赖。

### H-3: 会员到期不踢 WSS 连接 — 设计 vs 计划不一致

**设计文档 §3.1.2**: "24h 缓冲期过后：云端强制断开 Agent WSS → 所有活跃 session 终止"

**Cloud P2 Task 9** `MembershipScheduler`:
- ✅ 将 user status 标为 `expired`
- ✅ 将 session 标为 `failed`
- ❌ **没有任何代码断开 Agent WSS 连接**
- ❌ `MembershipScheduler` 没有注入 `KnWsHandler` 引用，无法调用 `kickDevice()`

**影响**: 到期用户的 Agent 仍然连着 WSS，仍然可以创建新 session（如果 Agent 不主动检查）。Security boundary 失效。

**修复**: `MembershipScheduler` 需要通过 Redis Pub/Sub `ws:control` channel 发布 `{action: "kick_device", device_id: ...}` 消息。`WsHandler` 已经订阅了 `ws:control` channel（C1 Task 5），收到后调用 `kickDevice()` 就通了。需要在 Task 9 的代码中补这一段 Redis publish 逻辑。

### H-4: APNs JWT 生成是 stub — 非平凡实现

**Cloud P2 Task 10** `generateApnsJwt()`:
- 代码注释 "return generateSignedJWT(); // TODO: 完整实现见 Step 1.5"
- 但是 Step 1.5 只有 `KnPushToken` Entity + Mapper，没有 JWT 实现

**实际需要**:
1. 解析 p8 格式的椭圆曲线私钥（PEM SECP256R1）
2. 用 ES256 算法签 JWT
3. JWT payload: `{iss: team_id, iat: now, head: key_id}`

依赖: jjwt (已引入) 需要配合 BouncyCastle 做 EC 密钥解析，或 JDK 17+ 内置 `java.security.KeyFactory`。

**建议**: Task 10 需要新增一个明确的 Step 来实现 APNs JWT 签名，或标注为独立子 Task "10.5: APNs JWT 签名实现"，并给出具体实现路径。

### H-5: WSS URL 默认值 `wss://api.knshark.com` — 需确认开发/生产切换

设计文档中 WSS URL 为 `wss://api.knshark.com`。Plan 中 Agent 通过环境变量 `KN_CLOUD_URL` 覆盖，iOS 通过 `Info.plist` 覆盖，Cloud 侧部署时通过 `application.yml`。

**潜在问题**: 
- Agent 编译时没有默认开发环境 URL。本地测试时需手动设 `KN_CLOUD_URL=ws://localhost:8081`。容易忘记。
- iOS `Info.plist` 默认 `https://api.knshark.com`，本地开发需要用 `http://localhost:8080`。Xcode 多 scheme 管理需配置。

**建议**: 在 execution-guide.md 中补充"本地开发环境变量配置"表格，列出所有需要设置的环境变量和默认值。

### H-6: Profile env var 加密与 Python CLI 的向后兼容

设计 §6.5 引入 AES-256-GCM 加密 env var value，`common/src/config_crypto.rs` 实现。

**问题**: 当前 Python CLI (`bin/profile` + `lib/config.py`) 直接读写 `config.yaml` 明文。升级后:
1. Agent 保存 profile → env vars 被加密 → `config.yaml` 中出现 `kn:v1:hex` 格式
2. Python CLI 读取 → `sed`/`awk` 无法解密 → 返回密文给用户
3. Python CLI 写入 → 写明文 → Agent 通过 `decrypt_value` 的向前兼容逻辑（无前缀 = 明文）正常读取

所以单向兼容 OK。但 Python CLI 显示的 env var value 会是密文，体验不好。

**建议**: Phase 4 加一个任务: Python CLI 集成 Keychain 读取 + AES 解密，或在 Python 侧也实现解密逻辑（`security` 库调 Keychain，`cryptography` 库做 AES-GCM）。Plan 中目前完全没有 Python 侧的改造。

### H-7: 云服务无消息去重实现

设计 §4.5 定义了 `msg:dedup:{msg_id}` Redis key (TTL 5min) 用于幂等去重。但 Cloud Phase 1 Task 5 `WsHandler.handleTextMessage()` 中没有去重逻辑。

**影响**: 网络重传导致的消息重复不会被过滤。对于 `input`（用户输入文本）这会导致 PTY 中重复输入。

**建议**: Task 5 补一段: `String dedupKey = "msg:dedup:" + msgId; if (redis.hasKey(dedupKey)) return; redis.set(dedupKey, "1", Duration.ofMinutes(5));`

### H-8: build-agent.sh 路径不一致

| 文件 | 路径 | 用法 |
|------|------|------|
| Agent P3 Task 17 | `build-agent.sh` (repo 根) | 创建在根 |
| execution-guide.md 第 238 行 | `bash desktop/build-agent.sh` | 调用路径是 desktop/ 下 |
| Agent P3 Task 17 Step 4 | `bash desktop/build-agent.sh` | 同上 |

创建位置和调用位置矛盾。要么创建在 repo 根，要么创建在 desktop/ 下。如果 workspace 根 `Cargo.toml` 存在，脚本应放在 repo 根，用 `cargo build --release --bin kn-agent`。

---

## 四、中优先级问题 (🟢 MEDIUM)

### M-1: 工具名硬编码

Agent P2 Task 15 `resolve_tool_path` 中 tool 名称硬编码:
```rust
let candidates: &[&str] = match tool {
    "claude" => &["claude"],
    "codex"  => &["codex", "qoder"],
    "qoder"  => &["qoder", "codex"],
    _        => return Err(format!("未知 tool: {}", tool)),
};
```

v1 OK，但新增 tool 需要改代码。Phase 4 建议改为从 `config.yaml` 的 profile 名称反查 tool → binary 映射。

### M-2: Cloud 端口号硬编码

- `kn-cloud-api`: 8080 (未在任何配置文件显式声明)
- `kn-cloud-ws`: 8081 (仅在 `application.yml` 片段中出现)
- Nginx 配置直接写 `127.0.0.1:8080/8081`

**建议**: 全部通过 `application.yml` 的 `server.port` 配置，Nginx 用变量或注释说明端口对应关系。

### M-3: 错误消息中英混杂

设计文档和 Plan 中的错误消息部分中文、部分英文:
- `ErrorCode`: 中文 ("绑定码已过期或不存在")
- Cloud P1 `AuthFilter`: 英文 ("missing field: type")
- Agent P1 `ws_client.rs`: 中文 ("WSS 连接失败")

**建议**: 统一为英文（面向开发者 + 日志）+ iOS 端做本地化。

### M-4: Config backup rotation 在 Agent 侧未实现 ✅ 已验证无需改动

设计文档 CLAUDE.md 要求 config 写必须通过 3 代备份轮转（`.bak → .bak.1 → .bak.2 → .bak.3`）。但 Agent P1 Task 1 只提了跨进程文件锁，没有提备份轮转。

**影响**: Agent 写 config.yaml 如果坏了，可能覆盖掉唯一的 `.bak`。

**建议**: common/ 中提供 `write_config_atomic()` 函数，内部包含 backup rotation + fsync + rename。Desktop 和 Agent 共用。

**→ 验证结果 (2026-06-17)**: 经实际代码审查，`desktop/src-tauri/src/profile_cmd.rs:120-153` **已完整实现** 3 代备份轮转（`.bak → .bak.1 → .bak.2 → .bak.3`）+ `tmp → fsync → rename` 原子写。Agent P1 Task 1 Step 3 也已正确描述这套机制。迁入 `common/` 后 Desktop 和 Agent 直接复用，无需额外开发。

### M-5: iOS 键盘工具栏与 xterm.js 通信未考虑 CJK IME

iOS 上输入中文时，系统键盘有内联输入法候选区。`term.onData()` 在 IME 激活时收到的不是最终字符，而是 composing 状态下的中间输出。当前实现直接把 `term.onData()` 的每个字节发给 PTY，会导致 CJK 输入乱码。

**建议**: TerminalView 需处理 IME composing 状态——仅 `textInput` 的 `insertText` 回调发给 PTY，`setMarkedText` 仅在 WebView 侧显示。或改为使用 Swift 侧 `UITextInput` 协议处理输入。

### M-6: 云端 `agent_info` 处理不在主要消息路由中

设计 §4.3 定义了 `agent_info` inbound 消息类型。Cloud P1 Task 5 的 `handleTextMessage` 中有处理。但 Agent P1 Task 5 Step 4.5 在 `ws_client.rs` 中发送 `agent_info` 的逻辑位置在 **收到 connected 之后**，此时消息处理方法 `handle_message` 还没被调用（ws_client 用的是 `tx.send(msg)` → Agent 主循环处理）。Plan 中 Agent 侧发送 `agent_info` 是通过 `write.send()` 直接在 ws_client 内部发送的——正确，因为不需要经过 Agent 消息循环。

但 Agent 主循环收到其他消息时也需要能处理 `agent_info` 的响应吗？不需要，`agent_info` 只有上行。**无问题**。

### M-7: Cargo workspace 创建时 Desktop 代码的 import 更新工作量大

Agent P1 Task 1 Step 6 需要更新 Desktop 所有 `use crate::commands` / `use crate::profile_cmd` 引用。当前 `lib.rs` 中有大量 `commands::` 引用（约 20+ 处），全部需要改为 `kn_common::commands::`。

**工作量**: 约 30-60 分钟。已在 Plan 中提及，但需要确认编译后所有功能正常。

### M-8: 无 API 版本号 (v1) 在 URL 路径之外的协商

设计 §4.2 定义了 `protocol_version` 在 WSS `connected` 消息中协商。但 REST API (`/api/v1/`) 没有版本协商机制。URL 前缀版本号 (`v1`) 是手动升级的——如果 API 有不兼容变更，需要同时支持 `/api/v1/` 和 `/api/v2/` 两套，直到所有客户端升级。

**建议**: v1 OK，但应记录为已知约束：API breaking change = URL 前缀升级 + 旧版本保留至少一个发布周期。

---

## 五、实施步骤遗漏 (Gaps in Plans)

以下功能在设计文档中有描述或隐含，但在实施计划中缺少对应的 Task:

### 5.1 缺失的 Task

| # | 遗漏项 | 涉及设计章节 | 建议 Phase |
|---|--------|-------------|-----------|
| G-1 | **Python CLI 兼容加密 config** — Python 侧读取 `kn:v1:` 加密值的解密能力 | §6.5 | Phase 3 或 4 |
| G-2 | **Agent 侧消息去重** — `msg:dedup` 检查，防重放 | §4.5 | Phase 2 |
| G-3 | **云端 session:pending TTL 超时清理回调** — TTL 到期后的 error 通知给调用方 | §3.1.3 | Phase 2 |
| G-4 | **iOS App 启动时 token 有效性检查** — 如果 Keychain 中 JWT 已过期 → 跳到登录页 | 隐含 | Phase 1 |
| G-5 | **Agent IPC 推送 PTY 输出到 Desktop** — Tauri Event 机制桥接 | §3.4.5 | Phase 3 |
| G-6 | ~~**config.yaml 的原子写工具函数** (含 3 代备份轮转)~~ ✅ 已有 — `profile_cmd.rs` 已实现，迁入 common/ 即可复用 | CLAUDE.md | Phase 1 (common/) |
| G-7 | **Agent 日志远程采集** — 至少有个设计/占位 | 附录 C.1 | Phase 4 |
| G-8 | **iOS xterm.js CJK IME 处理** — 键盘 composing 状态管理 | §3.3.2 | Phase 2 |
| G-9 | **Desktop 📡 恢复连接** (resume) — panel 中的 resume 按钮后端逻辑 | §3.4.2 | Phase 3 |

### 5.2 已有 Task 内缺失的 Step

| Task | 缺失内容 |
|------|---------|
| Agent P1 Task 5 | `agent/Cargo.toml` 需加 `http`, `hostname` 依赖 |
| Agent P2 Task 9 | `agent/Cargo.toml` 需加 `rand` 依赖 |
| Agent P2 Task 15 | `find_binary` 调用签名修正 (`&[name]` 而非 `name`) |
| Cloud P1 Task 5 | `handleBindingConnect` 方法体需实现；消息去重逻辑需补 |
| Cloud P2 Task 9 | `MembershipScheduler` 需通过 Redis Pub/Sub 通知 WsHandler 踢连接 |
| Cloud P2 Task 10 | APNs JWT 签名需具体实现（非 TODO） |
| iOS P1 Task 3 | `scheduleReconnect()` 需保存/复用连接参数；需发应用层 JSON ping |
| iOS P1 Task 5 | `TerminalView` 需注册 `terminalResize` handler |
| iOS P1 Task 6 | `DeviceViewModel.bindDevice()` 需完整实现 (HTTP POST bind-confirm) |

---

## 六、性能与架构审查

### 6.1 性能关注点

| 关注点 | 风险 | 缓解 |
|--------|------|------|
| Agent IPC → Tauri invoke → 前端 (PTY 输出) | 🔴 高 | Tauri invoke 每帧 JSON 序列化开销；需走 Event 推送而非 invoke 轮询 |
| Cloud WS 消息中继 (Redis Pub/Sub) | 🟡 中 | 单实例零开销；多实例时跨实例中继有 Redis 网络延迟 (~1ms)。初期单实例 OK |
| Agent log 每日翻转 + 7 天清理 | 🟢 低 | 已实现，目录遍历每天一次，开销忽略 |
| iOS xterm.js scrollback: 5000 | 🟢 低 | ~1-2MB 内存，WKWebView 可承受 |
| 每 30s checkpoint 写入 | 🟢 低 | 小文件原子写，每个 session < 1KB |
| 多 Session 并发 PTY | 🟢 低 | tokio async I/O，单个 Agent 可管理 10+ session |

### 6.2 架构关注点

| 关注点 | 评价 |
|--------|------|
| **Cargo workspace 方案** | ✅ 正确决定。避免 Agent 链接 Tauri 依赖，二进制体积从几十 MB 降到几 MB |
| **Cloud HTTP / WS 分进程** | ✅ 正确。HTTP 重启不影响 WS 长连接。Nginx 反向代理简洁 |
| **Agent 是 PTY 唯一持有者** | ✅ 正确。单点管理，输入/输出统一路由 |
| **Desktop 保留直接 PTY 降级路径** | ✅ 正确。Agent 离线不影响本地使用 |
| **设备指纹单因子 (IOPlatformUUID)** | ✅ 正确。去掉了不可靠的 MAC/hostname |
| **卡密 AES-256-GCM 加密** | ✅ 正确。纯随机码可爆破，加密后每个码自包含签名 |
| **profile env var AES-256-GCM + Keychain** | ⚠️ 方向正确，但 Python CLI 兼容性需补 |

---

## 七、实施阶段评估

### 7.1 各 Phase 就绪度（更新于 2026-06-17 修复完成）

| Phase | 就绪度 | 说明 |
|-------|--------|------|
| **Agent P1** | 🟢 95% | http/hostname 依赖已补 (H-2)；find_binary 签名已修正 (H-1)；backup rotation 已确认 (M-4)；消息去重已补 (G-2) |
| **Agent P2** | 🟢 95% | Task 8 已改为引用 common trait (B-2)；session ID 去中心化 (B-1)；nanoid 替代 u64 hex |
| **Agent P3** | 🟢 95% | IPC 长连接 + Tauri Event 桥接已补 (B-3, Step 0.6)；build-agent.sh 路径统一 (H-8)；resume 流程已补 (G-9) |
| **Cloud P1** | 🟢 95% | 消息去重已补 (H-7)；handleBindingConnect 已实现 + ws:bind-result Pub/Sub (5.2)；端口显式声明 (M-2) |
| **Cloud P2** | 🟢 90% | APNs JWT 完整实现 (H-4)；会员到期 Redis Pub/Sub 踢连接 (H-3)；pending TTL 超时通知 (G-3) |
| **iOS P1** | 🟢 95% | SessionListView 引用已移除 (B-4)；CJK IME 缓冲保护 (M-5)；terminalResize 已注册 (5.2)；bindDevice 模型修正 (5.2) |
| **iOS P2** | 🟢 90% | push payload kn_type 已对齐；检查清单一致 (5.2) |

### 7.2 执行顺序合理性

执行指南中三端 Phase 1 并行的策略是正确的——三个 repo 互不依赖，可以同时开工。

**建议调整**: Agent P2 应在 Cloud P1 WSS 可连通之后做（执行指南已正确标注）。但还需加一个前提: **Agent P2 Task 8 (pty.rs trait 抽取) 是 Desktop 现有代码的重构，应尽早做**，最好在 Agent P1 完成后立即做，因为:
1. 它影响现有 Desktop 代码
2. 做完了 Agent 后续 Task 才能用 trait
3. 做完了 Desktop P3 才能开始 IPC 对接

---

## 八、行动计划

### 8.1 编码前修复清单 (P0 — 必须修) ✅ 全部完成

- [x] **B-1**: 设计文档 §3.1.3 改为去中心化 session_id，发起方本地生成 nanoid
- [x] **B-2**: Agent P2 Task 8 改为引用 `kn_common::pty_trait::PtyOutputSink`
- [x] **B-3**: Agent P3 Task 16 新增 Step 0.6 IPC 长连接 + Tauri Event 桥接
- [x] **B-4**: iOS P1 Task 7 TabView 改为 Terminal + Devices 两页
- [x] **H-1**: Agent P2 Task 15 `find_binary(candidates)` 匹配 `&[&str]` 签名
- [x] **H-2**: Agent P1 Task 1 Cargo.toml 补 `http = "1"` + `hostname = "0.1"`
- [x] **H-3**: Cloud P2 Task 9 MembershipScheduler 补 deviceMapper/redis 注入
- [x] **H-4**: Cloud P2 Task 10 generateApnsJwt() 完整实现 ES256 + p8 解析

### 8.2 编码后修复清单 (P1 — 边做边修) ✅ 全部完成

- [x] **H-5**: execution-guide.md 新增"本地开发环境变量"章节（7 变量 + 启动命令）
- [x] **H-6**: 设计文档 Phase 4 新增 Python CLI 加密兼容任务
- [x] **H-7**: Cloud P1 Task 5 补消息去重 (session_id + seq, 5min TTL)
- [x] **H-8**: build-agent.sh 路径统一为 repo 根

### 8.3 v2 改进清单 (P2 — 不阻塞 v1) ✅ 全部完成

- [x] M-1: `resolve_tool_path` 标注 v2 改为 config 动态反查
- [x] M-5: terminal.html 加 IME compositionstart/end 缓冲保护
- [x] M-4: 确认 profile_cmd.rs 已有 3 代备份轮转，补文档说明
- [x] M-3: 错误消息统一中文 (Cloud WsHandler 5 处)
- [x] M-2: Cloud API application.yml 显式声明 server.port: 8080
- [x] M-8: 设计文档 §3.1.1 新增 REST API 版本策略
- [x] M-6: 确认无问题（agent_info 仅上行）
- [x] G-2/3/4/9: 缺失 Task 已补全
- [x] G-8: iOS xterm.js IME 处理（同 M-5，已加 compositionstart/end 缓冲）
- [ ] G-7: Agent 日志远程采集 — v2 技术债务（设计附录 C.1 已记录，v1 不实现）

---

## 九、总结

设计文档经过三次审查，质量已经很高。**逻辑闭环完整**，异常场景矩阵覆盖全面（40+ 场景），安全设计有深度（4 层防共享 + 设备指纹 + 卡密加密）。**技术上全部可行**，Rust workspace + Java Spring Boot + SwiftUI 都是成熟技术。

实施计划已覆盖所有功能点。仅 1 项遗留为 v2 技术债务：**Agent 日志远程采集**（附录 C.1 已记录）。

P0（4B+4H）、P1（4H）、P2（8M+4G）共 28 项问题已全部修复，可进入实施。云端和 Agent 可同时开工，iOS 在 Agent P1 完成后启动。
