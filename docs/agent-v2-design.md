# KN Agent v2：跨平台远程控制设计

> 状态：提案。本文由产品方向变更触发，不描述当前已实现的行为；实现完成后，应将事实迁回 `docs/architecture.md`、`docs/agent.md` 和 `docs/protocol.md`。

## 决策

KN v2 的唯一核心职责是：让用户从手机安全地控制**自己电脑上已安装、已登录的 AI CLI**。

- 不管理、导入、切换或同步 AI API Key、CLI 配置和环境变量。
- 不再以 `profile` 或 `.ai-profile` 作为产品和协议概念。
- 本地 Agent 拥有项目白名单、Runner 发现、PTY 和会话恢复；Cloud 负责账户、设备归属、授权和消息路由；iOS 负责控制和查看安全的运行状态。
- Desktop 不是必需组件。未来如保留，只能是 macOS 的可选界面，调用 Agent 的本地接口，不能拥有项目或 Agent 状态。

## 用户体验

```bash
# 一次安装与绑定
npx @kn/agent setup

# 在某个项目中将当前目录明确授予 Agent 使用
cd ~/work/kn
kn project add . --runner codex

# 日常检查和管理
kn doctor
kn project list
kn project remove proj_xxx
```

`setup` 检测 Node 版本、可用 Runner、CLI 自身登录状态和后台 Agent；它不读取或写入 AI Key。安装后的常用形态是全局命令 `kn`，`npx` 只用于试用和首次安装，不能作为常驻后台进程的唯一入口。

## 扫码绑定

### 为什么 CLI 足够

CLI 能创建一次性配对申请、渲染二维码、等待确认并安全保存设备令牌；二维码的确认发生在已登录的 iOS App。这避免在电脑端再做账户密码登录、浏览器 OAuth 或 Desktop UI。

绑定是“把一台电脑归属到一个已登录 KN 用户”的双向确认，而不是“用二维码传递长期凭据”。二维码只含短期、一次性、不可用于 Agent WSS 认证的配对地址。

### 流程

```text
电脑 CLI / Agent                       Cloud                         已登录 iOS
      │                                  │                                │
      │ POST /device/pairings            │                                │
      │  { machinePublicId, deviceName } │                                │
      │─────────────────────────────────►│                                │
      │  { pairingId, qrUrl, expiresAt } │                                │
      │◄─────────────────────────────────│                                │
      │                                  │                                │
      │ 在终端显示 QR + 短码             │        扫 QR / 输入短码          │
      │                                  │◄───────────────────────────────│
      │                                  │  iOS JWT + pairingId            │
      │                                  │                                │
      │                                  │ 显示设备名、OS、到期时间         │
      │                                  │───────────────────────────────►│
      │                                  │          用户明确“确认绑定”      │
      │                                  │◄───────────────────────────────│
      │ GET /device/pairings/{id}/claim  │                                │
      │  { oneTimePollSecret }           │                                │
      │─────────────────────────────────►│                                │
      │ { deviceToken, deviceId }        │                                │
      │◄─────────────────────────────────│                                │
      │ 本地安全保存 token，启动 WSS      │                                │
      │─────────────────────────────────►│                                │
```

CLI 输出应同时包含二维码和手动短码，保证 SSH、无图形终端和摄像头不可用时仍可完成绑定：

```text
$ kn setup
✓ Codex CLI: 已安装，已登录
✓ Claude Code: 已安装，已登录
✓ 后台 Agent: 已安装

用 KN App 扫描二维码，或在“添加设备”中输入短码： F7K2Q9
设备：Jun 的 MacBook Pro · macOS · 有效期 10 分钟
[二维码]

等待确认…  (Ctrl+C 可取消，不会影响已安装的 Agent)
✓ 已绑定为“Jun 的 MacBook Pro”
✓ Agent 已上线
```

### 安全与失败语义

- QR URL 仅携带随机 `pairingId` 和短期展示令牌；不能含 `deviceToken`、用户 JWT、真实项目路径或 Runner 认证信息。
- iOS 必须展示设备名、系统和到期时间，并要求登录用户明确确认。扫描二维码本身不完成绑定。
- Agent 使用独立的、高熵 `deviceToken` 建立 WSS；令牌仅存本机，macOS 用 Keychain、Windows 用 Credential Manager。无法使用系统凭据库时，退回仅当前用户可读的文件并在 `doctor` 中告警。
- 配对 10 分钟失效、仅能领取一次；取消、过期、网络中断和重复领取均应幂等。
- Agent 在保存 token 后再报告在线。若“Cloud 已签发 token、CLI 未收到响应”，用 `pairingId + poll secret` 重试领取；不能重新发起配对或生成第二个设备。
- 解绑立即吊销 device token，并关闭该设备的 WSS 与所有控制租约。

