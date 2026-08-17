# 全链路架构审查报告

> 审查日期：2026-06-27
> 审查范围：kn (agent + desktop + common)、kn-cloud (api + ws + common)、kn-ios
> 审查方法：逐文件代码阅读 + 数据流链路追踪

---

## 一、系统架构概览

```
┌──────────┐   WSS    ┌──────────────┐   WSS    ┌─────────────┐  Unix Socket  ┌──────────────┐
│  kn-ios  │ ◄──────► │  kn-cloud    │ ◄──────► │   kn-agent  │ ◄────────────► │ Tauri Desktop│
│  (Swift) │          │  (Java 21)   │          │   (Rust)    │     IPC        │  (Rust+TS)   │
└──────────┘          └──────┬───────┘          └─────────────┘                └──────────────┘
                             │                                                        │
                        ┌────┴────┐                                              ┌────┴────┐
                        │ MySQL   │                                              │ ~/.kn/  │
                        │ Redis   │                                              │ config  │
                        └─────────┘                                              └─────────┘
```

### 关键数据流

| 方向 | 路径 |
|------|------|
| iOS → Agent 输入 | `iOS WSS` → `kn-cloud KnWsHandler` → `MessageRelayService` → `Agent WSS` → `handle_incoming` → `InputMerger` → PTY stdin |
| Agent → iOS 输出 | PTY stdout → `OutputFanout.broadcast()` → `flush_chunked()` → WSS → `kn-cloud handleOutput` → `MessageRelayService` → iOS WSS |
| Desktop → Agent | Tauri `invoke("agent_ipc")` → Unix Socket → `IpcServer` → `SessionManager` |
| 设备绑定 | Desktop QR → iOS 扫码 → `POST /bind-confirm` → Redis bind code → Agent 轮询 `GET /bind-result` |

---

## 二、P0 级问题（Critical — 数据完整性/安全/崩溃）

### P0-1: Agent `handleStartSession` 重复的会话数检查（已修复 ✅）

**文件**: `agent/src/main.rs`（已修复）+ `agent/src/session/manager.rs` 第 62-72 行

**原始分析（已更正）**: ~~在调用 `create()` 之前先用 `active_count()` 检查，存在 TOCTOU 竞态。~~

**更正后的结论**: `SessionManager::create()` 在第 62 行获取 `create_mutex`，第 66-67 行在锁内重新检查 count，count+insert 是原子的。并发请求即使同时通过外部检查，`create()` 内部也会正确拒绝超限请求。**外部预检查是多余的**，不会导致突破限制，但会造成以下问题：
1. 代码重复：限制逻辑分散在两处（调用方 + `create()` 内部）
2. 错误处理不一致：外部检查的 `error_notify` 格式和 `create()` 返回的 `SessionLimit` 错误走不同路径
3. `active_count` 查询失败时"放行"，绕过了明确的限制检查

**已实施的修复**:
1. 删除了 `handleStartSession` 中的冗余预检查（原第 597-613 行）
2. `create()` 返回 `SessionLimit` 时，新增向云端发送 `error_notify` 的逻辑
3. 添加了 `AgentError` 的显式 import

```rust
// 修复后: 限制检查只在 create() 内部做，count+insert 原子
Err(e) => {
    tracing::error!(error = %e, "创建会话失败");
    if let AgentError::SessionLimit { current, max } = &e {
        let err = proto::WsMessageBuilder::error_notify(
            "SESSION_LIMIT",
            &format!("Agent 会话数已满 ({}/{}), 请关闭之前的会话", current, max),
        );
        if let Some(tx) = outgoing.lock().await.as_ref() {
            let _ = tx.send(err);
        }
    }
}
```

---

### P0-2: kn-cloud `handleStartSession` 会话数限制 TOCTOU（已修复 ✅）

**文件**: `kn-cloud-ws/.../handler/SessionCoordinator.java`

**原始分析**: `ZCOUNT` 检查活跃会话数和 `SETNX` 获取创建锁之间存在竞态窗口——两个并发请求可能同时看到 count=9 并各自获取锁。

**更正**: 与 P0-1 类似，`handleSessionCreated` 在 ZADD 后会重新检查 count（desktop 路径），且 SETNX 锁自身防止并发创建。**但**跨节点场景下锁在 `handleStartSession` 中被提前释放（旧代码 line 174），导致第二个请求可以在第一个 session 的 ZADD 之前获取锁，**可能突破限制**。

