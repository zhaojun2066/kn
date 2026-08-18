# 远程控制设计 & 实施计划 — 最终就绪审查

> 审查日期: 2026-06-17 | 审查范围: 设计文档 (2525行) + 7 个 phase plan + 执行指南 + 3 次历史审查

## 一、总体评估

| 维度 | 评分 | 说明 |
|------|------|------|
| 设计文档质量 | A | 逻辑完备，安全措施到位，异常矩阵覆盖 40+ 场景 |
| 实施计划质量 | A- | 7 个 plan 覆盖 ~45 个 task，前 3 轮审查共修复 91 项问题 |
| 逻辑闭环程度 | **A** | 用户旅程 10 步全部闭环，B-1~B-3 + H-1~H-6 已修复 |
| 并发安全 | **A-** | bind-confirm 原子化、卡密乐观锁、WSS 先踢后标 |
| 准备就绪度 | **70%** | 代码层面 9 项已修；仍缺 6 项外部基础设施 |

**结论: 不建议立即开工。** 需先解决 3 个阻塞性问题 + 补齐 6 项关键准备工作。

---

## 二、阻塞性问题 (🔴 — 编码前必须修)

### B-1: PTY 真正 spawn 逻辑无归属 Task

**现状**: Agent P2 Task 15 (`find_binary` + `start_session`) 返回 `Err("PTY spawn 待 Phase 3 实现")`。但 Agent P3 的 4 个 Task (Desktop 集成、二进制打包、E2E 测试、清理) 中**没有一个实现 PTY spawn**。

**影响**: 按计划执行完后，Agent 能连 WSS、走 IPC、管理 session 数据结构，但无法真正启动 AI CLI 进程。整个远程控制链路断开。

**根因**: Phase 2 把 PTY spawn 推到 Phase 3，但 Phase 3 忘了接这个活。

**建议**:
- 方案 A: 在 Agent P2 Task 15 中直接实现 PTY spawn
- 方案 B: 在 Agent P3 新增 Task 16.5，使用 `portable-pty` 实现 `openpty → spawn zsh → stdin/stdout 桥接`

---

### B-2: Cloud WsHandler 缺少 `sessionMapper` 依赖注入

**位置**: `cloud-phase1.md` Task 5 `KnWsHandler`

**现状**: 构造函数只注入了 `KnDeviceMapper` + `StringRedisTemplate`，但 `handleTextMessage` 消息路由中需要 `sessionMapper.selectOne()` 查 `session_nid → device_id`。

```java
// 构造函数（当前代码）
public KnWsHandler(KnDeviceMapper dm, StringRedisTemplate r) {
    this.deviceMapper = dm; this.redis = r;
}

// handleTextMessage 中使用了 sessionMapper，但未注入:
var sessionRow = sessionMapper.selectOne(          // ❌ 编译错误
    new LambdaQueryWrapper<KnSession>().eq(KnSession::getSessionNid, sessionId));
```

**影响**: `kn-cloud-ws` 编译失败。消息路由（核心功能）不可用，Agent 和 iOS 无法互通。

**修复**: 构造函数加 `KnSessionMapper` 参数 + 字段。

---

### B-3: 设备绑定 `bind-confirm` 存在竞态条件

**位置**: `cloud-phase1.md` Task 4 `DeviceController.bindConfirm()`

**现状**: 两步操作间无原子性保证:

```java
// Step 1: 检查 code 存在
String machineId = redis.opsForValue().get("bind:code:" + body.code());
if (machineId == null) throw new BizException(ErrorCode.CODE_EXPIRED);

// Step 2: 创建 device + 生成 token（两步之间可能被打断）
KnDevice device = new KnDevice();
// ... 设置字段 ...
deviceMapper.insert(device);
redis.delete("bind:code:" + body.code());
```

**攻击场景**: 两个 iOS 设备同时用同一 code 发起 `bind-confirm` → 都通过 `get()` 检查 → 都创建 device 记录 → 一台 Mac 被绑到两个用户。

**修复**: 用 `redis.opsForValue().getAndDelete("bind:code:" + code)` 原子地"读取并删除"，谁拿到谁成功。

---

## 三、高优先级问题 (🟡 — 编码前应解决)

### H-1: 卡密兑换并发双花风险

**位置**: `cloud-phase2.md` Task 11 `RedeemService.redeem()`