现有实现已经具备这一模型的主要基础：`bind-init → poll → activate`、待领取标记和幂等激活。v2 可以保留语义、换成跨平台 CLI 的实现；不必重新设计账户或设备安全模型。

## Agent 状态与本地数据

Agent 使用系统标准数据目录：macOS `~/.kn/`；Windows `%APPDATA%\\kn\\`。项目白名单和会话状态属于 Agent，不属于 Desktop。

```text
kn/
  agent.json             # Agent 配置；不含 AI Key
  projects.json          # projectId、显示名、绝对路径、defaultRunnerId
  sessions/              # 本地恢复数据与受控终端日志
  device-token           # 优先由系统凭据库存储
```

`kn project add .` 必须在本机执行。v2 第一阶段不允许手机提交任意目录、任意 shell 命令或任意环境变量。以后如支持手机发起“添加项目”申请，也必须由电脑端本地确认。

## Runner 与项目

Runner 是 Agent 可以安全启动的固定 CLI 适配，不是用户配置档案。

```ts
type RunnerInfo = {
  id: "codex" | "claude";
  label: string;
  available: boolean;
  authState: "ready" | "notLoggedIn" | "missing" | "unsupported";
  version?: string;
};

type Project = {
  id: string;
  displayName: string;
  path: string;             // 只在 Agent 本地保存
  defaultRunnerId?: string;
};
```

Agent 只上报 Runner 的公开状态和项目的 `id` / `displayName` / `defaultRunnerId`。Cloud 与 iOS 不需要获得本机绝对路径、Git remote、环境变量或 CLI 凭据。

## v2 协议合同

Cloud 继续是公开移动协议与 Agent 内部协议之间的唯一适配层。以下是目标公共语义，而非 Agent wire 格式：

| 方向 | 公共消息 | 必要数据 | 禁止数据 |
| --- | --- | --- | --- |
| Agent → iOS | `deviceStatus` | online、OS、agentVersion | token、机器硬件标识 |
| Agent → iOS | `runnerCatalog` | id、label、available、authState | 环境变量、认证详情 |
| Agent → iOS | `projectList` | id、displayName、defaultRunnerId | 本机 path、remote URL |
| iOS → Agent | `startRun` | requestId、projectId、runnerId、终端尺寸 | cwd、command、env |
| 双向 | `runState` / `output` / `input` / `interrupt` | sessionId、状态、流数据 | Cloud 持久化的终端正文 |
| Agent → iOS | `runSummary` | 时间、阶段、验证/Git/PR 安全摘要 | 原始命令、凭据、完整日志 |

`startRun` 由 Cloud 验证用户、设备归属、订阅权限和控制租约后才映射给 Agent。Agent 再验证 `projectId` 和 `runnerId` 属于本机白名单，绝不把 iOS 传来的值拼成 shell 命令。

```json
{
  "type": "startRun",
  "data": {
    "requestId": "req_123",
    "projectId": "proj_456",
    "runnerId": "codex",
    "cols": 100,
    "rows": 30
  }
}
```

## 实施顺序

1. 在 Cloud、iOS 和新 Agent 确认上述 v2 协议合同与安全字段；删除 profile、cwd 和 command 从公开启动消息的入口。
2. 建立 TypeScript monorepo：CLI、Agent core、Runner adapter、macOS/Windows 平台 adapter；先实现 `setup`、配对、后台服务、`doctor`。
3. 实现本地项目白名单、Runner 发现和 `startRun`，让 iOS 以项目 + Runner 启动与恢复终端会话。
4. 迁移会话状态、验证与项目交付能力；Cloud 只存安全摘要，实时终端流不入持久化历史。
5. 停止构建 Tauri Desktop、Python profile CLI 和 shell wrapper。若以后重做 Desktop，只能调用 v2 Agent 的本地接口。

## 非目标

- 托管或同步任何 AI Key、SSH Key、CLI 认证文件或环境变量。
- 通过 Cloud 执行任意本机 shell 命令。
- 在第一阶段提供远程文件浏览或任意目录选择。
- 让 Desktop 成为 Agent 的安装、项目注册或会话恢复前提。