**已实施的修复**:
1. 创建 Lua 脚本 `lua/start_session_lock.lua` — 原子执行 ZCOUNT + SETNX
2. 锁 TTL 从 10s 提升到 30s（覆盖 Agent PTY 启动时间）
3. 跨节点场景不再提前释放锁 — 由远端节点 `handleSessionCreated` 在 ZADD 后释放
4. 新增 `DefaultRedisScript` 字段，复用导入（替换 `endSessionInRedis` 中的全限定名）

---

### P0-3: Agent `kill_session` 使用 `unsafe libc::kill` PID 重用风险（已修复 ✅）

**文件**: `agent/src/session/manager.rs`

**原始问题**: PTY 子进程退出后 PID 可能被 OS 回收重用，`kill_session` 盲目发送 SIGKILL 可能误杀不相关进程。

**已实施的修复**（两层防护）:
1. **`kill_session` 验活**: 发 SIGKILL 前用 `kill(pid, 0)` 检查进程是否仍存活。若已退出（`ESRCH`），跳过 SIGKILL
2. **进程退出即清理**: `spawn_blocking` 中 `child.wait()` 返回后，立即从 `child_pids` 中移除 PID。正常退出路径根本不会走到 `kill_session`

```rust
// 修复后 — kill_session 中两段式安全检查
if let Some(pid) = self.child_pids.lock().await.remove(nid) {
    // remove 语义：PID 仅取出一次，杜绝重复 kill
    unsafe {
        if libc::kill(pid as i32, 0) == 0 {  // 验活
            libc::kill(pid as i32, libc::SIGKILL);
        } else {
            tracing::info!(pid = pid, "进程已退出，跳过 SIGKILL（避免 PID 重用）");
        }
    }
}

// spawn_blocking 中 — 进程退出立即清理
child.wait()...;
self_for_pid_cleanup.child_pids.blocking_lock().remove(&cleanup_nid);
```

**残余风险**: `kill(pid, 0)` 和 `kill(pid, SIGKILL)` 之间有微秒级窗口，理论上 PID 可在此间隙被回收。实际极低概率（需在微秒内 PID 回绕）。彻底方案需进程组（setpgid），留待后续优化。

---

### P0-4: Agent WSS 客户端静默丢弃 Binary 帧

**文件**: `agent/src/ws_client.rs` 第 271 行

**问题**: WebSocket read loop 中 `Message::Binary` 被通配分支 `_ => {}` 静默丢弃。如果服务端或中间代理发送 binary 帧，数据将完全丢失且无日志。

```rust
// ws_client.rs:271
_ => {}  // Binary 帧、Ping/Pong 帧都被静默丢弃
```

**修复建议**: 至少对 Binary 帧记录 warn 日志。如果协议约定不使用 binary，应在收到时主动关闭连接（kn-cloud 的 `handleBinaryMessage` 就是如此处理的）。

---

### P0-5: kn-cloud `drainPendingMessages` 非原子操作导致消息丢失（已修复 ✅）

**文件**: `kn-cloud-ws/.../service/MessageRelayService.java`

**原始问题**: `LRANGE` 全量读取 → 逐条发送 → `DEL` 删除 key。Agent 中途断连时，已读未发的消息随 `DEL` 永久丢失。

**已实施的修复**: 改用 `LPOP` 逐条原子弹出。发送失败时将消息 `LPUSH` 回队列头部。

```java
// 修复后 — LPOP 逐条原子弹出
while (agentSession.isOpen()) {
    String msgJson = redis.opsForList().leftPop(key);  // 原子弹出
    if (msgJson == null) break;
    if (sender.send(agentSession, msgJson)) {
        delivered++;
    } else {
        redis.opsForList().leftPush(key, msgJson);  // 失败回推
        break;
    }
}
// 不再 DEL key — 未消费的消息自然保留在 Redis
```

**关键属性**:
- 断连时已弹出的消息已成功发送（`delivered++`），未弹出的保留在 Redis
- 发送失败的消息回推到队列头部，下次 drain 或另一节点可重试
- 不需要 DEL key — 队列为空时自然消失（7 天 TTL），无残留问题

---

