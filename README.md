# AI Profile Manager

多 profile 管理系统，让你在不同终端会话中为 `claude` / `codex` 使用不同的 API key、base URL 和模型配置。

> 🌐 官网：[https://zhaojun2066.github.io/ai-profile-manager/](https://zhaojun2066.github.io/ai-profile-manager/)

---

## 目录

- [项目概览](#项目概览)
- [安装](#安装)
  - [方式一：安装包安装（推荐，含 Desktop + CLI）](#方式一安装包安装推荐含-desktop--cli)
  - [方式二：源码安装（仅 CLI + Shell Wrapper）](#方式二源码安装仅-cli--shell-wrapper)
- [CLI 与 Desktop 对比](#cli-与-desktop-对比)
- [CLI 使用](#cli-使用)
  - [配置管理](#配置管理)
  - [Shell Wrapper](#shell-wrapper)
  - [命令参考](#命令参考)
- [配置结构](#配置结构)
- [常见场景](#常见场景)
- [Desktop 应用](#desktop-应用)
  - [界面详解](#界面详解)
  - [功能详解](#功能详解)
  - [技术架构](#技术架构)
  - [内部架构详解](#内部架构详解)
  - [开发指南](#开发指南)
  - [构建与发布](#构建与发布)
  - [快捷键参考](#快捷键参考)
  - [更新机制](#更新机制)
  - [已知陷阱](#已知陷阱)
- [项目结构](#项目结构)
- [FAQ](#faq)

---

## 项目概览

### 这是什么？

AI Profile Manager 通过环境变量注入，让你在不同终端会话中为 `claude` / `codex` 无缝切换 API 配置。任何兼容 Claude Code 或 Codex CLI 的 API 提供商都可以——官方服务、第三方中转、自部署网关，一个 profile 搞定。

典型场景：

- 同时使用**多个 API 提供商**（如官方 Anthropic + DeepSeek 中转），不想手动改配置
- 不同项目用**不同的 API Key**
- 用**第三方兼容 API**（任何支持 Anthropic/OpenAI 协议的服务）
- 多个团队成员共用一台机器，各自有独立配置

### 核心设计

```
                    ┌─────────────────────────────┐
                    │   ~/.claude-profiles/        │
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
```

**三个组件，一份数据，文件锁保证并发安全。**

| 组件 | 语言 | 作用 |
|------|------|------|
| `profile` CLI | Python 3 | 命令行增删改查 profile |
| Shell Wrapper | Bash | 拦截 `claude`/`codex`，注入环境变量到子进程 |
| Desktop GUI | TypeScript + Rust (Tauri v2) | 可视化管理 + 内置 PTY 终端 |

### 技术栈总览

| 层 | 技术 |
|----|------|
| CLI | Python 3, hand-rolled YAML parser (zero-dependency), fcntl file locking |
| Shell Wrapper | Bash, `sed`-based YAML reading (zero-dependency) |
| Desktop Frontend | React 18 + TypeScript + Tailwind CSS + Vite |
| Desktop Backend | Rust (Tauri v2), `portable-pty`, `serde_yaml` |
| 终端模拟 | xterm.js + WebGL addon + Fit addon |
| 站点 | Vue 3 + TypeScript + Vite + Tailwind CSS |
| 构建/发布 | GitHub Actions (全平台), Tauri bundler |

---

## 安装

项目提供两种安装方式：

- **安装包安装**：一键获取 Desktop GUI + CLI + Shell Wrapper，适合大多数用户
- **源码安装**：只安装 CLI + Shell Wrapper，适合纯终端用户或开发者

### 方式一：安装包安装（推荐，含 Desktop + CLI）

安装包包含完整的 Desktop GUI 应用，同时也安装了 CLI 工具和 Shell Wrapper，安装后终端和桌面都能用。

#### 1. 下载

从 [GitHub Releases](https://github.com/zhaojun2066/ai-profile-manager/releases/latest) 下载对应平台的安装包：

| 平台 | 格式 |
|------|------|
| macOS Apple Silicon (M1/M2/M3) | `.dmg` (aarch64) |
| macOS Intel | `.dmg` (x86_64) |
| Windows | `.msi` |
| Linux | `.AppImage` |

#### 2. 安装

**macOS：** 打开 `.dmg`，将 `AI Profile Manager.app` 拖入 `/Applications/`。

> 由于应用未经过 Apple 开发者签名，首次打开会提示「已损坏，无法打开」。解决方法：
>
> **方法一**（推荐）— 清除隔离属性：
> ```bash
> xattr -d com.apple.quarantine "/Applications/AI Profile Manager.app"
> ```
> 权限不足时加 `sudo`。
>
> **方法二** — 右键打开：在 Finder 中右键 App → 打开，弹出的对话框中点「打开」。

**Windows：** 双击 `.msi` 按向导安装。

**Linux：** 给 `.AppImage` 添加执行权限后运行：
```bash
chmod +x AI_Profile_Manager*.AppImage
./AI_Profile_Manager*.AppImage
```

#### 3. 安装后

首次启动 Desktop 应用会自动：
- 检测系统环境（`claude` / `codex` 是否已安装、`brew` / `apt` / `winget` 是否可用）
- 扫描已有配置（`~/.claude/settings.json`、`~/.codex/auth.json`、`~/.codex/config.toml`），帮你导入为 profile
- 初始化 `~/.claude-profiles/` 目录和默认 `config.yaml`
- 确保 Shell Wrapper 已安装到 `~/.zshrc`

之后在终端执行 `source ~/.zshrc`，就可以用 `ai claude <name>` 命令了。

### 方式二：源码安装（仅 CLI + Shell Wrapper）

如果你只需要命令行工具，或者想从源码构建 Desktop：

#### 1. 克隆仓库

```bash
git clone https://github.com/zhaojun2066/ai-profile-manager.git
cd ai-profile-manager
```

#### 2. 执行安装脚本

```bash
bash install.sh
```

安装脚本会自动：
- 复制文件到 `~/.claude-profiles/`
- 将 `~/.claude-profiles/bin` 加入 PATH
- 在 `~/.zshrc` 中自动激活 Shell Wrapper

**所有文件统一在一个目录下：**

```
~/.claude-profiles/
├── bin/profile          ← 管理 CLI
├── lib/config.py        ← 共享模块（YAML 读写 + 文件锁）
├── shell-rc             ← Shell Wrapper（claude/codex 拦截函数）
├── config.yaml          ← 你的所有 profile 数据
└── .config.lock         ← 文件锁
```

#### 3. 激活

```bash
source ~/.zshrc
```

> install.sh 已自动在 `~/.zshrc` 中添加了 PATH 和 source 配置，只需重新加载即可。

#### 4. 导入现有配置（可选）

如果你已经有 AI CLI 工具的配置：

```bash
profile init
```

`profile init` 会自动从已有配置中导入环境变量：
- **`~/.claude/settings.json`** — Claude Code 配置
- **`~/.codex/config.toml` + `~/.codex/auth.json`** — Codex CLI 配置

#### 5. 确认安装成功

```bash
profile list
```

应该能看到至少一个 profile。

---

## CLI 与 Desktop 对比

CLI 和 Desktop 共享同一份 `~/.claude-profiles/config.yaml`，数据实时同步。你可以根据场景混用：

| | CLI + Shell Wrapper | Desktop GUI |
|---|---|---|
| **管理 profile** | `profile add/set/remove` 命令 | 可视化表单 + 4 步创建向导 |
| **启动 AI 工具** | 终端执行 `ai claude <name>` | 点击「运行」按钮，右侧终端自动启动 |
| **查看配置** | `profile show <name>` | 表格展示，key 打码，可直接编辑 |
| **批量操作** | 逐个执行命令 | Cmd/Ctrl 多选，批量删除/导出 |
| **导入配置** | `profile init`（自动导入 Claude + Codex） | 图形化导入，预览后确认，支持 JSON 文件 |
| **终端体验** | 依赖系统终端 | 内置 xterm.js PTY 终端，支持 tab、搜索、主题 |
| **会话历史** | 依赖 shell history | 自动记录，一键恢复 |
| **配置备份** | 手动操作 | 自动备份 + 一键恢复 |
| **适用场景** | SSH 远程、纯终端环境、脚本自动化 | 日常桌面使用、频繁切换配置 |

**两种方式随时混用——Desktop 里改的配置，终端立刻生效，反之亦然。**

---

## CLI 使用

### 快速上手

```bash
# 查看所有 profile
profile list

# 启动
ai claude deepseek       # Claude Code + deepseek profile
ai claude                # 交互式选择 → Claude Code
ai codex codex-default   # Codex + codex-default profile

# 原始的 claude / codex 命令不受影响，照常使用
claude                   # 原生 Claude Code，无 profile 注入
```

每次启动时，对应的 API key、base URL、模型等环境变量自动注入，**会话退出后自动清除**，不影响其他终端窗口。

### 配置管理

#### 新增 profile

**方式一：交互式引导（推荐）**

```bash
profile add my-provider -i
```

一步步询问，回车跳过不用的字段：

```
--- Setting up profile: my-provider ---
(Press Enter to skip any field)

  Description: 我的第三方中转

Which API provider(s) will this profile use?
  [A] Anthropic (Claude Code)
  [O] OpenAI   (Codex)
  [B] Both
  Choice [A/O/B] [A]: a

--- Anthropic / Claude Code settings ---
  API Key (ANTHROPIC_AUTH_TOKEN): sk-xxxxxxxx
  Base URL (ANTHROPIC_BASE_URL): https://api.my-provider.com/anthropic
  Default model (ANTHROPIC_MODEL): claude-sonnet-4-6
  Haiku model (ANTHROPIC_DEFAULT_HAIKU_MODEL):
  Sonnet model (ANTHROPIC_DEFAULT_SONNET_MODEL):
  Opus model (ANTHROPIC_DEFAULT_OPUS_MODEL):
  Disable autoupdater? (1/0) (DISABLE_AUTOUPDATER):

--- Custom env vars ---
  Env var name (Enter to finish):

Profile 'my-provider' created with 3 env vars.
```

**方式二：逐条手动设置**

```bash
profile add my-provider "我的第三方中转"
profile set my-provider ANTHROPIC_AUTH_TOKEN=sk-xxxxxxxx
profile set my-provider ANTHROPIC_BASE_URL=https://api.my-provider.com/anthropic
profile set my-provider ANTHROPIC_MODEL=claude-sonnet-4-6
```

#### 修改 profile

```bash
# 修改已有的 key
profile set deepseek ANTHROPIC_AUTH_TOKEN=sk-new-key

# 删除某个配置项
profile unset deepseek ANTHROPIC_DEFAULT_OPUS_MODEL
```

#### 删除 profile

```bash
profile remove my-provider
```

#### 设置默认 profile

```bash
# 设为默认
profile default deepseek

# 查看当前默认
profile default
```

设了默认后，`claude` / `codex` 无参数启动时直接使用默认 profile，不再弹出选择器。

#### 查看 profile 详情

```bash
# 完整信息（敏感 key 自动打码）
profile show deepseek
```

输出示例：

```
[deepseek]
  desc: DeepSeek 中转
  env:
    ANTHROPIC_AUTH_TOKEN=sk-4****079c
    ANTHROPIC_BASE_URL=https://api.deepseek.com/anthropic
    ANTHROPIC_MODEL=deepseek-v4-pro
    ANTHROPIC_DEFAULT_HAIKU_MODEL=deepseek-v4-flash
    ANTHROPIC_DEFAULT_SONNET_MODEL=deepseek-v4-pro[1M]
    ANTHROPIC_DEFAULT_OPUS_MODEL=deepseek-v4-pro[1M]
    DISABLE_AUTOUPDATER=1
```

### Shell Wrapper

Shell Wrapper 是一组 Bash 函数，安装时自动注入到 `~/.zshrc`。它定义了 `ai()` 函数，拦截 `claude` / `codex` 子命令。

**工作流程：**

1. 用户执行 `ai claude deepseek`
2. Wrapper 从 `config.yaml` 读取 `deepseek` profile 的 `env` 字段
3. 以 `export VAR=VAL` 形式注入环境变量到 subshell
4. 启动真实的 `claude` / `codex` 进程
5. 进程退出后，subshell 销毁，环境变量自动清除

**特性：**
- **Session 级隔离** — 每个终端窗口独立注入，互不干扰
- **原生命令不受影响** — 直接调用 `claude` / `codex` 不经过 wrapper
- **交互式选择** — 无参数调用时用 fzf 弹出选择器（fallback 到手动输入）
- **默认 profile** — 设了默认后无需每次选择

实现文件：`shell/ai-profile.sh`，安装后复制到 `~/.claude-profiles/shell-rc`。

### 命令参考

#### `profile` CLI

| 命令 | 说明 | 示例 |
|------|------|------|
| `profile list` | 列出所有 profile | `profile list` |
| `profile show <name>` | 查看 profile 详情（key 打码） | `profile show deepseek` |
| `profile env <name>` | 输出 env 变量（shell eval 格式） | `profile env deepseek` |
| `profile names` | 输出 profile 名列表（fzf 用） | `profile names` |
| `profile add <name> [desc]` | 新增 profile，`-i` 交互式 | `profile add work "工作账号"` |
| `profile remove <name>` | 删除 profile | `profile remove work` |
| `profile set <name> <K=V>` | 设置环境变量 | `profile set work ANTHROPIC_MODEL=opus` |
| `profile unset <name> <K>` | 删除环境变量 | `profile unset work ANTHROPIC_MODEL` |
| `profile default [name]` | 查看/设置默认 profile | `profile default work` |
| `profile init` | 从已有配置（Claude + Codex）导入 | `profile init` |

#### Shell Wrapper

| 用法 | 说明 |
|------|------|
| `ai claude <profile>` | 用指定 profile 启动 Claude Code |
| `ai codex <profile>` | 用指定 profile 启动 Codex CLI |
| `ai claude` | 交互式选择 profile 后启动 Claude Code |
| `ai codex` | 交互式选择 profile 后启动 Codex CLI |
| `ai profile list` | 列出所有 profile（标注默认） |
| `ai profile env <name>` | 查看某个 profile 的环境变量 |
| `ai profile switch <name>` | 切换默认 profile |
| `ai` / `ai --help` | 显示帮助信息 |
| `claude` / `codex` | 原生命令，不受影响，无 profile 注入 |

---

## 配置结构

配置文件位于 `~/.claude-profiles/config.yaml`，可以直接编辑。

```yaml
# 默认 profile（无参数启动时使用）
default: deepseek

profiles:
  # ── 兼容 Anthropic 协议的 API ──
  deepseek:
    desc: "DeepSeek 中转（示例）"
    env:
      ANTHROPIC_AUTH_TOKEN: sk-xxxxxxxx
      ANTHROPIC_BASE_URL: https://api.deepseek.com/anthropic
      ANTHROPIC_MODEL: deepseek-v4-pro
      ANTHROPIC_DEFAULT_HAIKU_MODEL: deepseek-v4-flash
      ANTHROPIC_DEFAULT_SONNET_MODEL: deepseek-v4-pro[1M]
      ANTHROPIC_DEFAULT_OPUS_MODEL: deepseek-v4-pro[1M]
      DISABLE_AUTOUPDATER: "1"

  # ── 兼容 OpenAI 协议的 API ──
  codex-default:
    desc: "官方或兼容 OpenAI 协议的 API"
    env:
      OPENAI_API_KEY: sk-proj-xxxxxxxx

  # ── Codex 第三方中转 ──
  codex-custom:
    desc: "OpenAI 兼容中转"
    env:
      OPENAI_API_KEY: sk-xxxxxxxx
      OPENAI_BASE_URL: https://api.custom-provider.com/v1
      OPENAI_MODEL: gpt-5
```

### env 字段说明

| 变量 | 用途 | 适用工具 |
|------|------|----------|
| `ANTHROPIC_AUTH_TOKEN` | API key（Claude Code 用） | Claude Code |
| `ANTHROPIC_BASE_URL` | 自定义 API 端点 | Claude Code |
| `ANTHROPIC_MODEL` | 默认模型 | Claude Code |
| `ANTHROPIC_DEFAULT_HAIKU_MODEL` | Haiku 对应模型 | Claude Code |
| `ANTHROPIC_DEFAULT_SONNET_MODEL` | Sonnet 对应模型 | Claude Code |
| `ANTHROPIC_DEFAULT_OPUS_MODEL` | Opus 对应模型 | Claude Code |
| `OPENAI_API_KEY` | API key（Codex 用） | Codex |
| `OPENAI_BASE_URL` | 自定义 API 端点 | Codex |
| `OPENAI_MODEL` | 默认模型 | Codex |
| `DISABLE_AUTOUPDATER` | 禁用自动更新 | Claude Code |
| 任意自定义 key | 自由扩展 | 所有工具 |

`env` 是自由 key-value，可以添加任何环境变量，不受字段限制。

---

## 常见场景

### 场景 1：多个 API 提供商同时使用

```bash
# 终端 A：用 provider-A 的 profile
ai claude provider-a

# 终端 B：用 provider-b 的 profile
ai claude provider-b
```

两个终端互不影响，各自的 key 各自用。

### 场景 2：同一个客户端，不同项目用不同 key

```bash
profile add client-a "客户 A"
profile set client-a ANTHROPIC_AUTH_TOKEN=sk-client-a-xxx
profile set client-a ANTHROPIC_BASE_URL=https://api.client-a.com/anthropic

ai claude client-a
```

### 场景 3：用 OpenAI 兼容 API 跑 Codex

```bash
profile add codex-third "第三方 OpenAI 兼容中转"
profile set codex-third OPENAI_API_KEY=sk-xxx
profile set codex-third OPENAI_BASE_URL=https://api.third-party.com/v1

ai codex codex-third
```

### 场景 4：排查问题——确认当前 env

```bash
# 查看某个 profile 会注入哪些变量
profile env deepseek
```

---

## Desktop 应用

Desktop 应用是一个基于 **Tauri v2** 的原生桌面应用，提供可视化的 profile 管理界面和内置 PTY 终端。它与 CLI **共享同一份** `~/.claude-profiles/config.yaml`，两边数据实时同步。

- **前端：** React 18 + TypeScript + Tailwind CSS + Vite
- **后端：** Rust (Tauri v2) + `portable-pty` + `serde_yaml`
- **终端：** xterm.js + WebGL addon + Fit addon
- **支持平台：** macOS (ARM + Intel)、Windows、Linux

### 界面详解

Desktop 应用采用经典的三栏 + 终端布局：

```
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
│  Bottom Terminal (Ctrl+` 切换, VS Code 风格)                  │
│  ┌───────────┬──────────────────────────────────────────────┐│
│  │ tab1 tab2 │  $ _                                        ││
│  └───────────┴──────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────┘
```

#### 工具栏 (Toolbar)

顶部工具栏集中了所有管理操作：

| 按钮 | 功能 |
|------|------|
| `+ New` | 打开 4 步创建向导，新建 profile |
| `Copy` | 复制当前 profile |
| `Import` | 从 JSON 文件批量导入 profile |
| `Export` | 导出当前/选中 profile 为 JSON |
| `Scan` | 扫描系统已有配置，预览后导入 |
| `Backup` | 配置备份与恢复 |
| `Theme` | 切换终端配色主题（6 套可选） |
| `Terminal` | 切换底部终端面板 |
| `Help` | 快捷键帮助面板 |

#### 侧边栏 (Sidebar)

左侧 profile 列表，功能包括：

- **搜索** — 输入关键字实时过滤 profile
- **排序** — 按名称 / CLI 类型 / 环境变量数量排序
- **CLI 类型图标** — 每个 profile 旁标注 Claude 或 Codex 图标，一目了然
- **默认标记** — 星标显示当前默认 profile
- **右键菜单** — 右键 profile 弹出操作菜单（编辑 / 复制 / 导出 / 删除）
- **多选** — Cmd/Ctrl + 点击多选，批量删除或导出

#### 主面板 (Main Panel)

选中 profile 后展示详情，包含三个区块：

**环境变量表格 (Env Var Table)：**
- 两列布局：变量名 | 值
- 敏感 key（含 `KEY` / `TOKEN` / `SECRET` 等）默认打码显示，点击可切换显示/隐藏
- 双击直接编辑，回车保存
- 支持添加/删除行

**命令速查 (Command Reference)：**
- 展示该 profile 的 CLI 启动命令（`ai claude <name>` / `ai codex <name>`）
- 展示 profile 的 env 查看命令
- 一键复制命令

**会话历史 (Session History)：**
- 记录每次「运行」的历史，包含 profile 名、时间、工作目录
- 点击历史项一键恢复之前的终端会话

#### 双终端面板

Desktop 应用内置两个独立的 PTY 终端面板，各有独立 tab 和会话：

| 特性 | Right Terminal（右侧终端） | Bottom Terminal（底部终端） |
|------|--------------------------|---------------------------|
| 打开方式 | 点击 profile 的「运行」按钮 | 工具栏终端按钮 / `` Ctrl+` `` |
| 位置 | 主面板右侧 | 主面板下方（VS Code 面板风格） |
| 拖动调整 | 左右拖动分隔线 (min 480px) | 上下拖动分隔线 (min 120px) |
| 最大化 | `⌘⇧M`，隐藏侧边栏和主面板 | `⌘⇧M`，隐藏侧边栏和主面板 |
| 典型用途 | 启动 AI 工具进行交互 | 执行辅助命令、查看日志 |
| 终端搜索 | `⌘F` 搜索终端输出 | `⌘F` 搜索终端输出 |

每个终端面板支持：
- **多 Tab** — 创建多个终端会话，Tab 间独立运行
- **工作目录** — 显示当前工作目录，点击可切换
- **历史下拉** — 下拉菜单选择历史会话，一键恢复
- **6 套配色主题** — Dracula / Solarized / Monokai / One Dark / GitHub Light / Nord，独立切换

### 功能详解

#### 首次引导 (Onboarding Wizard)

首次启动 Desktop 应用时，自动弹出引导向导，分步完成：

1. **环境检测** — 检查系统中是否安装了 `claude`、`codex`、`brew`（或 `apt`、`winget`），提示用户安装缺失的依赖
2. **扫描已有配置** — 自动扫描以下位置的现有配置：
   - `~/.claude/settings.json` → 提取 Anthropic 相关环境变量（`ANTHROPIC_*`）
   - `~/.codex/auth.json` → 提取 `OPENAI_API_KEY`
   - `~/.codex/config.toml` → 提取 `model` 和 `base_url`
3. **预览确认** — 展示扫描结果，用户可选择导入哪些、自定义 profile 名称
4. **完成** — 导入选中的配置，生成初始 profile 列表

#### 配置备份与恢复

- **自动备份** — 每次写入 `config.yaml` 时自动创建备份（`config.yaml.bak`）
- **手动备份** — 工具栏一键导出完整配置为 JSON
- **一键恢复** — 从备份文件恢复上次的配置

#### 配置导入/导出

- **导出格式** — JSON，包含所有 profile 的完整数据（key 不加密，注意保管）
- **导入** — 支持 JSON 文件导入，自动识别 CLI 类型，预览后合并
- **批量操作** — 侧边栏多选后批量导出/删除

### 技术架构

```
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
```

#### 前端组件树

```
App.tsx
├── Toolbar.tsx              ← 新建/删除/复制/扫描/导入导出/主题/终端开关
├── Sidebar.tsx              ← Profile 列表 + 搜索 + 右键菜单 + CLI 类型图标
├── MainPanel.tsx            ← Profile 详情 + 环境变量表 + 命令说明 + 会话历史
├── TerminalPanel.tsx (×2)   ← Tab 栏 + xterm.js 终端 + 历史下拉
│   └── XTerm.tsx            ← xterm.js + WebGL + FitAddon 包装 (forwardRef)
├── ProfileDialog.tsx        ← 4 步创建向导：名称 → CLI 类型 → env vars → 完成
├── ImportPreview.tsx        ← JSON 导入预览 + CLI 类型检测
├── ScanPreview.tsx          ← 系统扫描结果 + 可选 profile
├── OnboardingWizard.tsx     ← 首次启动引导向导
├── ShortcutsPanel.tsx       ← 快捷键面板
├── AboutDialog.tsx          ← 关于对话框
└── common/
    ├── Button.tsx           ← 通用按钮组件
    ├── Badge.tsx            ← 标签/徽章
    ├── SearchInput.tsx      ← 搜索输入框
    └── CLIIcon.tsx          ← CLI 类型图标 (Claude/Codex)
```

#### Rust 命令模块

| 模块 | 职责 |
|------|------|
| `commands.rs` | Profile CRUD、文件 I/O、系统扫描（Claude + Codex）、环境检测、更新下载、导入导出 |
| `profile_cmd.rs` | 封装 Python `profile` CLI 的调用（子进程方式） |
| `pty.rs` | PTY 终端管理：spawn/write/resize/kill，通过 Channel 流式传输输出 |
| `lib.rs` | Tauri Builder 初始化、插件注册、命令注册 |

### 内部架构详解

#### PTY 终端数据流

```
用户按键 → xterm.onData → invoke("write_pty", {sessionId, data})
  → Rust write_pty → PTY stdin
  → Shell 输出 → PTY stdout
  → Rust reader 线程 → Channel.send(PtyEvent::Data)
  → 前端 Channel.onmessage → term.write(data)
```

每个终端会话（tab）对应一个独立的 PTY 进程。Rust 端用 `portable-pty` crate 创建伪终端，启动 `zsh -i -l`（login + interactive）。前端通过 Tauri Channel 接收 stdout/stderr 流，通过 `invoke("write_pty")` 发送键盘输入。

#### 双终端系统

App 维护两个完全独立的 `useTerminal` 实例，各有独立的 tabs、历史记录、PTY 会话：

```ts
// App.tsx
const rightTerminal = useTerminal("right");   // profile「运行」触发
const bottomTerminal = useTerminal("bottom"); // 工具栏按钮触发
```

`TerminalPanel` 通过 `mode="right"|"bottom"` prop 适配不同的边框方向、尺寸逻辑和 localStorage key。

#### PTY 尺寸同步链

```
容器 resize → ResizeObserver → RAF 合并 → fitAddon.fit()
  → xterm.js cols/rows 更新
  → onResize 回调 → invoke("resize_pty", {sessionId, cols, rows})
  → Rust: ioctl(TIOCSWINSZ) on PTY master fd
  → kernel 发送 SIGWINCH 给子进程
```

#### Profile 数据流

```
Desktop GUI (invoke) ──→ Rust commands.rs ──→ serde_yaml 读写
                                    │
CLI (bin/profile) ──→ lib/config.py ──→ hand-rolled YAML 读写
                                    │
                           ~/.claude-profiles/
                              config.yaml
                                    │
Shell Wrapper ──→ sed 读取 ─────────┘
```

三个组件通过同一份 YAML 文件通信，用 `fcntl.flock` 保证并发写入安全。

#### 跨平台设计

| 操作 | macOS | Linux | Windows |
|------|-------|-------|---------|
| HTTP 下载 | `curl -sL` | `curl -sL` | `curl -sL` |
| SHA256 校验 | `/usr/bin/shasum -a 256` | `sha256sum` / `shasum` | `certutil -hashfile SHA256` |
| 打开文件 | `/usr/bin/open` | `xdg-open` | `cmd /c start` |
| PTY 默认 shell | `/bin/zsh -i -l` | `/bin/bash -i -l` | `powershell.exe` |
| PTY resize | `ioctl(TIOCSWINSZ)` | `ioctl(TIOCSWINSZ)` | `MasterPty::resize()` |

系统二进制文件通过 `find_binary()` 函数按平台查找：macOS 依次检查 `/usr/bin/` → `/opt/homebrew/bin/` → `/usr/local/bin/`；Linux 检查 `/usr/bin/` → `/bin/`；Windows 使用裸命令名。

#### 字体渲染

xterm.js 使用 WebGL addon 获得最佳 Unicode 支持（Claude Code TUI 的制表符渲染需要）。字体栈包含 CJK 等宽字体：

```
ui-monospace, SF Mono, Cascadia Code, Menlo, Monaco,
JetBrains Mono, Fira Code, Consolas, Courier New,
PingFang SC, Microsoft YaHei, Noto Sans CJK SC, monospace
```

### 开发指南

#### 环境要求

- **Node.js** >= 22
- **Rust** (stable, via rustup)
- **macOS:** Xcode Command Line Tools
- **Linux:** `libwebkit2gtk-4.1-dev` 等 Tauri 系统依赖
- **Windows:** WebView2 (通常预装)

#### 目录结构（desktop/）

```
desktop/
├── src/                        # React 前端
│   ├── App.tsx                 # 顶层布局
│   ├── main.tsx                # 入口
│   ├── components/             # 组件
│   │   ├── Toolbar.tsx         # 工具栏
│   │   ├── Sidebar.tsx         # 侧边栏 (profile 列表)
│   │   ├── MainPanel.tsx       # 主面板 (profile 详情)
│   │   ├── TerminalPanel.tsx   # 终端面板 (tab + xterm)
│   │   ├── XTerm.tsx           # xterm.js 包装
│   │   ├── ProfileDialog.tsx   # 创建向导
│   │   ├── ImportPreview.tsx   # 导入预览
│   │   ├── ScanPreview.tsx     # 扫描结果
│   │   ├── OnboardingWizard.tsx # 首次引导
│   │   ├── ShortcutsPanel.tsx  # 快捷键面板
│   │   ├── AboutDialog.tsx     # 关于
│   │   ├── ConfirmDialog.tsx   # 确认对话框
│   │   ├── NameDialog.tsx      # 命名对话框
│   │   ├── ContextMenu.tsx     # 右键菜单
│   │   ├── EnvVarTable.tsx     # 环境变量表
│   │   ├── EnvVarRow.tsx       # 环境变量行
│   │   ├── ErrorBoundary.tsx   # 错误边界
│   │   └── common/             # 通用组件
│   ├── hooks/
│   │   ├── useProfiles.ts      # Profile CRUD
│   │   ├── useTerminal.ts      # PTY 会话管理
│   │   └── useTheme.ts         # 主题切换
│   ├── lib/
│   │   ├── types.ts            # TypeScript 类型
│   │   ├── tauri-api.ts        # Tauri invoke 封装
│   │   ├── terminalThemes.ts   # 终端配色定义
│   │   └── path-utils.ts       # 路径工具
│   └── styles/
│       └── globals.css         # CSS 变量主题
├── src-tauri/
│   ├── src/
│   │   ├── lib.rs              # Tauri Builder + 命令注册
│   │   ├── main.rs             # 入口
│   │   ├── commands.rs         # 所有 Tauri 命令处理器
│   │   ├── profile_cmd.rs      # Python profile CLI 封装
│   │   └── pty.rs              # PTY 终端管理
│   ├── Cargo.toml              # Rust 依赖
│   ├── tauri.conf.json         # Tauri 窗口/打包/更新配置
│   ├── capabilities/
│   │   └── default.json        # 权限声明 (shell/fs/dialog/updater)
│   ├── icons/                  # 应用图标 (多平台/多尺寸)
│   └── update.json             # 更新端点配置
├── update/
│   ├── update.json             # 当前更新地址 (构建时复制)
│   ├── update.prod.json        # 生产更新地址
│   └── demo.json               # 更新清单示例
├── dist/                       # 前端构建产物
├── public/                     # 静态资源 (二维码等)
├── index.html                  # HTML 入口
├── package.json                # npm 配置
├── vite.config.ts              # Vite 配置
├── tailwind.config.ts          # Tailwind 配置
├── tsconfig.json               # TypeScript 配置
├── build.sh                    # 一键构建脚本
└── BUILD.md                    # 构建发布详细文档
```

#### 常用开发命令

```bash
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
```

#### Shell Plugin Scope

当从 Rust 端调用系统命令或从前端使用 `Command.create` 时，命令二进制必须在 `capabilities/default.json` 中声明。在 `shell:allow-execute` 和 `shell:allow-spawn` 权限对象的 `allow` 数组中添加：

```json
{ "name": "命令名", "cmd": "/完整/路径", "args": true }
```

自定义 Rust `#[tauri::command]` 函数不受此限制，有完整系统访问权限。

### 构建与发布

#### 本地构建 (macOS)

```bash
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
```

构建产物位于 `src-tauri/target/release/bundle/dmg/`。

> **Tauri 不支持从 macOS 直接交叉编译 Windows/Linux 包。** 因为需要各平台的原生系统库（Windows 的 WebView2 + MSVC、Linux 的 webkit2gtk + GTK）。全平台构建请用 GitHub Actions。

#### GitHub Actions 全平台构建

项目使用 `.github/workflows/build-desktop.yml` 实现全平台 CI 构建：

- **macOS ARM + Intel** — 矩阵构建两个架构
- **Windows x64** — MSI 安装包
- **Linux x64** — AppImage

触发方式：推 tag (`v*`) 自动触发，或在 Actions 页面手动触发。

#### 版本发布流程

1. 修改 `src-tauri/tauri.conf.json` → `version`
2. 用 GitHub Actions 构建全平台包（或本地构建 macOS 双架构）
3. 下载产物，计算 SHA256：
   ```bash
   # macOS
   shasum -a 256 src-tauri/target/release/bundle/dmg/*.dmg
   # Linux
   sha256sum *.AppImage
   # Windows (PowerShell)
   Get-FileHash -Algorithm SHA256 *.msi
   ```
4. 将各平台 SHA256 填入更新清单（参考 `update/demo.json`）
5. 上传安装包到服务器/CDN
6. 更新服务器上的 `update.json`（`version`、`url`、`sha256`）

#### 代码签名

| 平台 | 需求 | 费用 |
|------|------|------|
| macOS | Apple Developer Program | $99/年 |
| Windows | 代码签名证书 | $200-400/年 |
| Linux | 无需签名 | 免费 |

未签名的 macOS 包被 Gatekeeper 拦截时，右键 → 打开即可绕过。Tauri 构建时自动添加 adhoc 签名（免费），个人使用完全正常。

### 快捷键参考

| 快捷键 | 功能 |
|--------|------|
| `⌘N` | 新建 Profile |
| `⌘B` | 切换侧边栏 |
| `` Ctrl+` `` / `⌘J` | 切换底部终端 |
| `⌘⇧M` | 最大化终端 |
| `⌘F` | 搜索终端输出 |
| `⌘K` | 快捷键帮助 |
| `Backspace` | 删除选中的 Profile |
| `⌘,` | (预留) 设置 |

桌面应用与 CLI 使用同一份配置文件和文件锁，并发安全。

### 更新机制

Desktop 应用内置自动更新功能。工作流程：

1. 应用启动时读取 `update/update.json` 中的 `update_url`
2. 向更新服务器请求 `update.json` 清单
3. 比较版本号，若有新版本则弹出更新对话框
4. 通过 `curl` 下载安装包
5. `shasum` / `sha256sum` / `certutil` 校验 SHA256
6. `open` / `xdg-open` / `cmd /c start` 打开安装包让用户手动安装

更新地址通过构建脚本管理：
- `npm run tauri:build:prod` — 自动使用 `update/update.prod.json`
- `npm run tauri:build:staging` — 使用 `update/update.staging.json`
- 手动覆盖：`echo '{"update_url":"http://localhost:8000/update.json"}' > update/update.json`

### 已知陷阱

#### 1. PTY Shell PATH 缺失 (`command not found`)

**现象：** dev 模式终端正常，生产包（`.app`）里 `command not found`。

**根因：** Tauri GUI 进程的 PATH 极简（macOS: `/usr/bin:/bin:/usr/sbin:/sbin`），不含 Homebrew 等用户安装的工具路径。`pty.rs` 复制父进程环境变量时把这份受限 PATH 传给了 PTY shell。

**修复：** PTY 启动 shell 时必须传 `-i -l`（login + interactive）两个标志，确保 shell 按标准流程加载 `~/.zprofile`（macOS）或 `~/.profile`（Linux）中的 PATH 扩展。

```rust
// pty.rs — 正确做法
cmd.args(["-i", "-l"]);  // login + interactive
// 不要只传 -i（不加载 .zprofile / .profile，生产环境 PATH 缺失）
```

#### 2. PTY 终端无法输入

**现象：** 生产包中内置终端键盘无法输入，dev 模式正常。

**根因：** macOS `.app` 进程没有 `TERM` 环境变量（这是终端模拟器特有的），PTY 复制了残缺的父进程环境。Shell 依赖 `TERM` 来判断终端能力（zsh 的 ZLE、bash 的 readline），没有 `TERM` 可能导致行编辑功能不可用。

**修复：** 在复制父进程环境变量后，强制覆盖终端必需的环境变量：

```rust
// pty.rs — 在 std::env::vars() 循环之后
cmd.env("TERM", "xterm-256color");
cmd.env("COLORTERM", "truecolor");
```

#### 3. Tauri 环境变量不可靠

**核心教训：**
- GUI 应用进程的环境变量是"残缺的"——只包含系统级变量
- **永远不要假设** `std::env::vars()` 包含完整的终端/用户环境
- **永远用 `-i -l` 启动 PTY shell**，让 shell 按标准流程初始化用户环境
- `TERM` 必须在 PTY 中显式设置为 `xterm-256color`（匹配 xterm.js 能力）

---

## 项目结构

```
kn/
├── README.md                   ← 本文件
├── install.sh                  ← 一键安装脚本
├── LICENSE                     ← MIT
│
├── bin/
│   └── profile                 ← CLI 入口 (Python)
│
├── lib/
│   └── config.py               ← 共享配置模块 (YAML + 文件锁)
│
├── shell/
│   └── ai-profile.sh           ← Shell Wrapper (Bash)
│
├── templates/
│   └── config.yaml             ← 默认配置模板
│
├── tests/
│   ├── test_config.py          ← 配置模块单元测试
│   └── test_json_output.py     ← JSON 输出测试
│
├── desktop/                    ← Desktop GUI (Tauri v2)
│   ├── src/                    # React 前端
│   ├── src-tauri/              # Rust 后端
│   ├── update/                 # 更新配置
│   ├── build.sh                # 构建脚本
│   ├── BUILD.md                # 构建文档
│   └── CLAUDE.md               # AI 辅助开发文档
│
├── site/                       ← 产品官网 (Vue 3)
│   ├── src/
│   │   ├── views/              # 页面 (LandingPage, DocsPage)
│   │   ├── components/         # 组件 (Hero, Features, Docs 等)
│   │   ├── data/docs.ts        # 文档数据
│   │   └── router/             # 路由 (hash-router)
│   └── public/                 # 静态资源
│
├── docs/                       ← 设计文档
│   └── superpowers/            # 计划与规格
│
└── .github/workflows/
    ├── build-desktop.yml       ← 全平台桌面构建
    └── deploy-site.yml         ← 官网部署到 GitHub Pages
```

---

## FAQ

**Q: 多个终端同时改 profile 会冲突吗？**

不会。写操作通过文件锁（`fcntl.flock`）保护，同时写入会排队等待，不会损坏数据。

**Q: API key 安全吗？**

key 明文存储在 `~/.claude-profiles/config.yaml` 中。建议确保该目录权限为 `700`：
```bash
chmod 700 ~/.claude-profiles
```

**Q: 怎么让 profile 对整个项目目录生效？**

可以使用 direnv。在项目根目录创建 `.envrc`：
```bash
export ANTHROPIC_AUTH_TOKEN=$(profile env work 2>/dev/null | grep AUTH_TOKEN | cut -d= -f2 | tr -d "'")
```

**Q: 忘记有哪些 profile 了？**

```bash
profile list
```

**Q: 如何修改 profile 的描述？**

编辑 `~/.claude-profiles/config.yaml`，直接改 `desc:` 字段即可。

**Q: CLI 的 `profile init` 和 Desktop 的扫描有什么区别？**

功能相同，都会导入 Claude Code 和 Codex 的已有配置。区别在于：
- **CLI `profile init`** — 自动导入，不预览，直接写入 config.yaml
- **Desktop「Scan」** — 扫描后展示预览，可勾选要导入的项、自定义 profile 名称，确认后再写入

**Q: `profile init` 导入后还能重新导入吗？**

不会覆盖已有 profile。如果有同名 profile，手动删除后再 `profile init`：
```bash
profile remove deepseek
profile init
```

**Q: 卸载/重置？**

```bash
rm -rf ~/.claude-profiles
# 然后编辑 ~/.zshrc，删除 # >>> AI Profile Manager >>> 到 # <<< AI Profile Manager <<< 之间的几行
```

---

[MIT License](LICENSE)
