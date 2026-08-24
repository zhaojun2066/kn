# 跨端协议边界

Cloud 是 KN 的协议边界：移动端只使用 Cloud 公共协议；Agent 使用内部协议；Cloud 的 mapper 和 dispatcher 负责两者转换。不要让 iOS 直接兼容 Agent 的字段、消息名或错误语义。

```text
iOS public protocol (camelCase)
        │
        ▼
Cloud: MobileMessageDispatcher / MobileProtocolMapper
        │
        ▼
Agent internal protocol (current wire names, often snake_case)
```

## 连接信封

Agent 的 WSS 消息使用以下信封；具体 `data` 以 `agent/src/proto.rs`、Cloud `AgentMessageDispatcher` 及测试为准：

```json
{
  "type": "…",
  "ts": 0,
  "sessionId": "s_…",
  "data": {}
}
```

当前 Agent 处理的方向包括：

- Cloud → Agent：连接确认、心跳、启动/恢复/结束会话、输入和控制信号、回放、项目 Git/PR/验证请求。
- Agent → Cloud：心跳、会话生命周期与输出、运行配置/项目列表、项目 Git/PR/验证结果与交付确认。

## 变更流程

1. 先判定消息属于 `mobile public` 还是 `agent internal`；不要在边界不明时新增 wire type。
2. 修改 Agent 内部消息时，同步修改 `agent/src/proto.rs`、Agent 处理逻辑、Cloud `AgentProtocolMapper` / `AgentMessageDispatcher` 与协议测试。
3. 修改移动端公开消息时，同步修改 Cloud `MobileMessageDispatcher` / `MobileProtocolMapper`、`../kn-ios` 的编码解码和相关测试。
4. 调整认证、会话 ID、重放、幂等或 ACK 语义时，检查 Redis 状态、断线恢复与所有三仓测试。

消息清单不是静态设计稿。实现中的枚举、白名单和协议测试始终优先于本页。