### P0-6: kn-cloud `buildMessageEntry` JSON 手工拼接（已修复 ✅，降为 P2）

**文件**: `kn-cloud-ws/.../service/MessageRelayService.java`

**重新评估**: 功能是会话消息历史记录（iOS 查看），消费端 `parseMessageEntry` 有 catch + filter 保护，坏条目只影响一条 UI 记录，不丢业务数据、不崩溃。实际触发需用户输入含 `\` 字符。降为 P2。

**已实施的修复**: 用 Jackson `ObjectNode` 替换 `String.format` 手拼。

```java
// 修复前: 手拼 JSON，\ 等特殊字符会生成非法 JSON
String.format("{\"seq\":%d,...,\"preview\":\"%s\"}",
        ..., preview(content).replace("\"", "\\\"").replace("\n", "\\n"));

// 修复后: Jackson 自动处理所有转义
var node = sender.getMapper().createObjectNode();
node.put("seq", seq);
node.put("preview", preview(content));
return node.toString();
```

---

## 三、P1 级问题（High — 可靠性/安全）

### P1-1: kn-cloud `bindConfirm` 无频率限制，6 位验证码可被暴力破解

**文件**: `kn-cloud-api/.../controller/DeviceController.java` 第 101-105 行

**问题**: 6 位数字验证码只有 100 万种可能，300 秒 TTL。已认证用户可以无限制调用 `bind-confirm`，如果攻击者获得有效 JWT，可以每秒尝试数千次，在几十分钟内暴力破解。**注意**：攻击前提是拥有有效 JWT（已登录用户），但缺少 per-user 频率限制仍是安全弱点。

**修复建议**: 使用 `LoginRateLimiter` 对 `bind-confirm` 端点添加 per-user 频率限制（如每分钟最多 10 次尝试，连续 5 次失败后锁定）。

---

### P1-2: Agent `OutputFanout.flush_chunked` 在 PTY 读取线程同步执行，可能阻塞数据读取

**文件**: `agent/src/session/output.rs` 第 168-185 行 (broadcast) + 第 190-235 行 (flush_chunked)

**问题**: `broadcast()` 在 `spawn_blocking` 线程中由 PTY reader 调用。当缓冲区达到 64KB 阈值时，直接在当前线程同步调用 `flush_chunked()`，而 `flush_chunked` 内部会：
1. 写入环形日志（文件 I/O）
2. 逐 10KB 分块发送到 WSS channel
3. 发送到 IPC channel

在高输出场景（如 cat 大文件），这个同步 flush 会阻塞 PTY reader 读取下一批数据。

```rust
// output.rs:174-185 — 在 spawn_blocking 线程中同步 flush
if buf_len >= 64 * 1024 {
    let data = std::mem::take(&mut *buf);
    drop(buf);
    Self::flush_chunked(...); // 同步执行，阻塞 PTY 读取
}
```

**修复建议**: 将 64KB 阈值触发也改为异步——把数据发送到 channel，由独立的 flush task 处理。或者将 `flush_chunked` 中的文件 I/O 移到 `spawn_blocking` 中。

---

### P1-3: kn-cloud Redis Pub/Sub 跨节点消息投递是 fire-and-forget

**文件**: `kn-cloud-ws/.../handler/SessionCoordinator.java` 第 560 行、`MessageRelayService.java` 第 288 行

**问题**: 使用 Redis `convertAndSend` 进行跨节点消息中继（`session_ended`、`output` 等），这是 fire-and-forget 模式——如果目标节点刚好重启或 Redis 订阅中断，消息将静默丢失。对于 `session_ended` 这类关键事件，丢失意味着 iOS 用户永远看不到会话结束。

```java
// SessionCoordinator.java:594
redis.convertAndSend(RedisKeys.wsRelay(targetNode), relay.toString());
// 如果目标节点不在线，消息直接丢弃，发送方不感知
```

**修复建议**: 
1. 对于关键事件（`session_ended`、`error_notify`），使用 Redis List 作为 fallback 队列
2. 或者在 Redis Hash 中标记事件，由 SessionHeartbeatMonitor 在下次扫描时补推

---

### P1-4: iOS `writeANSIImmediately` 字符串插值存在 JS 注入风险

**文件**: `kn-ios/Presentation/Terminal/TerminalView.swift`

**问题**: ANSI 数据通过字符串插值直接拼接到 JavaScript 调用中。虽然 base64 编码大大降低了风险，但如果 base64 字符串中意外包含 `'` 字符（base64 不含此字符，但代码未做防御），会破坏 JS 语法。