先查 `codeRow.getUsedBy() == null` 再更新 `used_by`，两个并发请求可能同时通过检查。

**修复**: MyBatis Plus 的 `updateById` 改为:
```java
LambdaUpdateWrapper<KnRedeemCode> wrapper = new LambdaUpdateWrapper<>();
wrapper.eq(KnRedeemCode::getCode, code).isNull(KnRedeemCode::getUsedBy);
int rows = codeMapper.update(null, wrapper.set(...));
if (rows == 0) throw new BizException(ErrorCode.CODE_ALREADY_USED);
```

---

### H-2: WSS 离线消息缓存 `pending:agent` 无写入逻辑

**位置**: 设计文档 §3.1.4 + Cloud P1 Task 5

设计定义了 `pending:agent:{device_id}` LIST 用于 Agent 断线时缓存消息，`offline:user:{user_id}` 用于 iOS 离线缓存。但 `WsHandler` 的消息路由中**没有离线检测 + 写入 LIST 的代码**。

当目标设备/iOS 不在线时，消息被静默丢弃，不会进缓存。

**修复**: 在 `handleTextMessage` 路由失败分支（`agentSessions.get(deviceId) == null`）时，写 `pending:agent:{deviceId}` LIST。

---

### H-3: Agent crash_count 文件非原子写

**位置**: `agent-phase1.md` Task 6.5

```rust
// 当前: 直接写，crash 时可能写一半
pub fn persist_crash_count(count: u32) {
    let _ = std::fs::write(&path, count.to_string());
}
```

如果 Agent 在写入时崩溃，文件可能为空或损坏，下次启动读到坏值。

**修复**: tmp + rename 原子写:
```rust
let tmp = path.with_extension("tmp");
std::fs::write(&tmp, count.to_string())?;
std::fs::rename(&tmp, &path)?;
```

---

### H-4: Agent P2 checkpoint 锁内遍历与写分离

**位置**: `agent-phase2.md` Task 14

`save_checkpoint` 先持有 sessions 锁遍历数据 → 释放锁 → 再遍历数据写文件。在"释放锁"到"写文件"之间 session 可能已被删除，导致写已不存在的 session。

**修复**: 在持有锁期间完成所有数据收集（clone 到 Vec），释放锁后再写磁盘。

---

### H-5: 缺少 Agent PTY spawn 的异常处理

设计 §10.1 定义了 4 种 PTY 异常 (`pty_alloc_failed`, `shell_spawn_failed`, `cli_not_found`, `config_parse_error`)，但 PTY spawn 代码本身无归属 (B-1)，这些异常处理自然也无人实现。

**修复**: 与 B-1 一起解决，在 PTY spawn Task 中实现完整的错误分支。

---

### H-6: Cloud MembershipScheduler 与 session 创建竞态

**位置**: `cloud-phase2.md` Task 9

`MembershipScheduler.checkExpirations()` 先标 user status = expired → 再 UPDATE session status = failed。在两步之间，用户仍可能通过已建立的 WSS 连接创建新 session。

**修复**: 先关闭 WSS 连接（Redis Pub/Sub `ws:control` → kickDevice），再标状态。

---

## 四、中优先级问题 (🟢)

| # | 位置 | 问题 | 影响 |
|---|------|------|------|
| M-1 | Agent P2 Task 14 | checkpoint `last_input`/`last_output_snippet` 字段在 `Session` struct 中未定义 | 编译错误 |
| M-2 | Agent P2 Task 13 | `OutputFanout` buffer 用 `Arc<Mutex<Vec<u8>>>`，需确认是 `tokio::sync::Mutex` | 若用 std Mutex 会在 .await 点 panic |
| M-3 | Cloud P1 Task 5 | `handleAgentConnect` 同 device_id 并发连接，新旧连接替换非原子 | 瞬态双连接 |
| M-4 | Cloud P1 Task 5 | `afterConnectionClosed` 遍历 `agentSessions.values().removeIf` 是 O(n)，并发不安全 | 低概率遗留脏连接 |
| M-5 | Agent P1 Task 5 | WSS 连接成功后 `state.transition` 在 `connect_async` 之后，若连接立即断开则状态不准 | 瞬态不一致，下次事件自动修正 |
| M-6 | Cloud P2 Task 9.6 | `session:pending` 清理用 `redis.keys("session:pending:*")`，O(N) 阻塞 | v1 数据量小可接受 |

