# kn

AI CLI 工具的多 profile 管理器。在不同终端会话中为 `claude` / `codex` / `qoder` 无缝切换 API key、Base URL 和模型配置。

> 🌐 官网：[https://zhaojun2066.github.io/kn/](https://zhaojun2066.github.io/kn/)

---

## 安装

从 [GitHub Releases](https://github.com/zhaojun2066/kn/releases/latest) 下载安装包：

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
profile add my-api -i       # 交互式创建 profile
ai claude my-api            # 启动 Claude Code + 指定 profile
ai codex                    # 自动检测：项目级 → 默认 → 交互选择
```

---

## Desktop 应用

Desktop 是 kn 的核心——一个基于 Tauri v2 的桌面 GUI，提供可视化的 profile 管理、内置 PTY 终端、扩展管理和用量追踪。

### 环境管理

- **可视化管理** — 表格展示环境变量，敏感 key 自动打码，双击编辑
- **4 步创建向导** — 名称 → CLI 类型 → 环境变量 → 完成
- **系统扫描导入** — 自动发现 `~/.claude/settings.json`、`~/.codex/auth.json` 等已有配置
- **批量操作** — 多选删除/导出，JSON 格式导入导出
- **项目绑定** — 读取项目 `.ai-profile`，自动关联 profile

### 扩展管理

统一管理 Skills、Agents、Hooks 等扩展能力：

- **Hooks** — Claude Code / Codex 的事件触发器，支持向导创建、编辑、启用/禁用、执行日志
- **Agents & Skills** — 扫描用户级和项目级配置，区分来源，内置 Agent 只读保护

### 双终端面板

两个独立 PTY 终端（login + interactive shell），支持多 Tab、6 套主题、终端搜索：

| 终端 | 打开方式 | 位置 |
|------|---------|------|
| Right Terminal | Profile "运行"按钮 | 主面板右侧 |
| Bottom Terminal | 工具栏 / `Ctrl+`` | 主面板下方 |

### Quick Switcher (`⌘K`)

全局快速启动器——模糊搜索 profile、项目目录，按使用频率排序，回车即启。

### Token 用量追踪

自动记录每次 `ai` 调用的 token 消耗，支持按模型/按项目维度查看，可配置价格计算费用。

---

## CLI 命令

```bash
profile list                    # 列出所有 profile
profile show <name>             # 查看详情（key 打码）
profile add <name> -i           # 交互式创建
profile set <name> KEY=VALUE    # 设置环境变量
profile remove <name>           # 删除
profile default [name]          # 查看/切换默认
```

Shell Wrapper `ai` 命令：

```bash
ai claude <profile>             # 指定 profile 启动 Claude Code
ai codex <profile>              # 指定 profile 启动 Codex
ai claude                       # 自动检测 profile
ai profile list                 # 列出 profile
ai profile switch <name>        # 切换默认
ai tips                         # 模型推荐 + 使用排行
```

> 直接运行 `claude` / `codex` 不受影响，不经过 wrapper。

---

## 项目级自动切换

在项目根目录创建 `.ai-profile` 文件，写入 profile 名，该目录下 `ai claude` 自动使用对应 profile：

```bash
echo "work" > ~/project/.ai-profile
cd ~/project && ai claude   # 自动使用 work profile
```

优先级：显式指定 > `.ai-profile` > 默认 profile > 交互选择

---

## FAQ

**API key 安全吗？** key 明文存储在 `~/.kn/config.yaml`，建议 `chmod 700 ~/.kn`。

**多终端同时改配置会冲突吗？** 不会，文件锁保护并发写入，3 代轮转备份防数据丢失。

**如何查看 token 用量？** Desktop 应用中的用量面板，支持按模型/按项目维度。

**支持哪些 AI 工具？** Claude Code、Codex CLI、Qoder CN（国产），任何兼容协议的 API 服务。

---

[MIT License](LICENSE)
