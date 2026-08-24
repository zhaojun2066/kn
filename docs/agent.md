# Agent

`agent/` 中的 `kn-agent` 是 macOS 本地守护进程：它管理远程 AI CLI 会话、PTY 输出、会话恢复、项目工作台操作，并通过 WSS 与 Cloud 通信。Desktop 不直接替移动端启动远程会话，而是经本地 IPC 请求 Agent。

## 运行与配置

- 二进制：`kn-agent`；`kn-agent bind` 可发起设备绑定。
- 常驻方式：Desktop 管理对应的 launchd 服务。
- 配置优先级：`KN_CLOUD_URL` / `KN_CLOUD_HTTP_URL` / `KN_PURCHASE_URL` 环境变量，其次 `<config_root>/agent/config.json`，最后为内置生产默认地址。生产根目录为 `~/.kn`；Desktop Debug 启动的 Agent 使用 `~/.kn-dev` 并通过 `KN_HOME` 传入。
- Cloud 连接：`wss://…/v1/ws`，Agent 使用 device token、机器 ID 与协议版本请求头认证。

## 本地状态

| 路径 | 用途 |
| --- | --- |
| `<config_root>/agent/ipc.sock` | Desktop 与 Agent 的 Unix Domain Socket |
| `<config_root>/agent/logs/` | Agent 日志 |
| `<config_root>/agent/sessions/` | 会话元数据、输出日志和恢复信息 |
| `<config_root>/agent/terminal-parser-profiles.json` | 终端解析规则 |
| `<config_root>/agent/config.json` | 可选运行时地址配置 |
| `<config_root>/projects.json` | Desktop 注册的项目，Agent 仅允许对其中项目执行远程工作台操作 |

## 主要模块

| 模块 | 职责 |
| --- | --- |
| `ipc.rs` | JSON-RPC 风格本地请求：状态、绑定、会话、兑换、解除绑定等 |
| `ws_client.rs`、`proto.rs` | WSS 连接、重连和 Agent 内部消息编解码 |
| `session/` | PTY 生命周期、输入/输出、持久化、回放、Git/PR/验证操作 |
| `bind.rs`、`device.rs`、`state.rs` | 设备身份、绑定状态和本地安全存储 |
| `project_delivery.rs`、`delivery_outbox_store.rs` | 可确认的项目交付队列与断线恢复 |

## 开发与校验

```bash
cargo check -p kn-agent
cargo test -p kn-agent --lib
```

修改消息类型时，必须同时检查 [Cloud](cloud.md) 的 dispatcher / mapper 和 [协议](protocol.md) 的边界规则；若改变移动端公开消息，还必须检查 `../kn-ios`。