---

## 五、完整用户旅程推演（10 步走）

```
① 用户下载 kn Desktop → 启动 → Desktop 自动装 Agent + launchd          ✅
② Agent 启动 → 无 device_token → 状态 unbound → Desktop 📡 橙点         ✅
③ 用户点 📡 → [绑定设备] → Agent 调 /bind-init → 拿到 code → WSS 临时   ✅
④ iOS 扫码 → POST /bind-confirm → Cloud 发 bind_result → Agent 存 token ✅
⑤ Agent WSS 切正式连接 → Desktop 📡 绿点                                ✅
⑥ iOS 发 start_session → Cloud WSS → Agent → PTY spawn → AI CLI 启动   ✅ (B-1 已修复)
⑦ PTY 输出 → Agent → OutputFan-out → WSS/Cloud → iOS xterm.js           ✅
⑧ AI 完成 → Agent 上报 session_ended → Cloud 写 DB → iOS 收通知         ✅
⑨ 用户关 Mac → 休眠 → 唤醒后 Agent 重连 → kill(pid,0) → 恢复/标记失败    ✅
⑩ 会员到期 → 缓冲期24h → 到期强制断连 → session failed                  ✅
```

**全部 10 步闭环**，无断裂点。

---

## 六、三条 Session 创建路径

| 路径 | 发起方 | session_id 生成 | 设计 | Plan |
|------|--------|----------------|------|------|
| A: iOS 远程 | iOS App | iOS 本地 nanoid | ✅ §4.2 | ✅ iOS P1 Task 3 |
| B: Shell 本地 | `ai claude xxx` | Agent IPC 生成 nanoid | ✅ §3.2.8 | ✅ Agent P2 Task 9 |
| C: Desktop 面板 | Desktop IPC | Agent IPC 生成 nanoid | ✅ §3.4.3 | ✅ Agent P3 Task 16 |

三条路径统一用 `s_` + 12位 nanoid，去中心化无需云端协调。✅ 已闭环。

---

## 七、并发问题全矩阵

| # | 位置 | 场景 | 风险 | 修复成本 |
|---|------|------|------|---------|
| C-1 | Cloud P1 Task 4 | bind-confirm get + delete 非原子 | 🔴 重复绑定 | 改 `getAndDelete` |
| C-2 | Cloud P2 Task 11 | redeem code 查 + 改非原子 | 🔴 双花卡密 | `WHERE used_by IS NULL` |
| C-3 | Cloud P1 Task 5 | handleAgentConnect 同 device 并发 | 🟡 瞬态双连接 | `putIfAbsent` + 踢旧 |
| C-4 | Cloud P2 Task 9 | Scheduler vs start_session 并发 | 🟡 到期仍建 session | 先断 WSS 再标状态 |
| C-5 | Agent P2 Task 14 | checkpoint 读-放锁-写破窗 | 🟡 写已删 session | 锁内 clone 完再放 |
| C-6 | Agent P1 Task 5 | WSS connect 后 transition 延迟 | 🟢 瞬态不一致 | 接受，下次事件修正 |
| C-7 | Agent P1 Task 6.5 | crash_count 直接写 | 🟢 文件损坏 | tmp+rename |
| C-8 | Agent P2 Task 9 | IPC 多 client 并发 handle | 🟢 安全 | tokio::spawn 隔离 |

---

## 八、编码前必须完成的准备工作

### 外部基础设施

| # | 事项 | 状态 |
|---|------|------|
| 1 | **GitHub 创建 `kn-cloud` 私有 repo** | ✅ 已创建 |
| 2 | **GitHub 创建 `kn-ios` 私有 repo** | ✅ 已创建 |
| 3 | **域名 `api.knshark.com`** + SSL 证书 | ⏳ 用户后续提供，开发/测试用 `localhost:8080/8081` |
| 4 | **Apple Developer Program** ($99/年) | ⏳ 用户正在注册 |
| 5 | **APNs p8 Key** | ⏳ 依赖 #4 |

### 开发环境（本地即可）

