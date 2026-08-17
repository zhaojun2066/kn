# architecture-chain-review.md 二次核验报告

审查日期: 2026-06-27

审查对象: `docs/architecture-chain-review.md`

核验范围: `kn`、`kn-cloud`、`kn-ios` 三个仓库中与原文风险、链路和改进建议直接相关的实现。未修改业务代码。

## 总体结论

原文对系统大体链路的还原基本可用，尤其是绑定、WSS 远程控制、Redis Pub/Sub 跨节点中继、ack 去重、cli_heartbeat、配置文件锁等主线描述，多数能在源码中找到对应实现。

但这份文档不能直接作为改造优先级依据。主要问题有三类:

1. 部分结论过时或不完整，例如 `SessionHeartbeatMonitor` 并不是只扫本地 `sessionCache`，`replay_output` 路径也没有被纳入所有权校验清单。
2. 部分风险是真实存在，但优先级被拔高，例如无分布式追踪不应列为 P0。
3. 有真实问题被漏报，例如 Shell fallback 的 `ai profile switch` 直接 `sed -i` 写配置，绕过锁、备份和原子写。

## 不属实或过时的结论

### 1. J-R3 关于 SessionHeartbeatMonitor 的描述不属实

原文段落: `docs/architecture-chain-review.md:1156`

原文称:

> SessionHeartbeatMonitor 只扫描本地 sessionCache，不扫描全量 Redis

核验结论: 不属实。当前 `SessionHeartbeatMonitor` 直接 Redis `SCAN user:sessions:*`，并对每个活跃 session 检查 `cli:heartbeat:{nid}`。

关联源码:

- `/Users/zhaojun/workspace/me/shark/kn-cloud/kn-cloud-ws/src/main/java/dev/kn/cloud/ws/component/SessionHeartbeatMonitor.java:66`
- `/Users/zhaojun/workspace/me/shark/kn-cloud/kn-cloud-ws/src/main/java/dev/kn/cloud/ws/component/SessionHeartbeatMonitor.java:68`
- `/Users/zhaojun/workspace/me/shark/kn-cloud/kn-cloud-ws/src/main/java/dev/kn/cloud/ws/component/SessionHeartbeatMonitor.java:80`
- `/Users/zhaojun/workspace/me/shark/kn-cloud/kn-cloud-ws/src/main/java/dev/kn/cloud/ws/component/SessionHeartbeatMonitor.java:84`

需要保留的真实风险: `SessionCoordinator.handleCliHeartbeat()` 里的 `scanAndEndMissingSessions()` 的确只扫本地 `sessionCache`，但这只是 Agent 上报列表与本地缓存的快速比对路径，不等同于 `SessionHeartbeatMonitor` 全局死会话扫描。

关联源码:

- `/Users/zhaojun/workspace/me/shark/kn-cloud/kn-cloud-ws/src/main/java/dev/kn/cloud/ws/handler/SessionCoordinator.java:463`
- `/Users/zhaojun/workspace/me/shark/kn-cloud/kn-cloud-ws/src/main/java/dev/kn/cloud/ws/handler/SessionCoordinator.java:480`
- `/Users/zhaojun/workspace/me/shark/kn-cloud/kn-cloud-ws/src/main/java/dev/kn/cloud/ws/handler/SessionCoordinator.java:482`

建议修正文案: 将 J-R3 改成“`cli_heartbeat` 的主动缺失比对只覆盖本节点缓存；全局兜底由 `SessionHeartbeatMonitor` Redis SCAN 完成”。

### 2. 所有权校验清单漏掉 replay_output，B-R6 的“已实现”结论不完整

原文段落:

- `docs/architecture-chain-review.md:440`
- `docs/architecture-chain-review.md:1196`

原文称 input/output/ctrl 等路径独立校验所有权，并标记 B-R6 为已实现。

核验结论: 不完整。`input`、`ctrl`、`resize`、`output` 有校验，但 `replay_output` 只根据 `sessionNid` 查 `SessionMeta` 并转发给 Agent，没有校验请求 iOS 用户 `userId == meta.userId()`。

已实现校验的源码:

- `/Users/zhaojun/workspace/me/shark/kn-cloud/kn-cloud-ws/src/main/java/dev/kn/cloud/ws/service/MessageRelayService.java:58`
- `/Users/zhaojun/workspace/me/shark/kn-cloud/kn-cloud-ws/src/main/java/dev/kn/cloud/ws/service/MessageRelayService.java:87`
- `/Users/zhaojun/workspace/me/shark/kn-cloud/kn-cloud-ws/src/main/java/dev/kn/cloud/ws/service/MessageRelayService.java:112`
- `/Users/zhaojun/workspace/me/shark/kn-cloud/kn-cloud-ws/src/main/java/dev/kn/cloud/ws/service/MessageRelayService.java:164`

缺失校验的源码:

- `/Users/zhaojun/workspace/me/shark/kn-cloud/kn-cloud-ws/src/main/java/dev/kn/cloud/ws/handler/KnWsHandler.java:626`
- `/Users/zhaojun/workspace/me/shark/kn-cloud/kn-cloud-ws/src/main/java/dev/kn/cloud/ws/service/MessageRelayService.java:133`
- `/Users/zhaojun/workspace/me/shark/kn-cloud/kn-cloud-ws/src/main/java/dev/kn/cloud/ws/service/MessageRelayService.java:140`
- `/Users/zhaojun/workspace/me/shark/kn-cloud/kn-cloud-ws/src/main/java/dev/kn/cloud/ws/service/MessageRelayService.java:147`

建议: 原文应把 `replay_output` 增加到 IDOR 校验表，并将 B-R6 从“✅ 已实现”改为“⚠️ 部分实现”。这是安全问题，优先级高于原文 P0-3 的 traceId。

### 3. “iOS ack 超时重试 1 次”的表述不准确

原文段落:

- `docs/architecture-chain-review.md:438`
- `docs/architecture-chain-review.md:1248`

核验结论: B-R4 正文“timeout 不重试”属实；但 3.4 表格写“iOS ack 超时重试 1 次”不准确。iOS 只在服务端返回 `dropped=true` 触发 `TransportError.rateLimited` 时重试一次；30s ack timeout 直接抛 `DomainError.requestTimeout`。

关联源码:

- `/Users/zhaojun/workspace/me/shark/kn-ios/Data/Network/WebSocketTransport.swift:127`
- `/Users/zhaojun/workspace/me/shark/kn-ios/Data/Network/WebSocketTransport.swift:133`
- `/Users/zhaojun/workspace/me/shark/kn-ios/Data/Network/WebSocketTransport.swift:168`
- `/Users/zhaojun/workspace/me/shark/kn-ios/Data/Network/WebSocketTransport.swift:175`
- `/Users/zhaojun/workspace/me/shark/kn-ios/Data/Network/WebSocketTransport.swift:176`

建议修正文案: 表格项改为“限流 dropped 重试 1 次；ack timeout 不重试”。

### 4. P2-4 “Agent 提供 connect_pty IPC 方法”是过时建议

原文段落: `docs/architecture-chain-review.md:1405`

核验结论: 原文建议用 `connect_pty` 取代 `socat` 依赖，但当前 Agent/Shell 已有等价的 IPC attach 机制: Shell 先 `new_session`，再 `attach` 获取 `pty.sock`，然后用 `socat` 桥接本地终端。

关联源码:

- `/Users/zhaojun/workspace/me/shark/kn/shell/ai-profile.sh:353`
- `/Users/zhaojun/workspace/me/shark/kn/shell/ai-profile.sh:361`
- `/Users/zhaojun/workspace/me/shark/kn/shell/ai-profile.sh:373`
- `/Users/zhaojun/workspace/me/shark/kn/agent/src/ipc.rs:21`
- `/Users/zhaojun/workspace/me/shark/kn/agent/src/ipc.rs:22`

仍然属实的部分: `socat` 依赖本身仍存在，原文 E-R1 是事实；但改进方向不应写成“新增 connect_pty IPC”，而应是“减少或内置 socat 替代桥接”。

## 属实但需要调整优先级或表述

### 1. B-R1 / D-R5 输出背压风险属实，但“无缓解”过重

原文段落:

- `docs/architecture-chain-review.md:435`
- `docs/architecture-chain-review.md:654`
- `docs/architecture-chain-review.md:1384`