```swift
// TerminalView.swift
webView.evaluateJavaScript("window.writeANSIBase64('\(base64)')")
```

**修复建议**: 改用 `WKWebView.callAsyncJavaScript` 并正确传递参数，避免字符串插值。或至少在使用前验证 base64 字符串不包含单引号。

---

### P1-5: Agent `start_session` 失败时 `ipc_rx` channel 未被消费，造成资源泄漏

**文件**: `agent/src/main.rs` 第 647-673 行

**问题**: 在 `handle_incoming` → `StartSession` 分支中，创建了 `(wss_tx, wss_rx)` 和 `(ipc_tx, ipc_rx)` 两对 channel。当 `start_session` 失败时（第 693 行 error 分支），`ipc_rx` 对应的消费 task 已经 spawn 但 receive loop 永远不会结束——因为 `ipc_tx` 被 move 进了 `start_session`，失败后 `ipc_tx` 被 drop，`ipc_rx` 会自然结束。**实际检查**：`ipc_tx` 通过 `start_session` 传递给了 `OutputFanout::new()`，如果 `start_session` 在 `OutputFanout::new()` 之前失败（第 362-395 行的 tool/env/openpty 阶段），`ipc_tx` 被 drop，`ipc_rx` 的消费 task 会正常退出。**但如果失败在 `OutputFanout::new()` 之后**，`ipc_tx` 被 fanout 持有，消费 task 永远不会退出。

**修复建议**: 在 error 分支中显式 drop `ipc_tx`（如果仍持有），确保消费 task 能正常退出。

---

### P1-6: iOS `pendingAcks` Continuation 泄漏

**文件**: `kn-ios/Data/Network/WebSocketTransport.swift` 第 164-197 行

**问题**: `sendOnce` 方法使用 `withCheckedThrowingContinuation` 等待 ack。如果 WSS 连接在 `task.send` 回调触发后、ack 到达前断开：
- `handleDisconnect` 会遍历 `pendingAcks` 清理
- 但如果在 `task.send` 成功和 `handleDisconnect` 之间有一个微小的竞态窗口，continuation 可能既不被 send 的 error 回调 resume，也不被 `handleDisconnect` resume

虽然有 30 秒超时兜底，但在边界情况下仍可能造成短暂泄漏。

**修复建议**: 在 `handleDisconnect` 和 30s 超时任务之外，增加 deinit 时的兜底清理。

---

### P1-7: kn-cloud `ws:user` TTL 不一致

**文件**: `kn-cloud-ws/.../service/ConnectionService.java` 第 285 行 vs `KnWsHandler.java` 第 434 行

**问题**: `connectUser` 设置 `ws:user:{userId}` 的 TTL 为 7 天，但 `refreshRedisTTL` 在每次心跳时将其重置为 90 秒。结果是在用户连接期间，TTL 在 90s 和 7d 之间反复震荡。虽然不影响正常功能（90s > 心跳间隔 30s），但语义不清晰，且如果心跳出现延迟（>90s），key 可能被错误地过期。

**修复建议**: 统一 TTL 策略：心跳刷新用 90s，初始设置也用 90s。如果用户断开，key 在 90s 后自动清理即可。

---

## 四、P2 级问题（Medium — 边界条件/代码质量）

### P2-1: Agent `handle_detach` 是空实现（stub）

**文件**: `agent/src/ipc.rs`（handle_detach 方法）

**问题**: IPC 的 `detach` 方法只检查会话是否存在，然后返回 `ok`，不执行任何实际操作。这意味着桌面端通过 IPC 发起的 PTY 接管请求不会生效。

**修复建议**: 如果此功能暂不需要，至少返回明确的 "not implemented" 错误而不是静默返回成功。如果未来需要，补充实际实现。

---

### P2-2: kn-cloud `PendingSessionScheduler` 扫描已废弃的 Redis key

**文件**: `kn-cloud-api/.../service/PendingSessionScheduler.java`

**问题**: 该定时任务每 60 秒 SCAN `session:pending:*` 模式的 key。代码注释和类注解均标记为 `@Deprecated`，表示新数据已不再写入这些 key，但清理任务仍在运行。在 Redis 实例上产生无意义的 SCAN 负载。