| # | 事项 | 说明 |
|---|------|------|
| 6 | 本地 Docker Compose (MySQL + Redis) | Cloud P1 本地开发调试 |
| 7 | 本地 Java 21 + Maven | Cloud P1/P2 编译运行 |
| 8 | Agent Cargo 依赖确认 | `portable-pty`, `tokio-tungstenite`, `security-framework` 等 |

### 计划修补 (P0 — 编码前修)

| # | 事项 | 对应问题 |
|---|------|---------|
| 13 | 补充 PTY spawn Task | B-1 |
| 14 | 修复 WsHandler sessionMapper 注入 | B-2 |
| 15 | 修复 bind-confirm 原子性 | B-3, C-1 |
| 16 | 补 pending:agent 写入逻辑 | H-2 |

---

## 九、各 Phase 就绪度

| Phase | 阻塞 | 就绪度 | 备注 |
|-------|------|--------|------|
| **Agent P1** | 无 | 🟢 95% | Workspace + 状态机 + WSS + session 全部完整 |
| **Agent P2** | B-1 (PTY 无归属) | 🟡 75% | 除 PTY spawn 外其余完整；Task 8 pty.rs 适配需小心 |
| **Agent P3** | B-1 (上游缺失) | 🟡 70% | 依赖 P2 PTY spawn 完成 |
| **Cloud P1** | B-2 (编译), B-3 (竞态) | 🟡 80% | 修复后可行 |
| **Cloud P2** | H-1 (双花) | 🟢 90% | 修复双花后可行 |
| **iOS P1** | 无 | 🟢 95% | 代码完整，逻辑自洽 |
| **iOS P2** | 无 | 🟢 95% | 逻辑完整 |

---

## 十、历史审查修复状态

前三次审查共发现 **91 项问题**，均已修复：

| 审查 | 文件 | 问题数 | 状态 |
|------|------|--------|------|
| 第一轮 | `2026-06-16-remote-control-design-issues.md` | 27 项 (4H+14M+9L) | ✅ 全部修复 |
| 第二轮 | `2026-06-17-remote-control-design-deep-review.md` | 35 项 (5B+12H+11M+7L) | ✅ 全部修复 |
| 第三轮 | `2026-06-17-remote-control-design-phase-review.md` | 29 项 (4B+8H+8M+9G) | ✅ 已修复, 仅 G-7 (远程日志) 为 v2 债务 |

---

## 十一、建议执行顺序

```
第 0 步 (当前): 解决 3B + 4H + 准备 12 项
    │
    ├── B-1: 决定 PTY spawn 归属 → 更新 plan
    ├── B-2: 修 WsHandler 构造注入
    ├── B-3: 修 bind-confirm 原子性
    ├── H-1~H-4: 修双花/离线缓存/crash原子写/checkpoint锁
    └── 准备: repo创建 + 服务器 + 域名 + Apple Developer

第 1 轮 (三端并行):
    Agent P1 ──── Cloud P1 ──── iOS P1
    (kn repo)    (kn-cloud)    (kn-ios)
    7 tasks      7 tasks       8 tasks

第 2 轮 (核心连通):
    Agent P2 ──→ Cloud P2 + iOS P2
    8 tasks      4 tasks    3 tasks

第 3 轮 (集成):
    Agent P3 (Desktop 📡 + 打包 + E2E)
```

---

## 十二、启动条件清单

在敲下第一行代码之前，确认以下全部就绪：

### 代码层面 (可立即修)
- [x] B-1: PTY spawn Task 已补充到 plan
- [x] B-2: WsHandler sessionMapper 依赖已修复
- [x] B-3: bind-confirm 改用 getAndDelete
- [x] H-1: RedeemService 加乐观锁
- [x] H-2: pending:agent 写入逻辑已补充
- [x] H-3: crash_count 改为 tmp+rename 原子写
- [x] H-4: checkpoint 锁内完成遍历（已正确实现，无需修改）
- [x] H-5: PTY 错误处理已随 B-1 覆盖
- [x] H-6: MembershipScheduler 先踢 WSS 再标 expired

### 基础设施 (需外部资源)
- [ ] kn-cloud 私有 repo 已创建
- [ ] kn-ios 私有 repo 已创建
- [ ] Linux 服务器已就绪 (MySQL + Redis + Nginx + Java 21)
- [ ] api.knshark.com 域名已解析 + SSL 已配置
- [ ] Apple Developer Program 已开通
- [ ] APNs p8 key 已创建
