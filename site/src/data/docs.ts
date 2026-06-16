import type { DocPage, DocGroup } from '../types/docs'

export const docGroups: DocGroup[] = [
  { id: 'getting-started', icon: '🚀', label: '入门指南', pages: ['introduction', 'installation', 'cli-vs-desktop'] },
  { id: 'cli-reference', icon: '📋', label: 'CLI 使用', pages: ['quickstart', 'config-management', 'shell-wrapper', 'command-reference', 'ai-tips', 'config-structure'] },
  { id: 'desktop', icon: '🖥️', label: 'Desktop 应用', pages: ['desktop-overview', 'desktop-install', 'desktop-ui', 'desktop-features', 'desktop-architecture', 'desktop-development'] },
  { id: 'scenarios', icon: '📖', label: '场景示例', pages: ['scenario-multi-account', 'scenario-project-keys', 'scenario-openai-proxy', 'scenario-qoder-cn'] },
  { id: 'more', icon: '💡', label: '更多', pages: ['faq', 'troubleshooting', 'uninstall'] },
]

export const docPages: Record<string, DocPage> = {
  // ─── 入门指南 ────────────────────────────────────────

  introduction: {
    id: 'introduction', group: 'getting-started', groupIcon: '🚀', title: '简介', next: 'installation',
    content: `KN 通过环境变量注入，让你在不同终端会话中为 \`claude\` / \`codex\` / \`qoderclicn\` 无缝切换 API 配置。任何兼容 Claude Code、Codex CLI 或 Qoder CN 的 API 提供商都可以——官方服务、第三方中转、自部署网关，一个 profile 搞定。

:::tip
**核心理念：** 每个终端会话独立注入环境变量，退出后自动清除，不影响其他窗口。
:::

## 典型场景

- 同时使用**多个 API 提供商**（如官方 Anthropic + DeepSeek 中转 + Qoder CN），不想手动改配置
- 不同项目用**不同的 API Key**，进目录自动切换（\`.ai-profile\` 文件）
- 用**第三方兼容 API**（任何支持 Anthropic/OpenAI 协议的服务）
- 多个团队成员共用一台机器，各自有独立配置

## 核心设计

\`\`\`
                    ┌─────────────────────────────┐
                    │   ~/.kn/        │
                    │   ├── config.yaml    ← 数据    │
                    │   ├── bin/profile    ← CLI    │
                    │   ├── lib/config.py  ← 库     │
                    │   └── shell-rc       ← 注入器  │
                    └──────┬──────────────────────┘
                           │ 读写同一份 config.yaml
              ┌────────────┼────────────┐
              ▼            ▼            ▼
        ┌──────────┐ ┌──────────┐ ┌──────────┐
        │ profile  │ │  Shell   │ │ Desktop  │
        │   CLI    │ │ Wrapper  │ │   GUI    │
        │ (Python) │ │  (Bash)  │ │ (Tauri)  │
        └──────────┘ └──────────┘ └──────────┘
\`\`\`

**三个组件，一份数据，文件锁保证并发安全。**

## 组件

| 组件 | 语言 | 作用 |
|------|------|------|
| \`profile\` CLI | Python 3 | 命令行增删改查 profile |
| Shell Wrapper | Bash | 拦截 \`claude\`/\`codex\`/\`qoderclicn\`，注入环境变量到子进程 |
| Desktop GUI | TypeScript + Rust (Tauri v2) | 可视化管理 + 内置 PTY 终端 |

## Desktop 亮点功能

- **Quick Switcher (⌘K)** — 全局快速启动器，模糊搜索 Profile 和项目
- **Hook 可视化管理** — 创建/编辑 Hook，支持 Stop、PreTool、PostTool 等事件
- **Agent 管理** — 浏览 Claude Code / Codex / Qoder 的 Agent 配置
- **Token 用量仪表盘** — 按模型/项目统计，成本估算，趋势图

## 技术栈

| 层 | 技术 |
|----|------|
| CLI | Python 3, hand-rolled YAML parser (zero-dependency), fcntl file locking |
| Shell Wrapper | Bash, \`sed\`-based YAML reading (zero-dependency) |
| Desktop Frontend | React 18 + TypeScript + Tailwind CSS + Vite |
| Desktop Backend | Rust (Tauri v2), \`portable-pty\`, \`serde_yaml\` |
| 终端模拟 | xterm.js + Canvas renderer + Fit addon |
| 站点 | Vue 3 + TypeScript + Vite + Tailwind CSS |
| 构建/发布 | GitHub Actions (macOS), Tauri bundler |`,
  },

  installation: {
    id: 'installation', group: 'getting-started', groupIcon: '🚀', title: '安装', prev: 'introduction', next: 'cli-vs-desktop',
    content: `项目提供两种安装方式：

- **安装包安装**：一键获取 Desktop GUI + CLI + Shell Wrapper，适合大多数用户
- **源码安装**：只安装 CLI + Shell Wrapper，适合纯终端用户或开发者

## 方式一：安装包安装（推荐）

安装包包含完整的 Desktop GUI 应用，同时也安装了 CLI 工具和 Shell Wrapper，安装后终端和桌面都能用。

### 1. 下载

从 [GitHub Releases](https://github.com/zhaojun2066/kn/releases/latest) 下载对应平台的安装包：

| 平台 | 格式 |
|------|------|
| macOS Apple Silicon (M1/M2/M3) | \`.dmg\` (aarch64) |
| macOS Intel | \`.dmg\` (x86_64) |

### 2. 安装

**macOS：** 打开 \`.dmg\`，将 \`KN.app\` 拖入 \`/Applications/\`。

> 由于应用未经过 Apple 开发者签名，首次打开会提示「已损坏，无法打开」。参考 [Desktop 安装与启动](/docs/desktop-install) 中的解决方法。

### 3. 安装后

首次启动 Desktop 应用会自动：
- 检测系统环境（\`claude\` / \`codex\` 是否已安装）
- 扫描已有配置，帮你导入为 profile
- 初始化 \`~/.kn/\` 目录和默认 \`config.yaml\`
- 确保 Shell Wrapper 已安装到 \`~/.zshrc\`

之后在终端执行 \`source ~/.zshrc\`，就可以用 \`ai claude <name>\` 命令了。

## 方式二：源码安装（仅 CLI + Shell Wrapper）

如果你只需要命令行工具，或者想从源码构建 Desktop：

### 1. 执行安装脚本

\`\`\`bash
git clone https://github.com/zhaojun2066/kn.git
cd kn
bash install.sh
\`\`\`

安装脚本会自动：
- 复制文件到 \`~/.kn/\`
- 将 \`~/.kn/bin\` 加入 PATH
- 在 \`~/.zshrc\` 中自动激活 Shell Wrapper
- 为 zsh 和 bash 配置 Shell 补全

**所有文件统一在一个目录下：**

\`\`\`
~/.kn/
├── bin/profile          ← 管理 CLI
├── lib/config.py        ← 共享模块（YAML 读写 + 文件锁）
├── shell-rc             ← Shell Wrapper
├── completions/          ← Shell 补全文件
├── config.yaml          ← profile 数据
└── .config.lock         ← 文件锁
\`\`\`

### 2. 激活

\`\`\`bash
source ~/.zshrc
\`\`\`

### 3. 导入现有配置（可选）

\`\`\`bash
profile init
\`\`\`

\`profile init\` 会自动从已有配置中导入环境变量：
- **\`~/.claude/settings.json\`** — Claude Code 配置
- **\`~/.codex/config.toml\` + \`~/.codex/auth.json\`** — Codex CLI 配置

### 4. 确认安装成功

\`\`\`bash
profile list
\`\`\`

应该能看到至少一个 profile。`,
  },

  'cli-vs-desktop': {
    id: 'cli-vs-desktop', group: 'getting-started', groupIcon: '🚀', title: 'CLI 与 Desktop 对比', prev: 'installation', next: 'quickstart',
    content: `CLI 和 Desktop 共享同一份 \`~/.kn/config.yaml\`，数据实时同步。你可以根据场景混用：

| | CLI + Shell Wrapper | Desktop GUI |
|---|---|---|
| **管理 profile** | \`profile add/set/remove\` 命令 | 可视化表单 + 4 步创建向导 |
| **启动 AI 工具** | 终端执行 \`ai claude <name>\` | 点击「运行」按钮，右侧终端自动启动 |
| **查看配置** | \`profile show <name>\` | 表格展示，key 打码，可直接编辑 |
| **批量操作** | 逐个执行命令 | ⌘ 多选，批量删除/导出 |
| **导入配置** | \`profile init\`（自动导入 Claude + Codex） | 图形化导入，预览后确认，支持 JSON 文件 |
| **终端体验** | 依赖系统终端 | 内置 xterm.js PTY 终端，支持 tab、搜索、主题 |
| **会话历史** | 依赖 shell history | 自动记录，一键恢复 |
| **配置备份** | 手动操作 | 自动备份 + 一键恢复 |
| **Hook 管理** | 手动编辑配置文件 | 可视化管理，启用/禁用，日志追溯 |
| **Agent 管理** | 手动浏览文件系统 | 图形界面浏览 + 搜索 |
| **Token 用量** | 不支持 | 自动追踪，按模型/项目统计，成本估算 |
| **Shell 补全** | zsh + bash 自动配置 | 不支持（Desktop 内终端独立） |
| **适用场景** | SSH 远程、纯终端环境、脚本自动化 | 日常桌面使用、频繁切换配置 |
| **Skills 管理** | 手动编辑文件 | 图形界面浏览 / 创建 / 编辑，三级来源 |
| **Plugins & Commands** | 命令行安装 | Marketplace 浏览安装，一键启用 / 禁用 |
| **Agent 管理** | 手动浏览文件系统 | 图形界面浏览 + 搜索 + 来源过滤 |

:::tip
**两种方式随时混用——Desktop 里改的配置，终端立刻生效，反之亦然。**
:::`,
  },

  // ─── CLI 使用 ────────────────────────────────────────

  quickstart: {
    id: 'quickstart', group: 'cli-reference', groupIcon: '📋', title: 'CLI 快速上手', prev: 'cli-vs-desktop', next: 'config-management',
    content: `## 基本使用

\`\`\`bash
# 查看所有 profile
profile list

# 启动 Claude Code
ai claude deepseek       # 指定 profile
ai claude                # 交互式选择

# 启动 Codex
ai codex codex-default   # 指定 profile
ai codex                 # 交互式选择

# 启动 Qoder CN
ai qoderclicn qoder-work # 指定 profile

# 原生命令不受影响
claude                   # 无 profile 注入，照常使用
\`\`\`

每次启动时，对应的 API key、base URL、模型等环境变量自动注入，**会话退出后自动清除**，不影响其他终端窗口。

## 工作流程

1. \`profile add <name> -i\` — 交互式创建 profile
2. \`ai claude <name>\` — 使用该 profile 启动
3. 退出后环境变量自动清除

## 项目级自动切换

在项目根目录创建 \`.ai-profile\` 文件，写入 profile 名称：

\`\`\`bash
echo "deepseek" > ~/projects/my-app/.ai-profile
\`\`\`

进入该目录后直接运行 \`ai claude\`，会自动使用对应 profile，无需每次指定。

**Profile 优先级：** 显式参数 > \`.ai-profile\` 项目绑定 > 默认 profile > 交互式选择

## 设置默认 Profile

\`\`\`bash
profile default deepseek   # 设置默认
ai claude                  # 直接使用默认，无需选择
\`\`\`

## 查看使用统计

\`\`\`bash
ai tips                    # 模型推荐 + 使用频率排名
\`\`\``,
  },

  'config-management': {
    id: 'config-management', group: 'cli-reference', groupIcon: '📋', title: '配置管理', prev: 'quickstart', next: 'shell-wrapper',
    content: `## 新增 Profile

### 方式一：交互式引导（推荐）

\`\`\`bash
profile add my-provider -i
\`\`\`

一步步询问，回车跳过不需要的字段。支持 Anthropic（Claude Code）和 OpenAI（Codex）两种 provider 类型。

### 方式二：逐条手动设置

\`\`\`bash
profile add my-provider "我的第三方中转"
profile set my-provider ANTHROPIC_AUTH_TOKEN=sk-xxxxxxxx
profile set my-provider ANTHROPIC_BASE_URL=https://api.example.com/anthropic
profile set my-provider ANTHROPIC_MODEL=claude-sonnet-4-6
\`\`\`

## 导入已有配置

\`\`\`bash
profile init
\`\`\`

自动扫描并导入：
- **\`~/.claude/settings.json\`** — Claude Code 环境变量
- **\`~/.codex/auth.json\` + \`~/.codex/config.toml\`** — Codex CLI 配置

## 修改 Profile

\`\`\`bash
# 修改已有的 key
profile set deepseek ANTHROPIC_AUTH_TOKEN=sk-new-key

# 删除某个配置项
profile unset deepseek ANTHROPIC_DEFAULT_OPUS_MODEL
\`\`\`

## 删除 Profile

\`\`\`bash
profile remove my-provider
\`\`\`

## 设置默认 Profile

\`\`\`bash
profile default deepseek   # 设为默认
profile default            # 查看当前默认值
\`\`\`

## 查看详情

\`\`\`bash
profile show deepseek
\`\`\`

:::info
敏感 key 自动打码显示（只显示前 4 位和后 4 位）。
:::`,
  },

  'shell-wrapper': {
    id: 'shell-wrapper', group: 'cli-reference', groupIcon: '📋', title: 'Shell Wrapper', prev: 'config-management', next: 'command-reference',
    content: `Shell Wrapper 是一组 Shell 函数，安装时自动添加到 \`~/.zshrc\`。

## 工作原理

1. 定义 \`ai\` 函数，拦截 \`claude\` / \`codex\` 子命令
2. 读取对应 profile 的 \`config.yaml\` 中的 \`env\` 字段
3. 以 \`export VAR=VAL\` 形式注入环境变量
4. 启动真实的 \`claude\` / \`codex\` 进程
5. 进程退出后，环境变量自动清除（Session 级隔离）

## 项目级自动切换

在项目根目录创建 \`.ai-profile\` 文件，写入 profile 名称。进入该目录后运行 \`ai claude\` 会自动使用对应 profile：

\`\`\`bash
echo "deepseek" > ~/projects/my-app/.ai-profile
cd ~/projects/my-app
ai claude    # 自动使用 deepseek profile
\`\`\`

**Profile 优先级：** 显式参数 > \`.ai-profile\` 项目绑定 > 默认 profile > 交互式选择

## Shell 补全

安装脚本自动为 zsh 和 bash 配置补全。安装后执行 \`source ~/.zshrc\` 即可使用：

- \`ai claude <Tab>\` → 列出所有 profile 名
- \`profile <Tab>\` → 列出所有子命令
- \`ai <Tab>\` → 列出所有子命令（claude / codex / qoderclicn / tips）

## 特性

- **多终端互不影响** — 每个终端窗口独立注入，互不干扰
- **原生命令不受影响** — 直接调用 \`claude\` / \`codex\` 不经过 wrapper
- **支持交互式选择** — 无参数调用时弹出 fzf 选择器
- **默认 profile** — 设了默认后无需每次选择
- **项目自动绑定** — \`.ai-profile\` 文件实现进目录即切换`,
  },

  'command-reference': {
    id: 'command-reference', group: 'cli-reference', groupIcon: '📋', title: '命令参考', prev: 'shell-wrapper', next: 'ai-tips',
    content: `## profile CLI 命令

| 命令 | 说明 |
|------|------|
| \`profile list\` | 列出所有 profile |
| \`profile show <name>\` | 查看 profile 详情（key 打码） |
| \`profile env <name>\` | 输出 env 变量（shell eval 格式） |
| \`profile names\` | 输出 profile 名列表（供 fzf 使用） |
| \`profile add <name> [desc]\` | 新增 profile，\`-i\` 为交互式 |
| \`profile remove <name>\` | 删除 profile |
| \`profile set <name> <K>=<V>\` | 设置环境变量 |
| \`profile unset <name> <K>\` | 删除环境变量 |
| \`profile default [name]\` | 查看/设置默认 profile |
| \`profile init\` | 从已有配置（Claude + Codex）导入 |

## Shell Wrapper 命令

| 用法 | 说明 |
|------|------|
| \`ai claude <profile>\` | 用指定 profile 启动 Claude Code |
| \`ai codex <profile>\` | 用指定 profile 启动 Codex |
| \`ai qoderclicn <profile>\` | 用指定 profile 启动 Qoder CN |
| \`ai claude\` | 交互式选择 profile 后启动 |
| \`ai profile list\` | 列出所有 profile（标注默认） |
| \`ai profile env <name>\` | 查看 profile 环境变量 |
| \`ai profile switch <name>\` | 切换默认 profile |
| \`ai tips\` | 模型推荐与使用频率排名 |
| \`ai\` / \`ai --help\` | 显示帮助信息 |`,
  },

  'ai-tips': {
    id: 'ai-tips', group: 'cli-reference', groupIcon: '📋', title: 'AI Tips', prev: 'command-reference', next: 'config-structure',
    content: `\`ai tips\` 是一个 Shell 内置命令，基于你的 shell 历史记录分析使用模式，给出模型推荐和排名。

## 功能

- **模型推荐** — 根据历史使用频率，推荐最适合你工作流的模型
- **使用排名** — 统计各 profile / 模型的使用次数
- **Shell 历史分析** — 自动读取 \`~/.zsh_history\` 或 \`~/.bash_history\`

## 使用方法

\`\`\`bash
ai tips
\`\`\`

输出示例：

\`\`\`
📊 AI CLI 使用统计 (最近 30 天)

Profile 使用排名:
  1. deepseek        ▏ 23 次  (deepseek-v4-pro)
  2. codex-work      ▏ 15 次  (gpt-5)
  3. anthropic       ▏  8 次  (claude-sonnet-4-6)

模型偏好:
  deepseek-v4-pro  → 常用在 deepseek profile
  gpt-5            → 常用在 codex-work profile

💡 建议: 将 deepseek 设为默认 profile
       profile default deepseek
\`\`\`

## 工作原理

\`ai tips\` 通过 Shell Wrapper 函数读取 \`~/.kn/config.yaml\`，结合 shell history 统计 \`ai claude <name>\` / \`ai codex <name>\` 等命令的使用频率，生成排名和推荐。

:::info
推荐数据完全基于本地 shell history，不会上传任何数据。
:::

## Shell 补全

安装脚本 \`install.sh\` 会自动为 zsh 和 bash 配置补全文件（位于 \`~/.kn/completions/\`），安装后执行 \`source ~/.zshrc\` 即可使用 Tab 补全。

支持的补全：
- \`ai claude <Tab>\` → 列出所有 profile 名
- \`ai codex <Tab>\` → 列出所有 profile 名
- \`profile <Tab>\` → 列出所有 profile 子命令`,
  },

  'config-structure': {
    id: 'config-structure', group: 'cli-reference', groupIcon: '📋', title: '配置结构', prev: 'ai-tips', next: 'desktop-overview',
    content: `配置文件位于 \`~/.kn/config.yaml\`。

## 格式示例

\`\`\`yaml
default: deepseek

profiles:
  deepseek:
    desc: "DeepSeek 中转（示例）"
    env:
      ANTHROPIC_AUTH_TOKEN: sk-xxxxxxxx
      ANTHROPIC_BASE_URL: https://api.deepseek.com/anthropic
      ANTHROPIC_MODEL: deepseek-v4-pro
      ANTHROPIC_DEFAULT_HAIKU_MODEL: deepseek-v4-flash
      ANTHROPIC_DEFAULT_SONNET_MODEL: deepseek-v4-pro
      ANTHROPIC_DEFAULT_OPUS_MODEL: deepseek-v4-pro
      DISABLE_AUTOUPDATER: "1"

  codex-default:
    desc: "官方或兼容 OpenAI 协议的 API"
    env:
      OPENAI_API_KEY: sk-proj-xxxxxxxx
      OPENAI_BASE_URL: https://api.openai.com/v1
      OPENAI_MODEL: gpt-5
\`\`\`

## env 字段说明

| 变量 | 用途 | 适用工具 |
|------|------|----------|
| \`ANTHROPIC_AUTH_TOKEN\` | API key（Claude Code 用） | Claude Code |
| \`ANTHROPIC_BASE_URL\` | 自定义 API 端点 | Claude Code |
| \`ANTHROPIC_MODEL\` | 默认模型 | Claude Code |
| \`ANTHROPIC_DEFAULT_HAIKU_MODEL\` | Haiku 模型 | Claude Code |
| \`ANTHROPIC_DEFAULT_SONNET_MODEL\` | Sonnet 模型 | Claude Code |
| \`ANTHROPIC_DEFAULT_OPUS_MODEL\` | Opus 模型 | Claude Code |
| \`OPENAI_API_KEY\` | API key（Codex 用） | Codex |
| \`OPENAI_BASE_URL\` | 自定义 API 端点 | Codex |
| \`OPENAI_MODEL\` | 默认模型 | Codex |
| \`DISABLE_AUTOUPDATER\` | 禁用自动更新 | Claude Code |

\`env\` 是自由 key-value，可以添加任何环境变量。`,
  },

  // ─── Desktop 应用 ─────────────────────────────────────

  'desktop-overview': {
    id: 'desktop-overview', group: 'desktop', groupIcon: '🖥️', title: '概览', prev: 'config-structure', next: 'desktop-install',
    content: `Desktop 应用是一个基于 **Tauri v2** 的原生桌面应用，提供可视化的 profile 管理界面和内置 PTY 终端。它与 CLI **共享同一份** \`~/.kn/config.yaml\`，两边数据实时同步。

- **前端：** React 18 + TypeScript + Tailwind CSS + Vite
- **后端：** Rust (Tauri v2) + \`portable-pty\` + \`serde_yaml\`
- **终端：** xterm.js + WebGL addon + Fit addon
- **支持平台：** macOS (ARM + Intel)

## 界面总览

\`\`\`
┌─────────────────────────────────────────────────────────────┐
│  Toolbar  [+ New] [Copy] [Import] [Export] [Scan] [⚙] ...  │
├──────────┬────────────────────────────────┬─────────────────┤
│          │                                │                 │
│ Sidebar  │        Main Panel              │  Right Terminal │
│          │                                │  (点击运行打开)   │
│ profile  │  ┌────────────────────────┐   │                 │
│  列表    │  │  Env Var Table         │   │  ┌───────────┐  │
│          │  │  (可编辑, 秘密打码)     │   │  │ tab1 tab2 │  │
│          │  └────────────────────────┘   │  ├───────────┤  │
│          │                                │  │           │  │
│          │  ┌────────────────────────┐   │  │  xterm    │  │
│          │  │  Command Reference     │   │  │  .js PTY  │  │
│          │  │  (命令速查)             │   │  │           │  │
│          │  └────────────────────────┘   │  │           │  │
│          │                                │  └───────────┘  │
│          │  ┌────────────────────────┐   │                 │
│          │  │  Session History       │   │                 │
│          │  │  (历史会话一键恢复)      │   │                 │
│          │  └────────────────────────┘   │                 │
├──────────┴────────────────────────────────┴─────────────────┤
│  Bottom Terminal (Ctrl+\` 切换, VS Code 风格)                 │
└─────────────────────────────────────────────────────────────┘
\`\`\`

## 组件

| 组件 | 技术 |
|------|------|
| 前端框架 | React 18 + TypeScript |
| 样式 | Tailwind CSS |
| 构建工具 | Vite |
| 桌面壳 | Tauri v2 |
| 后端语言 | Rust |
| 终端 | xterm.js + WebGL addon |
| PTY | portable-pty crate |
| 配置读写 | serde_yaml (Rust) / Python YAML (CLI) |`,
  },

  'desktop-install': {
    id: 'desktop-install', group: 'desktop', groupIcon: '🖥️', title: '安装与启动', prev: 'desktop-overview', next: 'desktop-ui',
    content: `## 下载预构建包

从 [GitHub Releases](https://github.com/zhaojun2066/kn/releases/latest) 下载对应平台的安装包：

| 平台 | 格式 |
|------|------|
| macOS Apple Silicon | \`.dmg\` (aarch64) |
| macOS Intel | \`.dmg\` (x86_64) |

## macOS 首次打开

由于应用未经过 Apple 开发者签名和公证，首次打开时会提示「已损坏，无法打开」或被阻止。

### 解决方法一：清除隔离属性（推荐）

\`\`\`bash
xattr -d com.apple.quarantine "/Applications/KN.app"
\`\`\`

如果提示权限不足，加 \`sudo\`：

\`\`\`bash
sudo xattr -d com.apple.quarantine "/Applications/KN.app"
\`\`\`

### 解决方法二：右键打开

在 Finder 中找到 App，**右键 → 打开**（不要双击），弹出的对话框里点「打开」。

## 开发模式启动

\`\`\`bash
cd desktop
npm install
npm run tauri dev
\`\`\`

> **注意：** \`tauri dev\` 修改 Rust 代码或 \`tauri.conf.json\` 后需要 Ctrl+C 重新启动。前端 TSX/CSS 修改支持热更新。

Desktop 应用与 CLI **共享同一份** \`~/.kn/config.yaml\`，数据实时同步。`,
  },

  'desktop-ui': {
    id: 'desktop-ui', group: 'desktop', groupIcon: '🖥️', title: '界面详解', prev: 'desktop-install', next: 'desktop-features',
    content: `Desktop 应用采用经典的三栏 + 终端布局。详细布局见 [概览](/docs/desktop-overview) 中的界面总览图。

## 工具栏 (Toolbar)

顶部工具栏集中了所有管理操作：

| <span style="display:none">按钮</span> | 功能 |
|------|------|
| ➕ **新建** | 打开 4 步创建向导，新建 profile |
| ⭐ **默认** | 设为默认 profile |
| 📋 **复制** | 复制当前 profile |
| 📥 **导入** | 下拉菜单：扫描系统配置 / 从 JSON 文件导入 |
| 📤 **导出** | 导出当前或选中的 profile 为 JSON |
| 🔍 **扫描** | 扫描系统已有配置，预览后导入 |
| 💾 **备份** | 配置备份（在设置菜单中） |
| ↩️ **恢复** | 从备份恢复配置（在设置菜单中） |
| 🌓 **主题** | 浅色 / 深色 / 自动 三档循环切换 |
| 💻 **终端** | 切换底部终端面板 |
| 🗂️ **侧边栏** | 切换侧边栏显示 |
| ⚙️ **设置** | 齿轮菜单：刷新、备份、恢复、检查更新、快捷键、关于 |

## 侧边栏 (Sidebar)

左侧 profile 列表，功能包括：

- **搜索** — 输入关键字实时过滤 profile
- **排序** — 按名称 / CLI 类型 / 环境变量数量排序
- **CLI 类型图标** — 每个 profile 旁标注 Claude 或 Codex 图标
- **默认标记** — 星标显示当前默认 profile
- **右键菜单** — 右键 profile 弹出操作菜单（编辑 / 复制 / 导出 / 删除）
- **多选** — ⌘ + 点击多选，批量删除或导出

## 主面板 (Main Panel)

选中 profile 后展示详情，包含三个区块：

### 环境变量表格 (Env Var Table)

- 两列布局：变量名 | 值
- 敏感 key（含 \`KEY\` / \`TOKEN\` / \`SECRET\` 等）默认打码显示，点击可切换显示/隐藏
- 双击直接编辑，回车保存
- 支持添加/删除行

### 命令速查 (Command Reference)

- 展示该 profile 的 CLI 启动命令
- 展示 env 查看命令
- 一键复制命令

### 会话历史 (Session History)

- 记录每次「运行」的历史（profile 名、时间、工作目录）
- 点击历史项一键恢复之前的终端会话

## 双终端面板

Desktop 应用内置两个独立的 PTY 终端面板，各有独立 tab 和会话：

| 特性 | Right Terminal（右侧终端） | Bottom Terminal（底部终端） |
|------|--------------------------|---------------------------|
| 打开方式 | 点击 profile 的「运行」按钮 | 工具栏终端按钮 / \`Ctrl+\`\` |
| 位置 | 主面板右侧 | 主面板下方（VS Code 面板风格） |
| 拖动调整 | 左右拖动分隔线 (min 480px) | 上下拖动分隔线 (min 120px) |
| 最大化 | \`⌘⇧M\` | \`⌘⇧M\` |
| 典型用途 | 启动 AI 工具进行交互 | 执行辅助命令、查看日志 |
| 终端搜索 | \`⌘F\` | \`⌘F\` |

每个终端面板支持：
- **多 Tab** — 创建多个终端会话，Tab 间独立运行
- **工作目录** — 显示当前工作目录，点击可切换
- **历史下拉** — 下拉菜单选择历史会话，一键恢复
- **6 套配色主题** — Dracula / Solarized / Monokai / One Dark / GitHub Light / Nord`,
  },

  'desktop-features': {
    id: 'desktop-features', group: 'desktop', groupIcon: '🖥️', title: '功能详解', prev: 'desktop-ui', next: 'desktop-architecture',
    content: `## 首次引导 (Onboarding Wizard)

首次启动 Desktop 应用时，自动弹出引导向导，分步完成：

1. **环境检测** — 检查系统中是否安装了 \`claude\`、\`codex\`、\`qoderclicn\`、\`brew\`，提示用户安装缺失的依赖
2. **扫描已有配置** — 自动扫描以下位置的现有配置：
   - \`~/.claude/settings.json\` → 提取 Anthropic 相关环境变量
   - \`~/.codex/auth.json\` → 提取 API Key
   - \`~/.codex/config.toml\` → 提取 Model 和 Base URL
   - \`~/.qoder-cn/\` → Qoder CN 配置
3. **预览确认** — 展示扫描结果，用户可选择导入哪些、自定义 profile 名称和 CLI 类型
4. **完成** — 导入选中的配置，生成初始 profile 列表

## Quick Switcher (⌘K)

全局快速启动器，类似 VS Code 的 Command Palette：

- **模糊搜索** — 输入关键字即时过滤 Profile 和项目
- **按频率排序** — 使用最多的 profile 排在前面
- **一键直达** — 选中后直接在新终端中启动
- **快捷键** — \`⌘K\`

## Hook 可视化管理

图形界面管理 Claude Code / Codex 的 Hook 配置：

- **支持的事件类型** — \`Stop\`、\`SessionEnd\`、\`PreTool\`、\`PostTool\`、\`Notification\`
- **按 CLI 类型分组** — 每个 CLI 工具的 Hook 独立管理
- **启用/禁用** — 一键开关 Hook，无需删除
- **日志追溯** — 通过 \`run-with-log.sh\` 执行，Hook 输出完整记录

## Skills 管理

浏览和管理 Claude Code / Codex 的 Skills 配置：

- **三级来源** — 用户级（\`~/.claude/skills/\`）、项目级、系统内置
- **内置 Skills 只读** — 系统内置 Skills 展示但不可修改
- **快速定位** — 按名称搜索，按来源和 CLI 类型过滤

## Plugins & Commands 管理

统一管理 Marketplace 插件和自定义 Commands：

- **Marketplace 插件** — 浏览、安装、启用/禁用、更新插件
- **自定义 Commands** — 管理快捷命令，按 CLI 类型分组
- **一键操作** — 启用/禁用、卸载，插件版本信息一目了然

## Agent 管理

浏览和管理 Claude Code / Codex / Qoder 的 Agent 配置：

- **三级来源** — 用户级（\`~/.claude/agents/\`）、项目级、内置 Agent
- **只读内置 Agent** — 系统内置 Agent 展示但不可修改，保证稳定性
- **快速定位** — 按名称搜索，按来源过滤

## Token 用量仪表盘

自动追踪 Token 消耗，可视化费用趋势：

- **按模型统计** — 每个模型的 token 输入/输出量
- **按项目维度** — 关联工作目录统计项目用量
- **成本估算** — 可配置模型价格，自动计算费用
- **趋势图** — 每日用量趋势一目了然
- **数据存储** — \`~/.kn/usage.jsonl\`

## 终端主题

6 套内置配色方案，每个终端面板独立设置：

| 主题 | 风格 |
|------|------|
| Dracula | 经典紫黑 |
| Solarized | 护眼暖色 |
| Monokai | 高对比度 |
| One Dark | Atom 风格 |
| GitHub Light | 浅色明亮 |
| Nord | 极简冷色 |

## 配置备份与恢复

- **自动备份** — 每次写入 \`config.yaml\` 时自动创建备份（3 代轮转）
- **手动备份** — 工具栏一键导出完整配置为 JSON
- **一键恢复** — 从备份文件恢复上次的配置

## 配置导入/导出

- **导出格式** — JSON，包含所有 profile 的完整数据（key 不加密，注意保管）
- **导入** — 支持 JSON 文件导入，自动识别 CLI 类型，预览后合并
- **批量操作** — 侧边栏多选后批量导出/删除

## Profile 管理

除了 [CLI 方式](/docs/config-management) 管理 profile 外，Desktop 提供可视化操作：

- **4 步创建向导** — 名称 → CLI 类型（Anthropic/OpenAI/Both/Qoder） → env vars → 完成
- **可视化编辑** — 表格直接编辑环境变量，支持添加自定义变量
- **Profile 复制** — 一键复制 profile 作为模板
- **右键操作** — 侧边栏右键菜单快速操作`,
  },

  'desktop-architecture': {
    id: 'desktop-architecture', group: 'desktop', groupIcon: '🖥️', title: '技术架构', prev: 'desktop-features', next: 'desktop-development',
    content: `## 架构总览

\`\`\`
┌────────────────────────────────────────────────────────┐
│                    React Frontend                       │
│  ┌──────────┐ ┌──────────┐ ┌──────────────────────┐   │
│  │ Sidebar  │ │MainPanel │ │   Terminal Panels     │   │
│  │ (profile │ │ (detail  │ │  ┌────────┐┌────────┐ │   │
│  │  list)   │ │  + env)  │ │  │ Right  ││ Bottom │ │   │
│  │          │ │          │ │  │(launch)││(toggle)│ │   │
│  └──────────┘ └──────────┘ │  └────────┘└────────┘ │   │
│                             └──────────────────────┘   │
│  invoke() ──────────── Tauri IPC ────────── Channel    │
└────────────────────────────────────────────────────────┘
                          │
┌────────────────────────────────────────────────────────┐
│                    Rust Backend                         │
│  ┌────────────┐ ┌────────────┐ ┌──────────────────┐   │
│  │ commands.rs│ │profile_cmd │ │     pty.rs        │   │
│  │ CRUD +     │ │.rs Python  │ │ portable-pty      │   │
│  │ file I/O   │ │ CLI wrapper│ │ spawn/write/resize│   │
│  └────────────┘ └────────────┘ └──────────────────┘   │
└────────────────────────────────────────────────────────┘
\`\`\`

## Rust 命令模块

| 模块 | 职责 |
|------|------|
| \`commands.rs\` | Profile CRUD、文件 I/O、系统扫描（Claude + Codex + Qoder）、环境检测、更新下载、导入导出 |
| \`profile_cmd.rs\` | 封装 Python \`profile\` CLI 的调用（子进程方式） |
| \`pty.rs\` | PTY 终端管理：spawn/write/resize/kill，通过 Channel 流式传输输出 |
| \`hook_manager.rs\` | Hook 配置管理：CRUD、启用/禁用、执行日志 |
| \`agent_manager.rs\` | Agent 扫描与查看：用户级/项目级/内置 |
| \`usage.rs\` | Token 用量追踪：JSONL 读写、按模型/项目统计 |
| \`lib.rs\` | Tauri Builder 初始化、插件注册、命令注册 |

## PTY 终端数据流

\`\`\`
用户按键 → xterm.onData → invoke("write_pty", {sessionId, data})
  → Rust write_pty → PTY stdin
  → Shell 输出 → PTY stdout
  → Rust reader 线程 → Channel.send(PtyEvent::Data)
  → 前端 Channel.onmessage → term.write(data)
\`\`\`

每个终端会话（tab）对应一个独立的 PTY 进程。Rust 端用 \`portable-pty\` crate 创建伪终端，启动 \`zsh -i -l\`（login + interactive）。前端通过 Tauri Channel 接收 stdout/stderr 流，通过 \`invoke("write_pty")\` 发送键盘输入。

## PTY 尺寸同步链

\`\`\`
容器 resize → ResizeObserver → RAF 合并 → fitAddon.fit()
  → xterm.js cols/rows 更新
  → onResize 回调 → invoke("resize_pty", {sessionId, cols, rows})
  → Rust: ioctl(TIOCSWINSZ) on PTY master fd
  → kernel 发送 SIGWINCH 给子进程
\`\`\`

## Profile 数据流

\`\`\`
Desktop GUI (invoke) ──→ Rust commands.rs ──→ serde_yaml 读写
                                    │
CLI (bin/profile) ──→ lib/config.py ──→ hand-rolled YAML 读写
                                    │
                           ~/.kn/
                              config.yaml
                                    │
Shell Wrapper ──→ sed 读取 ─────────┘
\`\`\`

三个组件通过同一份 YAML 文件通信，用 \`fcntl.flock\` 保证并发写入安全。

## 字体渲染

xterm.js 使用 WebGL addon 获得最佳 Unicode 支持（Claude Code TUI 的制表符渲染需要）。字体栈包含 CJK 等宽字体：

\`\`\`
ui-monospace, SF Mono, Cascadia Code, Menlo, Monaco,
JetBrains Mono, Fira Code, Consolas, Courier New,
PingFang SC, Noto Sans CJK SC, monospace
\`\`\``,
  },

  'desktop-development': {
    id: 'desktop-development', group: 'desktop', groupIcon: '🖥️', title: '开发与构建', prev: 'desktop-architecture',
    content: `## 环境要求

- **Node.js** >= 22
- **Rust** (stable, via rustup)
- **macOS:** Xcode Command Line Tools

## 常用开发命令

\`\`\`bash
# 开发模式（前端热更新 + Rust 监听）
npm run tauri dev

# 仅类型检查
npx tsc --noEmit

# 仅构建前端
npx vite build

# 仅检查 Rust
cd src-tauri && cargo check

# 全量检查 (TS + Vite + Cargo)
npx tsc --noEmit && npx vite build && cd src-tauri && cargo check
\`\`\`

## Shell Plugin Scope

当从 Rust 端调用系统命令或从前端使用 \`Command.create\` 时，命令二进制必须在 \`capabilities/default.json\` 中声明。在 \`shell:allow-execute\` 和 \`shell:allow-spawn\` 权限对象的 \`allow\` 数组中添加：

\`\`\`json
{ "name": "命令名", "cmd": "/完整/路径", "args": true }
\`\`\`

自定义 Rust \`#[tauri::command]\` 函数不受此限制，有完整系统访问权限。

## 本地构建 (macOS)

\`\`\`bash
# 安装 Intel 交叉编译目标
rustup target add x86_64-apple-darwin

# 构建当前架构
npm run tauri:build:prod

# 构建 Apple Silicon (ARM)
npm run tauri:build:prod:arm

# 构建 Intel Mac
npm run tauri:build:prod:intel

# Debug 构建
npm run tauri:build:debug
\`\`\`

## GitHub Actions 构建

项目使用 \`.github/workflows/build-desktop.yml\` 实现 CI 构建：

- **macOS ARM + Intel** — 矩阵构建两个架构

触发方式：推 tag (\`v*\`) 自动触发，或在 Actions 页面手动触发。

## 版本发布流程

1. 修改 \`src-tauri/tauri.conf.json\` → \`version\`
2. 用 GitHub Actions 构建 macOS 包
3. 下载产物，计算 SHA256
4. 将 SHA256 填入更新清单
5. 上传安装包到服务器/CDN
6. 更新服务器上的 \`update.json\`

## 代码签名

| 平台 | 需求 | 费用 |
|------|------|------|
| macOS | Apple Developer Program | $99/年 |

## 更新机制

Desktop 应用内置自动更新功能：

1. 应用启动时读取 \`update/update.json\` 中的 \`update_url\`
2. 向更新服务器请求更新清单
3. 比较版本号，若有新版本则弹出更新对话框
4. 通过 \`curl\` 下载安装包
5. \`shasum\` 校验 SHA256
6. \`open\` 打开安装包

## 快捷键

| 快捷键 | 功能 |
|--------|------|
| \`⌘N\` | 新建 Profile |
| \`⌘B\` | 切换侧边栏 |
| \`Ctrl+\`\` / \`⌘J\` | 切换底部终端 |
| \`⌘⇧M\` | 最大化终端 |
| \`⌘F\` | 搜索终端输出 |
| \`⌘K\` | 快捷键帮助 |
| \`Backspace\` | 删除选中的 Profile |

## 已知陷阱

### PTY Shell PATH 缺失

**现象：** dev 模式终端正常，生产包（\`.app\`）里 \`command not found\`。

**根因：** Tauri GUI 进程的 PATH 极简，不含 Homebrew 等用户安装的工具路径。

**修复：** PTY 启动 shell 时必须传 \`-i -l\`（login + interactive）两个标志。

### PTY 终端无法输入

**现象：** 生产包中内置终端键盘无法输入。

**根因：** macOS \`.app\` 进程没有 \`TERM\` 环境变量。

**修复：** 在复制父进程环境变量后，强制设置 \`TERM=xterm-256color\` 和 \`COLORTERM=truecolor\`。

### Tauri 环境变量不可靠

**核心教训：**
- GUI 应用进程的环境变量是"残缺的"
- **永远不要假设** \`std::env::vars()\` 包含完整的终端/用户环境
- **永远用 \`-i -l\` 启动 PTY shell**
- \`TERM\` 必须在 PTY 中显式设置为 \`xterm-256color\``,
  },

  // ─── 场景示例 ────────────────────────────────────────

  'scenario-multi-account': {
    id: 'scenario-multi-account', group: 'scenarios', groupIcon: '📖', title: '多账号切换', prev: 'desktop-development', next: 'scenario-project-keys',
    content: `## 场景：多个 API 提供商同时使用

\`\`\`bash
# 终端 A：用 provider-A
ai claude provider-a

# 终端 B：用 provider-b
ai claude provider-b
\`\`\`

两个终端互不影响，各自的 key 各自用。

## 准备工作

确保两个 profile 都已创建：

\`\`\`bash
profile add provider-a -i
profile add provider-b -i
\`\`\`

## 技巧

- 设为默认避免每次选择：\`profile default provider-a\`
- 不同项目目录用 \`.ai-profile\` 文件自动绑定 profile
- 也支持 direnv 等高级工具`,
  },

  'scenario-project-keys': {
    id: 'scenario-project-keys', group: 'scenarios', groupIcon: '📖', title: '不同项目不同 Key', prev: 'scenario-multi-account', next: 'scenario-openai-proxy',
    content: `## 场景：不同客户使用不同 API Key

\`\`\`bash
profile add client-a "客户 A"
profile set client-a ANTHROPIC_AUTH_TOKEN=sk-client-a-xxx
profile set client-a ANTHROPIC_BASE_URL=https://api.client-a.com/anthropic

profile add client-b "客户 B"
profile set client-b ANTHROPIC_AUTH_TOKEN=sk-client-b-xxx
\`\`\`

## 使用

\`\`\`bash
cd ~/projects/client-a
ai claude client-a    # 用客户 A 的 key

cd ~/projects/client-b
ai claude client-b    # 用客户 B 的 key
\`\`\`

## 自动切换（推荐）

在项目根目录创建 \`.ai-profile\` 文件，写入 profile 名称：

\`\`\`bash
echo "client-a" > ~/projects/client-a/.ai-profile
echo "client-b" > ~/projects/client-b/.ai-profile
\`\`\`

进入对应目录后直接运行 \`ai claude\`，无需每次指定 profile。

## 高级：用 direnv 自动切换

如果需要更精细的控制，也可以用 direnv：

\`\`\`bash
export ANTHROPIC_AUTH_TOKEN=$(profile env client-a 2>/dev/null | grep AUTH_TOKEN | cut -d= -f2 | tr -d "'")
\`\`\``,
  },

  'scenario-openai-proxy': {
    id: 'scenario-openai-proxy', group: 'scenarios', groupIcon: '📖', title: '兼容 API 中转', prev: 'scenario-project-keys', next: 'scenario-qoder-cn',
    content: `## 场景：用第三方兼容 API 跑 Codex

\`\`\`bash
profile add codex-third "第三方 OpenAI 兼容中转"
profile set codex-third OPENAI_API_KEY=sk-xxx
profile set codex-third OPENAI_BASE_URL=https://api.third-party.com/v1
profile set codex-third OPENAI_MODEL=gpt-5

ai codex codex-third
\`\`\`

## 支持的兼容 API

任何兼容 Anthropic 或 OpenAI API 格式的服务都可以：

- 各种 AI Gateway / 中转服务
- 自部署的 vLLM / Ollama 等兼容端点
- 其他云厂商的兼容接口

## 相关场景

- **Qoder CN（国产）接入** — 参见 [Qoder CN 接入](/docs/scenario-qoder-cn)，使用阿里通义协议接入，配置方式类似但使用 Anthropic 兼容接口。

## 注意事项

- 确保 \`BASE_URL\` 路径正确
- 模型名要和第三方服务支持的名称一致`,
  },

  // ─── 更多 ────────────────────────────────────────────

  'scenario-qoder-cn': {
    id: 'scenario-qoder-cn', group: 'scenarios', groupIcon: '📖', title: 'Qoder CN (国产) 接入', prev: 'scenario-openai-proxy', next: 'faq',
    content: `## 场景：使用国产 Qoder CN（阿里通义协议）

Qoder CN 是阿里云推出的 AI CLI 工具，兼容 Anthropic 协议。KN 完整支持 Qoder CN 的 profile 管理。

## 快速配置

\`\`\`bash
profile add qoder-work "Qoder CN 工作用" -i
\`\`\`

交互式创建时选择 Anthropic 类型的 provider，填入 Qoder CN 的环境变量：

\`\`\`bash
profile set qoder-work ANTHROPIC_AUTH_TOKEN=sk-xxxxxxxx
profile set qoder-work ANTHROPIC_BASE_URL=https://api.qoder.cn/anthropic
profile set qoder-work ANTHROPIC_MODEL=qoder-pro
\`\`\`

## 使用

\`\`\`bash
ai qoderclicn qoder-work
\`\`\`

## CLI 工具对比

| 工具 | 命令 | 协议 |
|------|------|------|
| Claude Code | \`ai claude <profile>\` | Anthropic |
| Codex CLI | \`ai codex <profile>\` | OpenAI |
| Qoder CN | \`ai qoderclicn <profile>\` | Anthropic (兼容) |

## 环境变量

Qoder CN 与 Claude Code 共用相同的 Anthropic 环境变量格式：

| 变量 | 说明 |
|------|------|
| \`ANTHROPIC_AUTH_TOKEN\` | API Key |
| \`ANTHROPIC_BASE_URL\` | Qoder CN API 端点 |
| \`ANTHROPIC_MODEL\` | 默认模型 |
| \`ANTHROPIC_DEFAULT_HAIKU_MODEL\` | Haiku 级模型 |
| \`ANTHROPIC_DEFAULT_SONNET_MODEL\` | Sonnet 级模型 |
| \`ANTHROPIC_DEFAULT_OPUS_MODEL\` | Opus 级模型 |

## 注意事项

- 确保 \`BASE_URL\` 指向正确的 Qoder CN API 端点
- 模型名称需要和 Qoder CN 控制台中创建的名称一致
- 首次使用前确保已安装 Qoder CN CLI（\`qoderclicn\` 命令可用）`,
  },

  faq: {
    id: 'faq', group: 'more', groupIcon: '💡', title: 'FAQ', prev: 'scenario-qoder-cn', next: 'troubleshooting',
    content: `## 多终端冲突

**Q: 多个终端同时改 profile 会冲突吗？**

不会。写操作通过文件锁（\`fcntl.flock\`）保护，同时写入会排队等待。

## API Key 安全

**Q: API key 安全吗？**

key 明文存储在 \`~/.kn/config.yaml\`。建议确保目录权限为 700：

\`\`\`bash
chmod 700 ~/.kn
\`\`\`

## CLI profile init 与 Desktop Scan

**Q: CLI 的 \`profile init\` 和 Desktop 的扫描有什么区别？**

功能相同，都会导入 Claude Code、Codex 和 Qoder CN 的已有配置。区别在于：
- **CLI \`profile init\`** — 自动导入，不预览，直接写入 config.yaml
- **Desktop「Scan」** — 扫描后展示预览，可勾选要导入的项、自定义 profile 名称

## Project 级别

**Q: 怎么让 profile 对整个项目目录生效？**

在项目根目录创建 \`.ai-profile\` 文件，写入 profile 名称即可。进入该目录后 \`ai claude\` 会自动使用对应 profile。

\`\`\`bash
echo "deepseek" > ~/projects/my-app/.ai-profile
\`\`\`

也支持 direnv 等高级方式。

## Qoder CN

**Q: 支持 Qoder CN（阿里通义）吗？**

完整支持。Qoder CN 使用 Anthropic 兼容协议，通过 \`ai qoderclicn <profile>\` 启动。配置方式与 Claude Code profile 相同，使用 \`ANTHROPIC_*\` 环境变量。

## Shell 补全

**Q: Shell 补全怎么配置？**

安装脚本 \`install.sh\` 会自动为 zsh 和 bash 配置补全，安装后执行 \`source ~/.zshrc\` 即可使用 Tab 补全 profile 名和子命令。

## Token 用量

**Q: 如何查看 Token 用量和费用？**

Desktop 应用内置「Token 用量仪表盘」，自动追踪每次调用的 token 消耗，按模型和项目维度统计。可在设置中配置模型价格，自动估算费用。数据存储在 \`~/.kn/usage.jsonl\`。

## 修改描述

**Q: 如何修改 profile 的描述？**

编辑 \`~/.kn/config.yaml\`，直接改 \`desc:\` 字段即可。

## 重新导入

**Q: profile init 导入后还能重新导入吗？**

不会覆盖已有 profile。手动删除后再导入：

\`\`\`bash
profile remove imported
profile init
\`\`\``,
  },

  troubleshooting: {
    id: 'troubleshooting', group: 'more', groupIcon: '💡', title: '故障排查', prev: 'faq', next: 'uninstall',
    content: `## 安装问题

### \`profile: command not found\`

确保 PATH 包含 \`~/.kn/bin\`：

\`\`\`bash
echo $PATH | grep kn
\`\`\`

如果没有，手动添加：

\`\`\`bash
export PATH="$HOME/.kn/bin:$PATH"
\`\`\`

### Shell Wrapper 未激活

检查 \`~/.zshrc\` 中是否有 wrapper source 行：

\`\`\`bash
grep "kn" ~/.zshrc
\`\`\`

## 权限问题

### 文件锁冲突

如果遇到 \`PermissionError\` 相关的文件锁问题：

\`\`\`bash
rm ~/.kn/.config.lock
\`\`\`

### 目录权限不足

\`\`\`bash
chmod 700 ~/.kn
\`\`\`

### Shell 补全不生效

确保已 source 配置文件：

\`\`\`bash
source ~/.zshrc
\`\`\`

检查补全文件是否存在：

\`\`\`bash
ls ~/.kn/completions/
\`\`\`

如果缺失，重新运行安装脚本：\`bash install.sh\`

### .ai-profile 自动切换不生效

检查 \`.ai-profile\` 文件内容是否只包含一个有效的 profile 名（无空格、无换行）：

\`\`\`bash
cat ~/projects/my-app/.ai-profile
\`\`\`

确保该 profile 已在 \`config.yaml\` 中存在：\`profile list\`

## Desktop App 问题

### "已损坏，无法打开"

参考 [安装与启动](/docs/desktop-install) 中的 macOS 签名解决方案。

### 闪退

尝试命令行启动查看错误日志：

\`\`\`bash
/Applications/KN.app/Contents/MacOS/kn
\`\`\``,
  },

  uninstall: {
    id: 'uninstall', group: 'more', groupIcon: '💡', title: '卸载与重置', prev: 'troubleshooting',
    content: `## 完全卸载

\`\`\`bash
rm -rf ~/.kn
\`\`\`

## 清理 Shell 配置

编辑 \`~/.zshrc\`，删除以下标记之间的内容：

\`\`\`
# >>> KN >>>
...
# <<< KN <<<
\`\`\`

## 重置为初始状态

\`\`\`bash
# 备份现有配置
cp ~/.kn/config.yaml ~/.kn/config.yaml.bak

# 重新初始化
rm ~/.kn/config.yaml
profile init
\`\`\``,
  },
}
