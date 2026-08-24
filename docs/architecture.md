# KN 架构

KN 由三个独立仓库组成：本仓负责 macOS 桌面应用、CLI 和本地 Agent；`../kn-cloud` 负责私有 HTTP/WSS 服务；`../kn-ios` 是 iOS 客户端。三个仓库的历史文档不作为事实来源。

```text
macOS (kn)                                      Server (kn-cloud)                 iOS (kn-ios)
┌─────────────────────────────────┐            ┌──────────────────────┐          ┌───────────────┐
│ Desktop (Tauri) ── Unix socket ──┼───────────►│ WSS /v1/ws           │◄────────►│ SwiftUI client │
│ CLI + shell wrapper              │            │ protocol adapter     │          │               │
│ kn-agent ── PTY / local files    │            │ REST /api/v1/*       │          │               │
└───────────────┬─────────────────┘            └───────┬──────────────┘          └───────────────┘
                │                                      │
                ▼                                      ▼
 ~/.kn/ 默认根目录（可由 KN_HOME 覆盖）            MySQL + Redis
```

## 职责与数据所有权

| 位置 | 负责内容 | 主要状态 |
| --- | --- | --- |
| `bin/`、`lib/`、`shell/` | 运行配置 CLI 与 `ai()` wrapper | `~/.kn/config.yaml` |
| `desktop/` | 配置、项目、资源管理和本机终端 UI | `~/.kn/projects.json`、Tauri 进程内状态 |
| `agent/` | launchd 守护、PTY、会话恢复、Cloud 连接 | 生产 `~/.kn/agent/`；Debug Desktop 为 `~/.kn-dev/agent/` |
| `common/` | Rust 端共享路径、配置、加密和类型 | 无独立持久化 |
| `kn-cloud` | 身份、设备、会话路由、持久化与协议转换 | MySQL、Redis |
| `kn-ios` | 移动端 UI 与公开协议客户端 | iOS 应用本地状态 |

## 本仓关键约束

- `~/.kn/config.yaml` 是默认的运行配置源；Rust 通过 `config_dir()` 支持 `KN_HOME` 覆盖。写入需遵守跨进程锁、原子替换和三代备份规则。
- Desktop 的两个 PTY 面板彼此独立；浏览器端写入在 `useTerminal` 内按动画帧批处理。
- Agent 以 launchd 常驻，Desktop 经 Unix Domain Socket 调用它；远程终端和项目操作由 Agent 实际执行。
- Cloud 是移动端公开协议与 Agent 内部协议之间的唯一适配层。iOS 不应依赖 Agent 的 snake_case 消息。

详细职责见 [Desktop](desktop.md)、[Agent](agent.md)、[Cloud](cloud.md) 和[协议](protocol.md)。
