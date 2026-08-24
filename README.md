# KN

KN 是 macOS 上的 AI 开发工作台：管理 AI CLI 运行配置和本机终端，并让用户可从 iPhone 继续已连接 Mac 上的 AI CLI 会话。

项目由三个仓库协作：本仓（Desktop、CLI、Agent）、`kn-cloud`（私有 Cloud 服务）和 `kn-ios`（iOS 客户端）。功能和接口以代码与测试为准；开发文档入口见 [docs/README.md](docs/README.md)。

---

## 安装

从官网首页“下载”区域下载安装包。链接由 KN 自有发布服务按架构提供，并附带 SHA-256：

| 平台 | 格式 |
|------|------|
| macOS Apple Silicon | `.dmg` (aarch64) |
| macOS Intel | `.dmg` (x86_64) |

首次启动自动完成环境检测和 Shell Wrapper 安装。

或源码安装（仅 CLI + Shell Wrapper）：

```bash
git clone https://github.com/zhaojun2066/kn.git
cd kn && bash install.sh && source ~/.zshrc
```

---

## 快速上手

```bash
profile init                # 导入已有配置
profile add my-api -i       # 交互式创建运行配置
ai claude my-api            # 启动 Claude Code + 指定运行配置
ai codex                    # 自动检测：项目级 → 默认 → 交互选择
```

---

## Desktop 应用

Desktop 是 kn 的电脑端入口——一个基于 Tauri v2 的桌面 GUI，提供可视化运行配置管理、内置 PTY 终端、扩展管理、项目工作台和手机远程控制。

### 运行配置管理

- **可视化管理** — 表格展示环境变量，敏感 key 自动打码，双击编辑
- **4 步创建向导** — 名称 → CLI 类型 → 环境变量 → 完成
- **系统扫描导入** — 自动发现 `~/.claude/settings.json`、`~/.codex/auth.json` 等已有配置
- **批量操作** — 多选删除/导出，JSON 格式导入导出
- **项目绑定** — 读取项目 `.ai-profile`，自动关联运行配置

### 扩展管理

统一管理 Skills、Agents、Hooks 等扩展能力：

- **Hooks** — Claude Code / Codex 的事件触发器，支持向导创建、编辑、启用/禁用、执行日志
- **Agents & Skills** — 扫描用户级和项目级配置，区分来源，内置 Agent 只读保护

### 双终端面板

两个独立 PTY 终端（login + interactive shell），支持多 Tab、6 套主题、终端搜索：

| 终端 | 打开方式 | 位置 |
|------|---------|------|
| Right Terminal | 运行配置“运行”按钮 | 主面板右侧 |
| Bottom Terminal | 工具栏 / `Ctrl+`` | 主面板下方 |

### Quick Switcher (`⌘K`)

全局快速启动器——模糊搜索运行配置、项目目录，按使用频率排序，回车即启。

### 项目工作台与用量

- **项目工作台** — 管理已登记项目，并通过本地 Agent 提供 Git、PR 和验证任务能力
- **Token 用量追踪** — 记录 `ai` 调用的用量，支持按模型和项目查看，可配置价格计算费用

---

## CLI 命令

```bash
profile list                    # 列出所有运行配置
profile show <name>             # 查看详情（key 打码）
profile add <name> -i           # 交互式创建
profile set <name> KEY=VALUE    # 设置环境变量
profile remove <name>           # 删除
profile default [name]          # 查看/切换默认
```

Shell Wrapper `ai` 命令：

```bash
ai claude <profile>             # 指定运行配置启动 Claude Code
ai codex <profile>              # 指定运行配置启动 Codex
ai claude                       # 自动检测运行配置
ai profile list                 # 列出运行配置
ai profile switch <name>        # 切换默认
ai tips                         # 模型推荐 + 使用排行
```

> 直接运行 `claude` / `codex` 不受影响，不经过 wrapper。

---

## 项目级自动切换

在项目根目录创建 `.ai-profile` 文件，写入运行配置名，该目录下 `ai claude` 自动使用对应运行配置：

```bash
echo "work" > ~/project/.ai-profile
cd ~/project && ai claude   # 自动使用 work 运行配置
```

优先级：显式指定 > `.ai-profile` > 默认运行配置 > 交互选择

---

## FAQ

**API key 安全吗？** key 明文存储在 `~/.kn/config.yaml`，建议 `chmod 700 ~/.kn`。

**多终端同时改配置会冲突吗？** 不会，文件锁保护并发写入，3 代轮转备份防数据丢失。

**如何查看 token 用量？** Desktop 应用中的用量面板，支持按模型和项目维度查看。

**支持哪些 AI 工具？** Claude Code、Codex CLI、Qoder CN（国产），任何兼容协议的 API 服务。

---

[MIT License](LICENSE)