**修复建议**: 确认无旧数据残留后移除该定时任务。

---

### P2-3: iOS 硬编码开发者邮箱

**文件**: `kn-ios/Presentation/Auth/AuthViewModel.swift`

**问题**: `AuthViewModel` 中 `email` 的默认值为 `"zhaojun2066@gmail.com"`，这是真实的开发者邮箱，不应出现在生产代码中。

```swift
// AuthViewModel.swift
var email = "zhaojun2066@gmail.com"  // ← 应改为空字符串
```

**修复建议**: 改为 `""` 或使用 `@AppStorage` 保存"上次登录邮箱"作为默认值。

---

### P2-4: iOS `TerminalViewModel` 疑似死代码

**文件**: `kn-ios/Presentation/Terminal/TerminalViewModel.swift`（434 行）

**问题**: `TerminalViewModel` 是早期的单会话终端 ViewModel。已被新的 `TerminalTabManager`（支持多标签页）替代。在 `KnApp.swift` 中没有找到对 `TerminalViewModel` 的引用，但代码仍被编译进 target。

**修复建议**: 确认无引用后移除，或标记为 `@available(*, deprecated)` 以便在下一个大版本清理。

---

### P2-5: ~~kn-cloud `SessionService` 全量读取后内存分页~~ 非问题

`end_session.lua` 每次结束时裁剪 ZSet 到 30 条，活跃会话限制 10 条，所以最坏情况 `listSessions` 只读 30 条。内存分页没问题。

---

### P2-6: iOS `SendCtrlUseCase.nextSeqs` 永不清除

**文件**: `kn-ios/Domain/UseCases/SendCtrlUseCase.swift`

**问题**: 用于跟踪 ctrl 消息序列号的 `nextSeqs` 字典，在会话结束后不会清理对应的 entry。对于长时间使用的 App，这个字典会持续增长（虽然每个 session 只存一个 Int，泄漏量很小）。

**修复建议**: 在收到 `session_ended` 时清理对应 session 的 seq 记录。

---

## 五、P3 级问题（Low — 改善建议）

### P3-1: kn-cloud 数据库无外键约束

**文件**: `kn-cloud/deploy/init.sql`

**问题**: `kn_device.user_id`、`kn_session.user_id`、`kn_message.session_id` 等字段在应用层面维护引用完整性，但数据库没有 `FOREIGN KEY` 约束。如果应用逻辑有 bug，可能产生孤儿记录。

**影响**: 低——当前代码在所有写入路径都通过 Service 层保证一致性，无已知的孤儿记录问题。

---

### P3-2: iOS 测试覆盖率极低

**文件**: `kn-ios/knTests/knTests.swift`

**现状**: 仅 12 个单元测试，覆盖 `DomainError`、`LoginUseCase`、`RegisterUseCase`、`RedeemCodeUseCase`、`SendInputUseCase`。以下关键模块完全没有测试：
- `WebSocketTransport`（重连、ack 跟踪、消息编解码）
- `KeychainTokenStore`
- `TerminalTabManager`（核心的会话管理逻辑）
- 任何 UI 测试

**建议**: 至少为核心网络层和数据层增加单元测试。

---

### P3-3: kn-cloud `kn_device.device_token` VARCHAR(512) 索引过大

**文件**: `kn-cloud/deploy/init.sql`

**问题**: `device_token` 实际是 UUID 格式（36 字符），但列定义为 `VARCHAR(512)`，且建了索引（`idx_device_token`）。对于 MySQL InnoDB，大 VARCHAR 索引占用更多磁盘空间和内存。

**建议**: 将列宽缩小到 `VARCHAR(128)` 或 `VARCHAR(64)`，足够覆盖当前和未来的 token 格式。注意需要同时修改对应的 Java Entity 注解。

---

### P3-4: Agent `LOG_FILE_LOCKS` 静态表条目永不清理

**文件**: `agent/src/session/output.rs` 第 44 行

**问题**: `LOG_FILE_LOCKS` 是静态的 `HashMap<PathBuf, Arc<Mutex<()>>>`，条目在 session 结束后不会被移除。虽然每个 session 只占一个 entry（~100 bytes），但长期运行的 agent 可能会有大量已结束 session 的条目残留。