核验结论: 风险核心属实。Agent WSS outgoing channel 是 `mpsc::unbounded_channel`，OutputFanout 的 WSS 和 IPC sender 也都是 unbounded。大量输出时，网络发送端慢于 PTY 读取端，内存会增长。

关联源码:

- `/Users/zhaojun/workspace/me/shark/kn/agent/src/ws_client.rs:75`
- `/Users/zhaojun/workspace/me/shark/kn/agent/src/ws_client.rs:148`
- `/Users/zhaojun/workspace/me/shark/kn/agent/src/session/output.rs:21`
- `/Users/zhaojun/workspace/me/shark/kn/agent/src/session/output.rs:153`
- `/Users/zhaojun/workspace/me/shark/kn/agent/src/session/output.rs:223`

但“缓解措施: 无”不准确。当前已有 100ms 定时 flush、64KB 立即 flush、10KB 分块、远程开关、256KB 本地环形日志。这些不是背压，但能降低单条消息和本地日志膨胀。

关联源码:

- `/Users/zhaojun/workspace/me/shark/kn/agent/src/session/output.rs:10`
- `/Users/zhaojun/workspace/me/shark/kn/agent/src/session/output.rs:27`
- `/Users/zhaojun/workspace/me/shark/kn/agent/src/session/output.rs:167`
- `/Users/zhaojun/workspace/me/shark/kn/agent/src/session/output.rs:174`
- `/Users/zhaojun/workspace/me/shark/kn/agent/src/session/output.rs:199`

建议: 保留 P0/P1 级别，但把建议从“简单 bounded(64KB) 超出丢弃”改为更精细的策略: 对 WSS 出站队列设置有界队列和会话级降采样/丢弃策略，同时保留本地 ring log 供 `replay_output`。直接阻塞 PTY read 可能反向卡住子进程，需谨慎。

### 2. B-R3 / G-R1 Redis Pub/Sub 丢消息属实，但已有订阅重连，不是完全裸奔

原文段落:

- `docs/architecture-chain-review.md:437`
- `docs/architecture-chain-review.md:1385`

核验结论: 属实。Redis Pub/Sub 是 fire-and-forget，订阅者重启或断线期间会丢跨节点消息。当前没有 ack/重传。

关联源码:

- `/Users/zhaojun/workspace/me/shark/kn-cloud/kn-cloud-ws/src/main/java/dev/kn/cloud/ws/service/MessageRelayService.java:288`
- `/Users/zhaojun/workspace/me/shark/kn-cloud/kn-cloud-ws/src/main/java/dev/kn/cloud/ws/component/RedisSubscriber.java:118`
- `/Users/zhaojun/workspace/me/shark/kn-cloud/kn-cloud-ws/src/main/java/dev/kn/cloud/ws/component/RedisSubscriber.java:149`

但实现有订阅线程断线指数退避重连，原文可补充这一点。

关联源码:

- `/Users/zhaojun/workspace/me/shark/kn-cloud/kn-cloud-ws/src/main/java/dev/kn/cloud/ws/component/RedisSubscriber.java:71`
- `/Users/zhaojun/workspace/me/shark/kn-cloud/kn-cloud-ws/src/main/java/dev/kn/cloud/ws/component/RedisSubscriber.java:78`
- `/Users/zhaojun/workspace/me/shark/kn-cloud/kn-cloud-ws/src/main/java/dev/kn/cloud/ws/component/RedisSubscriber.java:86`

建议: Redis Streams 或应用层 ack 可以作为高可用阶段改造，但是否 P0 取决于是否已经多 WS 节点部署。如果当前是单节点部署，此项应降为 P1/P2。

### 3. P1-1 缩短心跳间隔属于过度优化

原文段落:

- `docs/architecture-chain-review.md:1154`
- `docs/architecture-chain-review.md:1392`

核验结论: 45s 最坏检测窗口属实，但把 Agent cli heartbeat 从 15s 降到 5s、Cloud scan 从 30s 降到 10s，会直接放大 Redis 写入和扫描频率。

关联源码:

- `/Users/zhaojun/workspace/me/shark/kn/agent/src/main.rs:455`
- `/Users/zhaojun/workspace/me/shark/kn/agent/src/main.rs:510`
- `/Users/zhaojun/workspace/me/shark/kn-cloud/kn-cloud-ws/src/main/java/dev/kn/cloud/ws/component/SessionHeartbeatMonitor.java:62`
- `/Users/zhaojun/workspace/me/shark/kn-cloud/kn-cloud-ws/src/main/java/dev/kn/cloud/ws/component/SessionHeartbeatMonitor.java:68`

建议: 这不是 P1 必改。除非已有明确用户体验数据证明 45s 不可接受，否则优先做 UI 层 reconnecting/interrupted 状态提示，或只把 scan 改为配置项。

### 4. P1-4 “InputMerger 增加 seq 排序”不建议直接做

原文段落:

- `docs/architecture-chain-review.md:439`
- `docs/architecture-chain-review.md:1395`

核验结论: 多来源输入交叉风险存在，但“按 seq 排序”不是天然正确方案。iOS seq 只对同一 iOS session 有意义；Desktop 本地 attach、Shell IPC、iOS 输入可能来自不同通道，统一排序会引入等待、重排和交互延迟。

建议: 除非产品明确支持多控制端同时操作同一 PTY，否则更务实的处理是同一时刻只允许一个远程控制 owner，或在 UI 上提示“本地/远程正在输入”。不建议作为 P1 直接实现 seq buffer。

## 原文漏报的真实问题

### 1. Shell fallback 的 `ai profile switch` 绕过配置写锁和备份

原文关联段落:

- `docs/architecture-chain-review.md:998`
- `docs/architecture-chain-review.md:1003`

原文聚焦 Rust 直接写绕过 Python 锁，但当前更明确的绕过点在 Shell fallback: `_profile_switch()` 直接执行 macOS `sed -i ''` 修改 `config.yaml`，没有 `.config.lock`，没有 3 代备份，没有 tmp+fsync+rename。

关联源码:

- `/Users/zhaojun/workspace/me/shark/kn/shell/ai-profile.sh:77`
- `/Users/zhaojun/workspace/me/shark/kn/shell/ai-profile.sh:84`
- `/Users/zhaojun/workspace/me/shark/kn/shell/ai-profile.sh:85`

对比安全写路径:

- `/Users/zhaojun/workspace/me/shark/kn/common/src/profile.rs:124`
- `/Users/zhaojun/workspace/me/shark/kn/common/src/profile.rs:142`
- `/Users/zhaojun/workspace/me/shark/kn/common/src/profile.rs:166`
- `/Users/zhaojun/workspace/me/shark/kn/common/src/profile.rs:185`

建议: 这应加入 H-R 系列，优先级高于 H-R4 diff 噪音。Shell 有 `PROFILE_CMD` 时应调用 CLI 切换；无 CLI 时 fallback 可以只提示安装/修复 CLI，或者实现带锁的最小写入。

### 2. `replay_output` IDOR 风险应加入安全风险表

原文关联段落:

- `docs/architecture-chain-review.md:1196`
- `docs/architecture-chain-review.md:1342`

如上所述，`replay_output` 缺少 userId 校验。虽然它请求的是 Agent 本地 ring log 回放，而不是直接操作 PTY stdin，但输出内容可能包含敏感命令结果或 token。建议列为安全风险 X 系列。

### 3. 文档没有区分“跨节点必需改造”和“单节点可接受”

原文关联段落:

- `docs/architecture-chain-review.md:1385`
- `docs/architecture-chain-review.md:1394`

Redis Pub/Sub 丢消息、Redis Streams、跨节点 relay ack 等建议只有在多 WS 节点部署时才是高优先级。若生产当前是单 WS 节点，`tryDirectRelay` 会优先本地投递，跨节点路径不是主路径。

关联源码:

- `/Users/zhaojun/workspace/me/shark/kn-cloud/kn-cloud-ws/src/main/java/dev/kn/cloud/ws/service/MessageRelayService.java:69`
- `/Users/zhaojun/workspace/me/shark/kn-cloud/kn-cloud-ws/src/main/java/dev/kn/cloud/ws/service/MessageRelayService.java:262`
- `/Users/zhaojun/workspace/me/shark/kn-cloud/kn-cloud-ws/src/main/java/dev/kn/cloud/ws/service/MessageRelayService.java:275`

