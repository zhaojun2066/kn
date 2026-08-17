# kn 全栈架构与代码审查报告

> **审查日期**: 2026-06-26  
> **审查范围**: `kn` (agent + desktop) · `kn-cloud` (api + ws) · `kn-ios`  
> **审查维度**: 正确性 · 架构设计 · 安全性 · 容错 · 复用性 · 扩展性 · 跨仓库协议一致性

---

## 目录

1. [Executive Summary](#1-executive-summary)
2. [架构概览](#2-架构概览)
3. [Bug 清单](#3-bug-清单)
   - [Critical](#critical)
   - [High](#high)
   - [Medium](#medium)
   - [Low](#low)
4. [架构与设计问题](#4-架构与设计问题)
5. [安全审计](#5-安全审计)
6. [容错与弹性](#6-容错与弹性)
7. [协议一致性](#7-协议一致性)
8. [改进建议行动计划](#8-改进建议行动计划)
9. [附录](#9-附录)

---

## 1. Executive Summary

### 总体质量评估

| 维度 | 评级 | 说明 |
|------|------|------|
| **正确性** | ⭐⭐⭐⭐ | 核心流程正确，存在少量边界条件 bugs |
| **架构** | ⭐⭐⭐ | 模块划分合理，但部分文件过大、职责混杂 |
| **安全性** | ⭐⭐⭐ | 存在 1 个 Critical + 3 个 Medium 安全问题 |
| **容错** | ⭐⭐⭐ | 重连机制完善，但部分降级策略有缺陷 |
| **复用性** | ⭐⭐ | kn-cloud 中会员逻辑重复；iOS 侧 WsData 解析过度补偿 |
| **协议一致性** | ⭐⭐ | WSS envelope 格式在 cloud 内部不一致，iOS 被迫 5 层 fallback |

### 发现问题统计

| 严重程度 | 数量 | 涉及仓库 |
|----------|------|---------|
| **Critical** | 1 | kn-cloud |
| **High** | 4 | kn-agent (2), kn-cloud (1), kn-desktop (1) |
| **Medium** | 8 | kn-agent (2), kn-cloud (3), kn-ios (2), 跨仓库 (1) |
| **Low** | 7 | kn-agent (1), kn-desktop (4), kn-ios (2) |

### 关键结论

1. **kn-cloud 生产配置存在硬编码 JWT secret 回退值**——如果环境变量未设置，服务不会启动失败而是静默使用开发密钥，这是一个严重的安全漏洞。
2. **WSS 协议 envelope 格式不统一**——cloud 端 `SessionCoordinator.sendSessionEventToUser` 把 `sessionId` 放在顶层 JSON，而其他消息放在 `data` 内部。iOS 被迫用 5 种 fallback 策略来兼容这种不一致性。
3. **kn-agent 代码质量较好**，核心状态机和 PTY 多路复用逻辑正确，但部分模块（`session.rs` 1259 行、`main.rs` 986 行）过大，测试中有断言错误。
4. **kn-desktop 状态管理需要重构**——`App.tsx` 约 30 个 useState，`useTerminal.ts` 1006 行承载过多职责。

### 修复执行摘要 (2026-06-26 更新)

本轮修复重点为 **kn-agent 模块拆分与协议统一**，同时修复了部分 kn-cloud 安全/容错问题。

| 严重程度 | 总数 | ✅ 已修复 | ❌ 未修复 | ⚠️ 部分修复 |
|----------|------|----------|----------|------------|
| **Critical** | 1 | 1 | 0 | 0 |
| **High** | 4 | 3 | 1 | 0 |
| **Medium** | 8 | 6 | 1 | 1 |
| **Low** | 7 | 2 | 5 | 0 |
| **架构改进** | 10 | 5 | 5 | 0 |
| **合计** | **30** | **17** | **12** | **1** |

**已完成的核心工作：**
- ✅ kn-agent: `session.rs` (1259 行) 拆分为 7 个模块；`main.rs` (986→309 行) 拆分为 handler/heartbeat/project/logging
- ✅ kn-agent: WSS 协议测试重写；deprecated `session_interrupted` 移除；`OutgoingMessage` 类型化 enum
- ✅ kn-agent: WSS 重连计数器重置、`append_log_static` 持久化、空函数 `restore_codex_auth` 移除
- ✅ kn-cloud: JWT secret 去掉硬编码回退、MailService fail-closed、SessionService ObjectMapper 注入
- ✅ kn-cloud: 会员逻辑提取到 `MembershipChecker` (common)；`sendSessionEventToUser` 统一使用标准 envelope
- ✅ kn-desktop: relay poller 改用递归 setTimeout

**未完成（待后续处理）：**
- ❌ kn-desktop: TOCTOU 路径检查、CSP 禁用、TOML 解析 `splitn`
- ❌ kn-cloud: 跨节点 `start_session` 检查
- ❌ kn-ios: `@unchecked Sendable` 审计、证书 pinning
- ❌ 大文件拆分: `useTerminal.ts` (1001 行)、`App.tsx` (2470 行)、`commands.rs` (1527 行)

---

## 2. 架构概览

### 2.1 系统拓扑

```
┌──────────────────────────────────────────────────────────────────┐
│                        User's Mac                                │
│  ┌─────────────┐     Unix Socket      ┌──────────────────┐      │
│  │ kn Desktop  │ ◄────────────────── ► │   kn-agent       │      │
│  │ (Tauri v2)  │    IPC (JSON-line)    │ (Rust daemon)    │      │
│  │ TS + Rust   │                       │ launchd 托管     │      │
│  └──────┬──────┘                       └────────┬─────────┘      │
│         │ PTY (portable-pty)                    │ WSS             │
│         ▼                                       │                 │
│  ┌──────────────┐                                │                 │
│  │  embedded    │                                │                 │
│  │  xterm.js    │                                │                 │
│  │  terminals   │                                │                 │
│  └──────────────┘                                │                 │
└──────────────────────────────────────────────────┼─────────────────┘
                                                   │
                    ┌──────────────────────────────┘
                    ▼
┌──────────────────────────────────────────────────────────────────┐
│                     kn-cloud (Linux Server)                       │
│  ┌─────────────────┐        ┌──────────────────┐                 │
│  │  kn-cloud-api   │        │  kn-cloud-ws     │                 │
│  │  :8080 (REST)   │        │  :8081 (WSS)     │                 │
│  │  JWT Auth       │        │  Device Auth     │                 │
│  └───────┬─────────┘        └────────┬─────────┘                 │
│          │ Redis / MySQL              │ Redis / MySQL             │
│          └──────────┬────────────────┘                           │
│                     ▼                                            │
│  ┌──────────────────────────────────────┐                        │
│  │  Nginx (api.shark.kim)               │                        │
│  │  TLS termination + reverse proxy     │                        │
│  └──────────────────────────────────────┘                        │
└──────────────────────────────────────────────────────────────────┘
                    │
                    │ WSS (JWT Auth)
                    ▼
┌──────────────────────────────────────────────────────────────────┐
│                     kn-ios (iPhone)                               │
│  ┌──────────────────┐    ┌──────────────────┐                    │
│  │  WebSocket       │    │  HTTPClient      │                    │
│  │  Transport       │    │  (REST API)      │                    │
│  └────────┬─────────┘    └────────┬─────────┘                    │
│           │                       │                               │
│  ┌────────┴───────────────────────┴──────────┐                   │
│  │  SwiftUI Views + @Observable ViewModels   │                   │
│  └───────────────────────────────────────────┘                   │
└──────────────────────────────────────────────────────────────────┘
```

### 2.2 关键数据流

```
[iOS 发起远程会话]
   iOS start_session ──WSS──► kn-cloud-ws ──WSS──► kn-agent
                                                      │
                                              PTY spawn + session_created
                                                      │
   iOS terminal input ◄──WSS── kn-cloud-ws ◄──WSS── output stream
       (relay)                                          (relay)

[Desktop 本地会话]
   desktop invoke("write_pty") ──► Rust PTY stdin
   Rust PTY stdout ──Channel──► xterm.js render
```

### 2.3 技术栈汇总

| 组件 | 语言/框架 | 关键依赖 |
|------|----------|---------|
| kn-agent | Rust · tokio | tungstenite, portable-pty, serde_json, tokio-stream |
| kn-desktop (backend) | Rust · Tauri v2 | portable-pty, serde_yaml, fs2, reqwest, sha2 |
| kn-desktop (frontend) | TypeScript · React 18 | xterm.js, Tailwind CSS |
| kn-cloud | Java 21 · Spring Boot 3.3 | MyBatis-Plus, Redis, jjwt, APNs |
| kn-ios | Swift · SwiftUI | URLSession WebSocket, Keychain |

---

## 3. Bug 清单

### Critical

#### C1. kn-cloud: 生产环境 JWT secret 存在硬编码回退值 ✅ 已修复

- **位置**: `kn-cloud-api/src/main/resources/application-prod.yml:33` + `kn-cloud-ws/src/main/resources/application-prod.yml:34`
- **代码**: ~~`secret: ${JWT_SECRET:!QAZxswdsa^%F4K(*JdsdshjKIO#$sdshark}`~~ → `secret: ${JWT_SECRET}`
- **影响**: 如果生产环境忘记设置 `JWT_SECRET` 环境变量，服务不会启动报错，而是静默使用硬编码的开发密钥。任何读过此配置文件的人都可以伪造 JWT token。
- **修复**: 已改为 `secret: ${JWT_SECRET}`（去掉默认值），Spring Boot 在缺失时会启动失败。

---

### High

#### H1. kn-agent: 协议测试断言错误 ✅ 已修复

- **位置**: `agent/tests/wss_protocol_test.rs`
- **代码**: ~~`assert_eq!(v["data"]["to_session_id"], 42);`~~
- **修复**: 测试文件已完全重写（341 行），统一使用 `sessionId` 字符串字段名，对齐新的 WSS 协议格式。测试现在正确验证 `sessionId` 为 String 类型。

#### H2. kn-agent: Ctrl+C 处理器重复查询 sessions（竞态窗口） ✅ 已修复

- **位置**: `agent/src/main.rs`（原 L806, L826）
- **修复**: `main.rs` 已从 986 行重构为 309 行。Ctrl+C 现在仅设置 `CancellationToken` 关闭信号，所有 session 清理由 `Drop` + `shutdown` 路径统一处理，不再有重复查询。

#### H3. kn-cloud: 会员/宽限期逻辑在 API 和 WS 模块间重复 ✅ 已修复

- **位置**:
  - `kn-cloud-api/src/main/java/.../service/MembershipService.java:77-96`
  - `kn-cloud-ws/src/main/java/.../handler/SessionCoordinator.java:590-595`
  - `kn-cloud-common/src/main/java/.../service/MembershipChecker.java` ⬅️ 新增共享类
- **修复**: `MembershipChecker` 已提取到 `kn-cloud-common`，`isInGracePeriod` / `isExpired` / `GRACE_PERIOD_DAYS` 的权威实现在 common 中。`MembershipService` 和 `SessionCoordinator` 均通过 delegation 调用 `MembershipChecker`。未来修改只需改 common 一处。

#### H4. kn-desktop: TOCTOU 文件路径安全检查 ❌ 未修复

- **位置**: `desktop/src-tauri/src/commands.rs:141-185`
- **代码**: `is_safe_path` 内部通过 `canonicalize()` 解析路径，但返回值仍是 `bool`，调用方 `write_file`/`read_file` 继续使用原始 `path` 字符串。
- **影响**: 存在 TOCTOU (Time-of-check-to-time-of-use) 竞态条件。
- **待修复**: `is_safe_path` 应返回 `Option<PathBuf>`（解析后的规范路径），调用方使用解析后的路径操作。

---

### Medium

#### M1. kn-agent: `append_log_static` 日志大小跟踪不持久 ✅ 已修复

- **位置**: `agent/src/session/output.rs:40-55`
- **修复**: 新增 `STATIC_LOG_SIZES` 全局 `HashMap<String, Arc<AtomicU64>>` + `get_static_log_size(nid)` 函数。`append_log_static` 现在通过此 HashMap 复用每个 nid 的日志大小跟踪，256KB 截断逻辑正确工作。

#### M2. kn-agent: 重复方法 `session_interrupted` (deprecated) vs `sessions_interrupted` ✅ 已修复

- **位置**: `agent/src/proto.rs:440-442`
- **修复**: deprecated 的 `session_interrupted` 已删除，仅保留 `sessions_interrupted`（在 `data` 内部包含 sessions 数组）。代码清晰无歧义。

#### M3. kn-cloud: `SessionService` 使用私有 ObjectMapper 绕过安全限制 ✅ 已修复

- **位置**: `kn-cloud-api/src/main/java/.../service/SessionService.java:34,36`
- **修复**: `SessionService` 现在通过构造函数注入全局配置的 `ObjectMapper`（`private final ObjectMapper objectMapper;`），不再自行创建私有实例。继承了 `JacksonConfig` 的 `StreamReadConstraints`。

#### M4. kn-cloud: 跨 WebSocket 实例的 `start_session` 只检查本地连接表 ❌ 未修复

- **位置**: `kn-cloud-ws/src/main/java/.../handler/SessionCoordinator.java:145`
- **代码**: `registry.getAgentSession(machineId)` 仍只查询本地 `ConcurrentHashMap`。
- **影响**: 多节点部署时，如果 agent 连接到 node-A 而 iOS 用户连接到 node-B，`start_session` 会错误地报告 agent 离线。
- **待修复**: 在检查本地连接表之前，先查询 Redis `ws:agent:{machineId}` 获取 agent 所在 wsNode，再决定是本地转发还是跨节点 relay。

#### M5. kn-cloud: `MailService` Redis 故障时 fail-open 绕过频率限制 ✅ 已修复

- **位置**: `kn-cloud-api/src/main/java/.../service/MailService.java:97`
- **代码**: ~~`return false; // Redis 不可用，放行（fail-open)`~~ → `return true; // Redis 不可用，拒绝发送（fail-closed），防止绕过频率限制`
- **修复**: 已改为 fail-closed 策略——Redis 不可用时拒绝发送邮件，防止邮件轰炸。

#### M6. kn-ios: `AuthRepositoryImpl.refreshToken` 返回假的 userId=0 ⚠️ 部分修复

- **位置**: `kn-ios/Data/Repositories/AuthRepositoryImpl.swift:79-80`
- **修复**: 现在通过 `JWTDecoder.decodeUserId(from: result.accessToken)` 从 JWT payload (sub claim) 解码 userId，仅在解码失败时回退到 0。比之前硬编码 0 有改进，但 fallback=0 依旧不完美。
- **待完善**: 如果 JWT 解码也失败，应让 refresh 流程报错而非静默创建 userId=0 的用户对象。

#### M7. kn-ios: `LoginUseCase` 创建空的 User 对象 ✅ 已修复

- **位置**: `kn-ios/Domain/UseCases/LoginUseCase.swift:40-55`
- **修复**: `LoginUseCase` 现在在登录后调用 `authRepo.fetchProfile()` 获取真实用户信息（email/nickname/membership/status/expiry），再构造 `User` 对象。不再使用假数据。

#### M8. 跨仓库: WSS envelope 格式不一致 ✅ 已修复

- **位置**: `kn-cloud-ws/.../SessionCoordinator.java:519-524`
- **修复**: `sendSessionEventToUser` 已改为使用标准 envelope：`WsMessageFactory.envelope(mapper, type, sessionNid)` 构建，`sessionId` 放入 `data` 内部。不再出现顶层 `sessionId` 与其他消息不一致的问题。
- **注**: iOS 侧的 5 层 fallback 解析代码尚未简化（见 L7 待办），但协议不一致的根因已消除。

---

### Low

#### L1. kn-agent: WSS 重连退避计数器不重置 ✅ 已修复

- **位置**: `agent/src/ws_client.rs:127`
- **代码**: 成功建立 WSS 连接后执行 `attempt = 0;`。
- **修复**: 成功连接后计数器正确重置，短暂网络抖动不再导致不必要的长退避等待。

#### L2. kn-agent: `restore_codex_auth()` 是空函数 ✅ 已修复

- **位置**: 整个 agent 代码库中已无 `restore_codex_auth` 的任何引用
- **修复**: 该空函数及其调用点已完全移除。Codex auth 恢复逻辑由重构后的 session 模块通过其他机制处理。

#### L3. kn-desktop: `useAgent.ts` relay poller 使用 `setInterval` 可能重叠 ✅ 已修复

- **位置**: `desktop/src/hooks/useAgent.ts:108-116`
- **代码**: 现在使用递归 `setTimeout` + `pollTimeoutRef` 模式（"recursive setTimeout to prevent overlapping cycles"）。
- **修复**: 每次 poll 完成后才调度下一次，消除请求重叠和响应错乱风险。

#### L4. kn-desktop: `useAgent.ts` relay poller 前一个 interval 未清理 ✅ 已修复

- **位置**: `desktop/src/hooks/useAgent.ts:109-116`
- **修复**: relay poller 重构使用单一 `pollTimeoutRef` + `pollPausedRef` 模式，通过 `mounted` 标志和 cleanup 函数确保组件卸载时正确清理，不再有残留 interval 问题。

#### L5. kn-desktop: CSP 被禁用 ❌ 未修复

- **位置**: `desktop/src-tauri/tauri.conf.json:25`
- **代码**: `"csp": null` 未变更。
- **待修复**: 设置 `"csp": "default-src 'self'"` 作为最小保护。

#### L6. kn-desktop: `scan_system_configs` TOML 解析过于简单 ❌ 未修复

- **位置**: `desktop/src-tauri/src/commands.rs:541-549`
- **代码**: 仍使用 `splitn(2, '=')` 逐行解析 TOML。
- **待修复**: 使用 `toml` crate（已是项目依赖）进行标准解析。

#### L7. kn-ios: `WebSocketTransport` 使用 `@unchecked Sendable` 绕过线程安全检查 ❌ 未修复

- **位置**: `kn-ios/Data/Network/WebSocketTransport.swift:32` + `HTTPClient.swift:35`
- **代码**: 两个类仍标注 `@unchecked Sendable`，虽然代码中添加了注释说明审计结论（"所有可变状态均受 MainActor 保护"），但未去掉 `unchecked`。
- **待修复**: 如果审计确认安全，改为 `@MainActor final class` 或使用 `@Sendable` 协议一致性，让编译器真正参与验证。

---

## 4. 架构与设计问题

### 4.1 模块过大，职责混杂

| 文件 | 行数 (审查时→现在) | 问题 | 建议 | 状态 |
|------|---------------------|------|------|------|
| `agent/src/session.rs` | 1,259 → **已删除** | SessionManager + InputMerger + OutputFanout + PTY 创建 + 工具路径解析 + Codex auth | 拆分为 `session/mod.rs` + `session/pty.rs` + `session/output.rs` + `session/input.rs` | ✅ 已拆分 |
| `agent/src/main.rs` | 986 → **309** | 入口 + 事件循环 + 4 分支 `tokio::select!` + 消息处理 match ~400 行 | 按消息类型拆分为 handler 模块 | ✅ 已拆分 |
| `desktop/src/hooks/useTerminal.ts` | 1,006 → **1,001** | PTY 生命周期 + Tab 管理 + 面板分割树 + 远程中继 + 历史记录 | 提取 `usePaneTree` hook + `usePtyLifecycle` hook | ❌ 未拆分 |
| `desktop/src/App.tsx` | 2,471 → **2,470** | ~30 个 useState，所有状态集中在顶层 | 引入 React Context (ProfileContext, AgentContext, TerminalContext) | ❌ 未拆分 |
| `desktop/src-tauri/src/commands.rs` | 1,528 → **1,527** | 配置 CRUD + 文件 I/O + 环境检查 + 目录遍历 + 编辑器启动 | 按领域拆分 | ❌ 未拆分 |

### 4.2 协议设计问题

**两阶段反序列化**（`agent/src/proto.rs:108-228`）设计巧妙但错误处理不一致：

```rust
// 一些字段使用 unwrap_or("")（静默忽略缺失）:
"pong" => { ts: data.get("ts").and_then(|v| v.as_str()).unwrap_or("") }

// 另一些使用 ok_or_else（返回错误）:
"start_session" => { data: data.get("data").ok_or_else(|| ...) }
```

**建议**: 统一错误处理策略——要么全部 fail-fast 返回错误，要么全部提供合理默认值。

### 4.3 跨仓库实体重复

kn-cloud 中实体的 **有意重复**（API 和 WS 模块各自独立定义 `KnDevice`、`KnUser` 等实体）是一门权衡取舍：

| 优点 | 缺点 |
|------|------|
| 模块独立演进 | Schema 变更需改 N 处 |
| 编译期隔离 | 容易遗漏更新 |
| 无循环依赖 | 字段定义可能不一致 |

当前设计**可行但需警惕**。建议：
- 每个模块的 entity 定义旁加注释说明其"权威来源"模块
- Schema 变更时 CI 检查是否所有模块的 entity 同步更新

### 4.4 并发设计 ✅ 已改进

**kn-agent 的 outgoing channel** 已从 `Arc<Mutex<Option<UnboundedSender<String>>>>` 升级为 `Arc<tokio::sync::Mutex<Option<mpsc::UnboundedSender<OutgoingMessage>>>>`。

**`OutgoingMessage` enum** 已实现（`agent/src/proto.rs:258`）：
```rust
pub enum OutgoingMessage {
    Ping,
    SessionCreated { nid: String, tool: String, cwd: String, ... },
    Output { nid: String, ansi_text: String },
    SessionEnded { nid: String, reason: String },
    ProfileList { profiles: Vec<ProfileInfo> },
    ProjectList { projects: Vec<ProjectInfo> },
    SessionsInterrupted { sessions: Vec<InterruptedSession> },
    ErrorNotify { code: String, message: String },
}
```
在 channel 边界序列化，提升类型安全。锁类型从 `std::sync::Mutex` 改为 `tokio::sync::Mutex` 避免异步上下文中的阻塞。

---

## 5. 安全审计

### 5.1 认证与授权

| 检查项 | kn-agent | kn-desktop | kn-cloud | kn-ios |
|--------|----------|------------|----------|--------|
| JWT 密钥管理 | N/A | N/A | ⚠️ 生产配置有硬编码回退 | ✅ Keychain |
| Token 刷新机制 | N/A | N/A | ✅ access 15min + refresh 30d | ✅ 自动刷新 |
| 设备绑定认证 | ✅ device_token 持久化 | ✅ Unix socket (FS 权限) | ✅ 6位绑定码 300s TTL | ✅ 扫码验证 |
| 速率限制 | ❌ 无 | ❌ 无 | ✅ Redis + 本地 fallback | N/A |
| 会话劫持防护 | ✅ machine_id 验证 | N/A | ✅ 并发检测 + 踢出 | N/A |

### 5.2 关键安全问题

1. **[C1] 生产 JWT secret 硬编码回退** (见 Bug 清单)
2. **[H4] TOCTOU 文件路径** (见 Bug 清单)
3. **[M5] MailService fail-open** (见 Bug 清单)
4. **[L5] CSP 禁用** (见 Bug 清单)
5. **kn-ios 无证书 pinning**: `AppConfiguration.enableCertificatePinning` 默认 false，HTTPClient 使用 `URLSession.shared` 无自定义 delegate。在不受信任网络中存在 MITM 风险。
6. **iOS `UserDefaultsStore` 存储 server IP 为明文**: 可被越狱设备或备份提取工具读取。

### 5.3 输入验证

- ✅ Agent IPC 协议使用 `1MB` 最大响应限制
- ✅ WSS 消息使用 `StreamReadConstraints` 限制嵌套深度和字符串长度
- ⚠️ `SessionService` 私有 ObjectMapper 绕过这些限制 (M3)
- ⚠️ Agent 使用 `unwrap_or("")` 对缺失字段容忍度不一致

---

## 6. 容错与弹性

### 6.1 重连机制

| 组件 | 策略 | 问题 |
|------|------|------|
| kn-agent WSS | 指数退避 (1s → 60s max) | 成功连接后不重置计数器 (L1) |
| kn-ios WSS | 指数退避 (1s → 30s max) | 函数正确但语义混淆 |
| kn-desktop → agent IPC | 无重试（立即反馈离线） | 适合本地 IPC 场景 |

### 6.2 降级策略

| 场景 | 当前行为 | 评估 |
|------|---------|------|
| Redis 宕机 (邮件频率) | fail-open (放行) | ❌ 应 fail-closed |
| Redis 宕机 (速率限制) | 本地 ConcurrentHashMap fallback | ⚠️ 本地/Redis 状态不同步 |
| Agent 离线 (start_session) | 返回 ErrorCode 通知 iOS | ✅ 正确 |
| WSS 断开 (主循环) | 自动重连 + 状态迁移 | ⚠️ Reconnecting→Unbound 竞态 |

### 6.3 崩溃恢复

- **kn-agent**: `state.rs` 有 crash_counter（max 5 次/5min），超过后停止重启。`cli_heartbeat` 每 15s 上报 session 状态。
- **kn-cloud**: Redis 作为跨节点共享状态，但无跨实例 session 转移（agent 断开后 session 终止）。
- **评估**: 整体合理，符合"agent 是会话唯一宿主"的设计假设。

### 6.4 超时处理

| 超时场景 | 超时值 | 处理方式 |
|---------|--------|---------|
| Agent IPC 读写 | 5s | 返回错误 |
| 绑定码轮询 | 300s TTL | 超时返回 `BindTimeout` |
| iOS start_session | 30s | 启动超时 + 取消 |
| iOS terminal resize | 600ms | CheckedContinuation 超时 |

---

## 7. 协议一致性

### 7.1 WSS Message Envelope 格式

设计文档定义的格式：
```json
{
  "type": "message_type",
  "ts": "ISO8601",
  "data": { /* payload */ }
}
```

**实际不一致**:

| 消息 | session 字段位置 |
|------|-----------------|
| `output` | `data.to_session_id` |
| `session_created` | `data.sessionId` |
| `session_ended` | `data.sessionId` |
| `SessionCoordinator.sendSessionEventToUser` | **顶层** `sessionId` |
| `start_session` 响应 | `data.sessionNid` |

### 7.2 iOS 的过度补偿

为了处理这种不一致，iOS 定义了 5 种解析策略（`WsData.sessionId(from:fallbackToSessionId:)`）：
1. 顶层 `sessionId` (Long → String → nil)
2. `data.sessionId` (Long → String)
3. `data.to_session_id` (String)
4. `data.sessionNid` (String)
5. `data.fallbackToSessionId` (String)

### 7.3 修复优先级

1. **kn-cloud**: `sendSessionEventToUser` 改为使用标准 envelope 格式（字段放入 `data`）
2. **kn-cloud**: 统一 session 字段名为 `sessionId`（或统一为 `sessionNid`），不要混用 `sessionId`/`to_session_id`/`sessionNid`
3. **kn-ios**: 协议统一后，简化为 1-2 种解析策略

---

## 8. 改进建议行动计划（含修复状态）

> **状态更新**: 2026-06-26 — 本轮修复主要完成 kn-agent 架构拆分 + kn-cloud 安全/协议统一。

### 第一优先级（本周修复）

| # | 问题 | 仓库 | 影响 | 状态 |
|---|------|------|------|------|
| 1 | 去掉生产环境 JWT secret 默认值 | kn-cloud | Critical security | ✅ 已修复 |
| 2 | 修复 WSS 协议测试断言 (`42` → `"s_abc"`) | kn-agent | 测试永远失败 | ✅ 已修复 |
| 3 | 合并 Ctrl+C 处理器的两次 session 查询 | kn-agent | 竞态窗口 | ✅ 已修复 |
| 4 | 统一 WSS envelope 格式 | kn-cloud + kn-ios | 协议一致性 | ✅ 已修复 |

### 第二优先级（本迭代修复）

| # | 问题 | 仓库 | 状态 |
|---|------|------|------|
| 5 | 提取会员逻辑到 common 模块 | kn-cloud | ✅ 已修复 |
| 6 | SessionService 注入全局 ObjectMapper | kn-cloud | ✅ 已修复 |
| 7 | `append_log_static` 使用持久化 log_size | kn-agent | ✅ 已修复 |
| 8 | 删除 proto.rs 重复的 `session_interrupted` | kn-agent | ✅ 已修复 |
| 9 | 修复 refreshToken userId=0 | kn-ios | ⚠️ 部分修复 |
| 10 | Relay poller 改用递归 setTimeout | kn-desktop | ✅ 已修复 |

### 第三优先级（架构改进）

| # | 问题 | 仓库 | 状态 |
|---|------|------|------|
| 11 | 拆分 `session.rs` (1,259 行) 为多个模块 | kn-agent | ✅ 已完成 |
| 12 | 拆分 `main.rs` (986 行) 为 handler 模块 | kn-agent | ✅ 已完成 |
| 13 | 拆分 `useTerminal.ts` (1,006 行) 提取 `usePaneTree` | kn-desktop | ❌ 未开始 |
| 14 | App.tsx 引入 React Context 减少 prop drilling | kn-desktop | ❌ 未开始 |
| 15 | 实现 `restore_codex_auth()` 或移除调用 | kn-agent | ✅ 已完成（已移除） |
| 16 | Outgoing channel 引入类型化 enum | kn-agent | ✅ 已完成 |

### 第四优先级（长期优化）

| # | 问题 | 仓库 | 状态 |
|---|------|------|------|
| 17 | WSS 重连成功后重置退避计数器 | kn-agent | ✅ 已修复 |
| 18 | 修复 TOCTOU 路径检查（使用解析后路径） | kn-desktop | ❌ 未修复 |
| 19 | 跨节点 start_session 支持 | kn-cloud | ❌ 未修复 |
| 20 | iOS 证书 pinning | kn-ios | ❌ 未修复 |
| 21 | MailService fail-open → fail-closed | kn-cloud | ✅ 已修复 |

### 未完成项汇总

| # | 问题 | 仓库 | 优先级 |
|---|------|------|--------|
| H4 | TOCTOU 路径检查 | kn-desktop | High |
| M4 | 跨节点 start_session | kn-cloud | Medium |
| M6 | refreshToken userId fallback=0 | kn-ios | Medium |
| L5 | CSP 被禁用 | kn-desktop | Low |
| L6 | scan_system_configs TOML 解析 | kn-desktop | Low |
| L7 | iOS @unchecked Sendable | kn-ios | Low |
| #13 | 拆分 useTerminal.ts | kn-desktop | 架构 |
| #14 | App.tsx React Context | kn-desktop | 架构 |
| #19 | iOS 证书 pinning | kn-ios | 长期 |
| — | 拆分 commands.rs (1,527 行) | kn-desktop | 架构 |
| — | iOS WsData 5 层 fallback 简化 | kn-ios | 架构 | |

---

## 9. 附录

### 9.1 kn-cloud Redis Key 设计

| Key Pattern | 用途 | TTL |
|-------------|------|-----|
| `ws:agent:{machineId}` | Agent 连接信息 (JSON) | 随连接生命周期 |
| `ws:user:{userId}` | User 连接信息 (JSON) | 随连接生命周期 |
| `ws:relay:{wsNodeId}` | 跨节点消息中继 (Pub/Sub) | 即时消费 |
| `ws:pong:{agentId}` | Agent 心跳时间戳 | 30s |
| `ws:user_pong:{connectionId}` | iOS 心跳时间戳 | 60s |
| `session:{sessionNid}` | Session 状态 (Hash) | 7 天 |
| `session:pending:{sessionNid}` | 待确认 session (Hash) | 60s |
| `bind:code:{code}` | 设备绑定码 | 300s |
| `rate:login:{identifier}` | 登录速率限制 | 15min |
| `usage:token:{userId}:{date}` | Token 用量 (Hash) | 永久 |

### 9.2 WSS 消息类型汇总

| 类型 | 方向 | 说明 |
|------|------|------|
| `connected` | CS → Client | 连接确认 + 协议版本 |
| `start_session` | Client → CS | 请求创建远程会话 |
| `start_session_ack` | CS → Client | 已转发给 Agent |
| `session_created` | CS → Client | Agent 已创建 PTY session |
| `session_ended` | CS ↔ Agent/Client | 会话结束通知 |
| `output` | Agent → CS → Client | PTY 输出数据 |
| `input` | Client → CS → Agent | 终端键盘输入 |
| `ctrl` | Client → CS → Agent | 控制信号 (ctrl_c/d/z) |
| `resize` | Client → CS → Agent | 终端尺寸变更 |
| `session_list` | Client → CS | 请求 session 列表 |
| `session_list_resp` | CS → Client | Session 列表响应 |
| `cli_heartbeat` | Agent → CS | CLI 进程心跳 (15s) |
| `session_interrupted` | Agent → CS | 崩溃恢复上报 |
| `error_notify` | Agent → CS → Client | 错误通知 |
| `heartbeat` | Bidirectional | 心跳 (30s 间隔) |
| `pong` | Bidirectional | 心跳响应 |
| `profile_list` | Agent → CS | 可用 Profile 列表 |
| `project_list` | Agent → CS | 项目列表 |

### 9.3 文件行数统计（审查时 → 当前）

| 文件 | 审查时 | 当前 | 变化 | 语言 |
|------|--------|------|------|------|
| `agent/src/session.rs` | 1,259 | **已删除** | 拆分为 7 个文件 | Rust |
| `agent/src/session/manager.rs` | — | 新建 | 核心 session 管理 | Rust |
| `agent/src/session/output.rs` | — | 新建 | 输出扇出 + 日志 | Rust |
| `agent/src/session/input.rs` | — | 新建 | 输入合并 | Rust |
| `agent/src/session/env.rs` | — | 新建 | 环境变量处理 | Rust |
| `agent/src/session/store.rs` | — | 新建 | 持久化存储 | Rust |
| `agent/src/session/types.rs` | — | 新建 | 类型定义 | Rust |
| `agent/src/session/mod.rs` | — | 新建 | 模块入口 | Rust |
| `agent/src/main.rs` | 986 | **309** | -677 行 (拆分为 handler/project/heartbeat/logging) | Rust |
| `agent/src/handler.rs` | — | 新建 | 消息路由处理 | Rust |
| `agent/src/heartbeat.rs` | — | 新建 | CLI 心跳循环 | Rust |
| `agent/src/project.rs` | — | 新建 | 项目文件监听 | Rust |
| `agent/src/logging.rs` | — | 新建 | 日志初始化 | Rust |
| `agent/src/proto.rs` | 622 | **709** | +87 行 (新增 OutgoingMessage enum) | Rust |
| `agent/tests/wss_protocol_test.rs` | ~130 | **341** | 测试重写 | Rust |
| `agent/tests/integration_test.rs` | 1,283 | — | 未变 | Rust |
| `agent/src/ipc.rs` | 1,228 | — | 未变 | Rust |
| `desktop/src/App.tsx` | 2,471 | 2,470 | 未变 | TypeScript |
| `desktop/src/hooks/useTerminal.ts` | 1,006 | 1,001 | 未变 | TypeScript |
| `desktop/src-tauri/src/commands.rs` | 1,528 | 1,527 | 未变 | Rust |
| `desktop/src-tauri/src/usage.rs` | 783 | — | 未变 | Rust |
| `desktop/src-tauri/src/agent_manager.rs` | 848 | — | 未变 | Rust |
| `kn-cloud-ws/.../SessionCoordinator.java` | ~550 | 602 | +~50 行 | Java |
| `kn-cloud-ws/.../KnWsHandler.java` | ~550 | — | 未变 | Java |
| `kn-cloud-common/.../MembershipChecker.java` | — | 新建 | 提取的共享会员逻辑 | Java |
| `kn-ios/.../WebSocketTransport.swift` | ~570 | — | 未变 | Swift |
| `kn-ios/.../TerminalViewModel.swift` | ~500 | — | 未变 | Swift |

### 9.4 审查方法论

本次审查通过以下方法进行：

1. **自动扫描**: 3 个并行 Explore agent 分别审查 kn-agent、kn-desktop、kn-cloud+kn-ios
2. **交叉验证**: 主线程逐行验证所有 Critical/High 级别的发现
3. **跨仓库对照**: 追踪 WSS 消息从 agent → cloud → iOS 的完整链路，识别格式差异
4. **静态分析**: 检查 Rust borrow/lifetime 正确性、Java 并发安全性、Swift actor 隔离

---

> **报告生成**: Claude Code 审查 + 人工验证  
> **审查人员**: 全栈技术专家 (AI)  
> **下次审查建议**: 第一优先级修复完成后重新审查 WSS 协议一致性