**建议**: 在 session 结束时从 `LOG_FILE_LOCKS` 中移除对应的 entry。

---

### P3-5: Agent `remote_enabled` 所有新会话默认为 true

**文件**: `agent/src/session/manager.rs` 第 89 行

```rust
remote_enabled: Arc::new(AtomicBool::new(true)),
```

**问题**: 所有通过 IPC（桌面端）创建的会话默认开启了远程控制。如果用户创建了敏感内容的会话，可能在不知情的情况下被远程可见。虽然需要先有 WSS 连接才能远程访问，但默认开启违反了最小权限原则。

**建议**: 将默认值改为 `false`，要求用户显式通过 IP C `set_remote_enabled` 开启。

---

### P3-6: Tauri 桌面前端 `agent_ipc` 同步调用可能阻塞

**文件**: `desktop/src-tauri/src/agent_ipc.rs` 第 62 行

**问题**: Tauri 命令 `agent_ipc` 使用同步的 `UnixStream` I/O（5 秒超时）。在 Tauri v2 中，命令默认在异步线程池执行，不会阻塞主线程。但同步 I/O 本身比异步 I/O 效率低，在高频调用（如状态轮询）时可能产生线程切换开销。

**建议**: 迁移到 `tokio::net::UnixStream` 异步 I/O。

---

## 六、全链路数据流校验

### 6.1 设备绑定链路 ✅

```
Desktop QR 生成 → iOS 扫码 → POST /bind-confirm (JWT) → Redis bind code → DeviceService.bindConfirm()
                                                                │
Agent 轮询 GET /bind-result?code=xxx ← 每 2s ←─────────────────┘
    │
    ▼
save_device_token() → WSS connect (try)
```

**验证结论**: 绑定链路完整。bind code 通过 Redis 传递，Agent 轮询获取 device_token。**已知问题**: P1-1（bindConfirm 无频率限制）。

### 6.2 会话创建链路（iOS → Cloud → Agent）

```
iOS start_session → KnWsHandler → SessionCoordinator.handleStartSession()
    │                                   │
    │                           ┌───────┴────────┐
    │                           │ ZCOUNT 检查     │
    │                           │ SETNX 加锁      │
    │                           │ Agent 在线检查  │
    │                           │ 转发给 Agent    │
    │                           └───────┬────────┘
    │                                   │
    ▼                                   ▼
start_session_ack ←────────────── Agent 收到 → create() → start_session()
                                                        │
iOS 收到 session_created ←──── Cloud ←─────────────────┘
```

**验证结论**: 链路完整。**已知问题**: P0-1（Agent 端 TOCTOU）、P0-2（Cloud 端 TOCTOU）。

### 6.3 输入/输出链路

```
iOS input → KnWsHandler → 去重(Redis SETNX) → MessageRelayService
                                                    │
                                    ┌───────────────┼───────────────┐
                                    │ 本节点Agent   │ 跨节点Pub/Sub │ 离线buffer   │
                                    ▼               ▼               ▼
                                Agent WSS      Redis relay      Redis List
                                    │
                                    ▼
                            handle_incoming() → InputMerger → PTY stdin

PTY stdout → OutputFanout.broadcast() → 100ms/64KB flush → WSS →
    │
    ▼
KnWsHandler.handleOutput() → MessageRelayService → iOS WSS
```

**验证结论**: 链路完整，有去重、离线缓冲、跨节点中继三层保障。**已知问题**: P0-4（Binary 帧丢弃）、P1-2（同步 flush 阻塞）、P1-3（fire-and-forget 跨节点 relay）。

### 6.4 桌面端本地会话链路

```
Tauri 前端 → invoke("agent_ipc") → Unix Socket → IpcServer
                                                      │
                                    ┌─────────────────┼─────────────────┐
                                    │ new_session     │ status/sessions │ ...
                                    ▼                 ▼
                              SessionManager      state machine
                                    │
                                    ▼
                              PTY spawn → OutputFanout → IPC channel → 前端 XTerm
```

**验证结论**: 链路完整。桌面端通过 IPC 独立于 WSS 连接，即使云端不可达也能本地操作。

---

## 七、优点总结

在审计过程中也发现了很多做得好的地方：