建议: 原文应明确“如果部署多节点，则 P0/P1；如果单节点，则降级”。

## 过度优化或不建议照做

### 1. P0-3 traceId 不应是 P0

原文段落: `docs/architecture-chain-review.md:1386`

核验结论: 分布式追踪有价值，但它不是阻断性 bug，也不是安全问题。当前更高优先级应是 `replay_output` 所有权校验和 Shell fallback 配置写安全。

建议: 降为 P2。可以先用已有 sessionNid/userId/machineId 增强日志，不必立即改协议加 `traceId`。

### 2. P1-3 “Redis 不可用时跨节点 direct 路径”表述不可行

原文段落: `docs/architecture-chain-review.md:1394`

核验结论: “去重切本地 LRU”可行；但“跨节点 relay 在 Redis 恢复前走 direct 路径”对不同 WS 节点之间不可行，因为 direct 路径依赖本节点 `ConnectionRegistry` 能拿到目标 WebSocketSession。跨节点时目标连接不在当前 JVM 内存里。

关联源码:

- `/Users/zhaojun/workspace/me/shark/kn-cloud/kn-cloud-ws/src/main/java/dev/kn/cloud/ws/service/MessageRelayService.java:262`
- `/Users/zhaojun/workspace/me/shark/kn-cloud/kn-cloud-ws/src/main/java/dev/kn/cloud/ws/service/MessageRelayService.java:278`
- `/Users/zhaojun/workspace/me/shark/kn-cloud/kn-cloud-ws/src/main/java/dev/kn/cloud/ws/component/RedisSubscriber.java:155`

建议: 如果要做 Redis 降级，只能覆盖单节点或本地缓存命中路径；多节点需要独立 RPC/Streams/消息队列，不能靠 direct fallback。

### 3. P1-5 ANSI 边界保护不是当前最优先

原文段落:

- `docs/architecture-chain-review.md:441`
- `docs/architecture-chain-review.md:1396`

核验结论: 10KB chunk 确实可能切 ANSI escape；当前 chunk 到 String 使用 `from_utf8_lossy`，而不是按字符边界分块。风险存在。

关联源码:

- `/Users/zhaojun/workspace/me/shark/kn/agent/src/session/output.rs:199`
- `/Users/zhaojun/workspace/me/shark/kn/agent/src/session/output.rs:218`
- `/Users/zhaojun/workspace/me/shark/kn/agent/src/session/output.rs:219`

但比起输出背压、`replay_output` 鉴权、Shell 配置写安全，这属于显示瑕疵风险。建议降为 P2，除非已有明确终端渲染 bug。

## 建议后的优先级

### P0

1. 补 `replay_output` 所有权校验。
2. Agent WSS outgoing / OutputFanout 出站队列背压或限流，至少给远程输出路径加有界队列和丢弃策略。

### P1

1. Shell `ai profile switch` fallback 改为不直接 `sed -i` 写配置，或实现锁+备份+原子写。
2. 多节点部署前，替换 Redis Pub/Sub 或补应用层 ack/重传；单节点部署可降级。
3. 修正文档中 J-R3、ack timeout、P2-4 等过时描述，避免误导后续实现。

### P2

1. traceId / structured logging / metrics。
2. ANSI escape 边界保护。
3. JWT key rotation。
4. raw 模式 `stty sane` 恢复。

## 可直接保留的原文结论

以下结论核验后基本属实:

1. 绑定码 300s、bind_poll generation guard、设备数量限制、machineId 唯一约束方向正确。
2. Redis SETNX 去重 fail-open 属实。
3. Redis Pub/Sub 不保证送达属实。
4. iOS ack timeout 不自动重试属实。
5. Agent AUTH_REJECTED 不重连并进入未绑定状态属实。
6. Redis `pending:agent:{machineId}` 离线缓冲最多 1000 条、7 天 TTL 属实。
7. Refresh token 旋转竞态、Keychain `AfterFirstUnlockThisDeviceOnly`、JWT key rotation 缺口属实。