| 方面 | 亮点 |
|------|------|
| **安全** | device_token 原子写入 (tmp+fsync+rename 0600)、AES-256-GCM 配置加密、delete_device_token 硬安全阀保护生产路径、WSS 消息级别 role 白名单、设备归属校验防 IDOR |
| **可靠性** | WSS 指数退避重连 (带抖动)、session_ended 幂等上报 (AtomicBool)、3 代配置备份轮转、跨进程文件锁 (fs2)、Redis 不可用时的 graceful degradation、错误计数达上限自动断连 |
| **数据一致性** | Lua CAS Refresh Token Rotation、end_session.lua 原子结束会话、Redis 去重 (SETNX)、幂等 session_created 检查 |
| **协议设计** | 两阶段会话创建 (start_session_ack → session_created)、ACK 机制 (delivered/failed/duplicate/dropped)、CLI 心跳监测死会话、环形日志支持会话恢复 replay |
| **代码质量** | 状态机穷举转换表编译期检查、CancellationToken 模式优雅关闭、IPC binder generation-gated 防竞态、KnWsHandler 清晰的消息分发架构、iOS 统一 DTO→Entity 映射 |

---

## 八、优先级汇总

| 优先级 | 编号 | 问题 | 影响 |
|--------|------|------|------|
| **P0** | P0-1 | ~~Agent 会话数 TOCTOU 竞态~~ ✅ 已修复 | 冗余检查已删除，create() 已正确保护 |
| **P0** | P0-2 | ~~Cloud 会话数 TOCTOU 竞态~~ ✅ 已修复 | Lua 脚本原子化 ZCOUNT+SETNX |
| **P0** | P0-3 | ~~kill_session PID 重用~~ ✅ 已修复 | 两层防护：验活 + 进程退出即清理 |
| **P0** | P0-4 | Binary 帧静默丢弃 | 数据丢失 |
| **P0** | P0-5 | ~~drainPendingMessages 非原子~~ ✅ 已修复 | LPOP 逐条原子弹出 + 失败回推 |
| **P2** | P0-6 | ~~JSON 手工拼接~~ ✅ 已修复，降 P2 | Jackson ObjectNode 消除手拼 |
| **P1** | P1-1 | bindConfirm 无频率限制 | 已认证用户暴力破解绑定码 |
| **P1** | P1-2 | OutputFanout 同步 flush 阻塞 | 高输出场景阻塞 PTY 读取 |
| **P1** | P1-3 | Redis Pub/Sub fire-and-forget | 跨节点关键消息丢失 |
| **P1** | P1-4 | iOS JS 字符串插值 | 边界情况 JS 注入（低概率） |
| **P1** | P1-5 | start_session 失败 ipc_rx 泄漏 | 资源泄漏（长时间运行） |
| **P1** | P1-6 | iOS pendingAcks 泄漏 | 边界情况内存泄漏 |
| **P1** | P1-7 | ws:user TTL 不一致 | Redis key 可能过早过期 |
| **P2** | P2-1 | handle_detach 空实现 | 功能不可用 |
| **P2** | P2-2 | PendingSessionScheduler 死代码 | Redis 无效 SCAN 开销 |
| **P2** | P2-3 | iOS 硬编码开发者邮箱 | 隐私泄露 |
| **P2** | P2-4 | iOS TerminalViewModel 死代码 | 代码膨胀 |
| ~~P2~~ | P2-5 | ~~SessionService 全量内存分页~~ 非问题 | end_session.lua 已裁剪 ZSet 到 30 |
| **P2** | P2-6 | iOS nextSeqs 不清除 | 微小内存泄漏 |
| **P3** | P3-1 | DB 无外键约束 | 极端情况孤儿记录 |
| **P3** | P3-2 | iOS 测试覆盖极低 | 回归风险 |
| **P3** | P3-3 | device_token 索引过大 | 磁盘/内存浪费 |
| **P3** | P3-4 | LOG_FILE_LOCKS 不清除 | 微小内存泄漏 |
| **P3** | P3-5 | remote_enabled 默认 true | 最小权限原则 |
| **P3** | P3-6 | Tauri agent_ipc 同步 I/O | 线程效率 |

---

> **文档版本**: v1.0
> **下次审查建议**: P0 项修复后 1 周内复审；P1 项修复后 2 周内复审；P2/P3 按迭代节奏处理。
