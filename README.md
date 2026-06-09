# AI Profile Manager

多 profile 管理系统，让你在不同终端会话中为 `claude` / `codex` / `qoder` 使用不同的 API key、base URL 和模型配置。

> 🌐 官网：[https://zhaojun2066.github.io/ai-profile-manager/](https://zhaojun2066.github.io/ai-profile-manager/)

---

## 目录

- [项目概览](#项目概览)
- [安装](#安装)
- [快速上手](#快速上手)
- [CLI 使用](#cli-使用)
- [Shell 增强功能](#shell-增强功能)
- [配置结构](#配置结构)
- [常见场景](#常见场景)
- [Desktop 应用](#desktop-应用)
- [高级功能](#高级功能)
- [项目结构](#项目结构)
- [开发指南](#开发指南)
- [构建与发布](#构建与发布)
- [FAQ](#faq)

---

## 项目概览

### 这是什么？

AI Profile Manager 通过**环境变量注入**，在不同终端会话中为 AI CLI 工具无缝切换 API 配置。任何兼容协议的 API 提供商都可以——官方服务、第三方中转、自部署网关，一个 profile 搞定。

**支持的 AI CLI 工具：**

| 工具 | CLI 命令 | 协议 |
|------|---------|------|
| Claude Code | `claude` | Anthropic |
| Codex CLI | `codex` | OpenAI |
| Qoder CN (国产) | `qoderclicn` | 阿里通义 |

**典型场景：**

- 同时使用**多个 API 提供商**（如官方 Anthropic + DeepSeek 中转），不想手动改配置
- 不同项目用**不同的 API Key**——进入目录自动切换
- 用**第三方兼容 API**（任何支持 Anthropic/OpenAI 协议的服务）
- 多团队成员共用一台机器，各自独立配置

### 核心架构

```
                         ~/.claude-profiles/config.yaml  (唯一数据源)
                              ↑ 读写 ↑
         ┌─────────────────────┼────────┼──────────────────┐
         ▼                     ▼        ▼                  ▼
   bin/profile          lib/config.py  desktop/src-tauri  shell/ai-profile.sh
   (Python CLI)         (YAML+lock)    (Rust serde_yaml)  (Bash sed reader)
```

**四个组件，一份数据，文件锁保证并发安全。**

| 组件 | 语言 | 作用 |
|------|------|------|
| `profile` CLI | Python 3 | 命令行增删改查 profile |
| Shell Wrapper | Bash + PowerShell | 拦截 `claude`/`codex`/`qoder`，注入环境变量到子进程 |
| Desktop GUI | TypeScript + Rust (Tauri v2) | 可视化管理 + Hook/Agent/Skill 管理 + 用量追踪 |
| 产品官网 | Vue 3 + Vite | 落地页 + 文档，部署到 GitHub Pages |

### 技术栈

| 层 | 技术 |
|----|------|
| CLI | Python 3, hand-rolled YAML parser (zero-dependency), fcntl file locking |
| Shell Wrapper | Bash (macOS/Linux), PowerShell (Windows), `sed`-based YAML reading |
| Desktop Frontend | React 18 + TypeScript + Tailwind CSS + Vite |
| Desktop Backend | Rust (Tauri v2), `portable-pty`, `serde_yaml`, `reqwest`, `sha2` |
| 终端模拟 | xterm.js + Canvas renderer + Fit addon + Search addon |
| 站点 | Vue 3 + TypeScript + Vite + Tailwind CSS |
| CI/CD | GitHub Actions (macOS ARM+Intel / Windows / Linux 全平台构建) |

---

## 安装

### 方式一：安装包（推荐，含 Desktop + CLI）

从 [GitHub Releases](https://github.com/zhaojun2066/ai-profile-manager/releases/latest) 下载对应平台安装包：

| 平台 | 格式 |
|------|------|
| macOS Apple Silicon (M1/M2/M3) | `.dmg` (aarch64) |
| macOS Intel | `.dmg` (x86_64) |
| Windows | `.msi` |
| Linux | `.AppImage` |

**macOS 安装：** 打开 `.dmg`，拖入 `/Applications/`。

> 首次打开若提示「已损坏」，运行：
> ```bash
> sudo xattr -d com.apple.quarantine "/Applications/AI Profile Manager.app"
> ```
> 或右键 App → 打开。

**Windows：** 双击 `.msi` 按向导安装。

**Linux：**
```bash
chmod +x AI_Profile_Manager*.AppImage
./AI_Profile_Manager*.AppImage
```

首次启动 Desktop 应用会自动完成：环境检测 → 配置扫描导入 → Shell Wrapper + 补全安装。

### 方式二：源码安装（仅 CLI + Shell Wrapper）

```bash
git clone https://github.com/zhaojun2066/ai-profile-manager.git
cd ai-profile-manager
bash install.sh
source ~/.zshrc
```

安装脚本会自动配置 Shell Wrapper **和** Shell 补全（zsh + bash）。详见 [Shell 增强功能](#shell-增强功能)。

**安装后目录：**
```
~/.claude-profiles/
├── bin/profile          ← 管理 CLI
├── lib/config.py        ← 共享模块 (YAML + 文件锁)
├── shell-rc             ← Shell Wrapper
├── completions/         ← Shell 补全
│   ├── _ai               ← Zsh completion
│   └── ai.bash           ← Bash completion
├── hooks/               ← Hook 脚本
│   ├── record-usage.py   ← Token 用量记录
│   └── run-with-log.sh   ← Hook 执行日志包装
├── hook-logs/           ← Hook 执行日志
├── config.yaml          ← 你的 profile 数据
└── .config.lock         ← 文件锁
```

---

## 快速上手

```bash
# 导入已有配置
profile init

# 查看
profile list

# 启动
ai claude deepseek       # Claude Code + deepseek profile
ai codex codex-default   # Codex + default profile
ai claude                # 交互式选择 profile
```

每次启动时环境变量自动注入，**会话退出后自动清除**，不影响其他终端。

---

## CLI 使用

### 命令参考

#### `profile` CLI

| 命令 | 说明 | 示例 |
|------|------|------|
| `profile list` | 列出所有 profile | `profile list` |
| `profile show <name>` | 查看详情 (key 打码) | `profile show deepseek` |
| `profile env <name>` | 输出环境变量 | `profile env deepseek` |
| `profile names` | 输出 profile 名列表 | `profile names` |
| `profile add <name> [desc]` | 新增 profile，`-i` 交互式 | `profile add work "工作号"` |
| `profile remove <name>` | 删除 profile | `profile remove work` |
| `profile set <name> <K=V>` | 设置环境变量 | `profile set work ANTHROPIC_MODEL=opus` |
| `profile unset <name> <K>` | 删除环境变量 | `profile unset work ANTHROPIC_MODEL` |
| `profile default [name]` | 查看/设置默认 profile | `profile default work` |
| `profile init` | 从已有配置导入 | `profile init` |

#### Shell Wrapper — `ai` 命令

| 用法 | 说明 |
|------|------|
| `ai claude <profile>` | 指定 profile 启动 Claude Code |
| `ai codex <profile>` | 指定 profile 启动 Codex CLI |
| `ai qoderclicn <profile>` | 指定 profile 启动 Qoder CN |
| `ai claude` | 自动检测：项目级 → 默认 → 交互选择 |
| `ai profile list` | 列出所有 profile (标注默认) |
| `ai profile env <name>` | 查看 profile 环境变量 |
| `ai profile switch <name>` | 切换默认 profile |
| `ai tips` | 模型选择推荐 + 常用 profile 排行 |
| `ai` / `ai --help` | 帮助信息 |
| `claude` / `codex` | 原生命令，不受影响 |

### 交互式创建

```bash
profile add my-provider -i
```

一步步询问，回车跳过不用的字段：

```
--- Setting up profile: my-provider ---
(Press Enter to skip any field)

  Description: 第三方中转站

Which API provider(s) will this profile use?
  [A] Anthropic (Claude Code)
  [O] OpenAI (Codex)
  [B] Both
  Choice [A/O/B] [A]:

--- Anthropic / Claude Code settings ---
  API Key (ANTHROPIC_AUTH_TOKEN): sk-xxxxxxxx
  Base URL (ANTHROPIC_BASE_URL): https://api.example.com/anthropic
  Default model (ANTHROPIC_MODEL): claude-sonnet-4-6
```

---

## Shell 增强功能

### Shell 补全

安装脚本自动配置 zsh 和 bash 的 `ai` 命令补全：

- **Zsh：** 自动添加 `fpath` + `compinit`（如 `.zshrc` 已有则跳过 compinit 避免重复初始化）
- **Bash：** 自动 source `ai.bash` 补全脚本

输入 `ai ` 后按 Tab 即可补全子命令、profile 名和选项。

### 项目级 Profile 自动切换

在项目根目录创建 `.ai-profile` 文件，写入 profile 名：

```bash
echo "my-proj-profile" > /path/to/project/.ai-profile
```

之后在该目录（及子目录）执行 `ai claude` 时，会自动使用 `my-proj-profile`，无需每次指定。

**优先级：** 显式指定 profile > `.ai-profile` 项目绑定 > 默认 profile > 交互选择

### `ai tips` — 模型推荐 + 使用排行

```bash
$ ai tips
AI Model Tips:
  编程开发   → claude-sonnet-4-6 / deepseek-v3
  复杂推理   → claude-opus-4-8 / deepseek-reasoner
  快速修改   → claude-haiku-4-5
  中文场景   → deepseek-chat / deepseek-v4-pro

  你最常用:
    deepseek (23 次)
    work (12 次)
    codex-default (5 次)
```

从 shell history 中自动统计你最常用的 profile。

---

## 配置结构

配置文件位于 `~/.claude-profiles/config.yaml`：

```yaml
default: deepseek

profiles:
  deepseek:
    desc: "DeepSeek 中转"
    env:
      ANTHROPIC_AUTH_TOKEN: sk-xxxxxxxx
      ANTHROPIC_BASE_URL: https://api.deepseek.com/anthropic
      ANTHROPIC_MODEL: deepseek-v4-pro
      ANTHROPIC_DEFAULT_HAIKU_MODEL: deepseek-v4-flash
      ANTHROPIC_DEFAULT_SONNET_MODEL: deepseek-v4-pro[1M]
      ANTHROPIC_DEFAULT_OPUS_MODEL: deepseek-v4-pro[1M]
      DISABLE_AUTOUPDATER: "1"

  codex-default:
    desc: "OpenAI 兼容 API"
    env:
      OPENAI_API_KEY: sk-proj-xxxxxxxx

  codex-custom:
    desc: "第三方中转"
    env:
      OPENAI_API_KEY: sk-xxxxxxxx
      OPENAI_BASE_URL: https://api.custom-provider.com/v1
      OPENAI_MODEL: gpt-5
```

### 环境变量参考

| 变量 | 用途 | 适用工具 |
|------|------|----------|
| `ANTHROPIC_AUTH_TOKEN` | API key | Claude Code |
| `ANTHROPIC_BASE_URL` | 自定义 API 端点 | Claude Code |
| `ANTHROPIC_MODEL` | 默认模型 | Claude Code |
| `ANTHROPIC_DEFAULT_HAIKU_MODEL` | Haiku 对应模型 | Claude Code |
| `ANTHROPIC_DEFAULT_SONNET_MODEL` | Sonnet 对应模型 | Claude Code |
| `ANTHROPIC_DEFAULT_OPUS_MODEL` | Opus 对应模型 | Claude Code |
| `OPENAI_API_KEY` | API key | Codex |
| `OPENAI_BASE_URL` | 自定义 API 端点 | Codex |
| `OPENAI_MODEL` | 默认模型 | Codex |
| `DISABLE_AUTOUPDATER` | 禁用自动更新 | Claude Code |
| 任意自定义 key | 自由扩展 | 所有工具 |

---

## 常见场景

### 多个 API 提供商同时使用

```bash
# 终端 A：provider-A
ai claude provider-a

# 终端 B：provider-B
ai claude provider-b
```

互不影响，各自 key 各自用。

### 不同项目用不同 key

```bash
profile add client-a "客户 A"
profile set client-a ANTHROPIC_AUTH_TOKEN=sk-client-a-xxx

echo "client-a" > ~/project-for-client-a/.ai-profile
cd ~/project-for-client-a
ai claude   # 自动使用 client-a profile
```

### OpenAI 兼容 API 跑 Codex

```bash
profile add codex-third "第三方中转"
profile set codex-third OPENAI_API_KEY=sk-xxx
profile set codex-third OPENAI_BASE_URL=https://api.third-party.com/v1
ai codex codex-third
```

### 排查问题

```bash
profile env deepseek   # 查看会注入哪些变量
ai profile list        # 查看所有 profile 和默认值
```

---

## Desktop 应用

Desktop 应用提供完整的 GUI 管理体验，包含内置 PTY 终端、可视化配置编辑、Hook/Agent/Skill 管理、用量追踪等功能。与 CLI **共享同一份** `config.yaml`，两边数据实时同步。

### 界面概览

```
┌──────────────────────────────────────────────────────────────────────┐
│  Toolbar  [+New] [Scan] [Import] [Export] [⚙] [Terminal] [Usage]   │
├──────────┬─────────────────────────────────┬─────────────────────────┤
│ Sidebar  │         Main Panel              │    Right Terminal       │
│          │                                 │    (点击「运行」打开)     │
│ profile  │  ┌─ Env Var Table ────────────┐ │  ┌── tab1 ── tab2 ──┐  │
│  列表    │  │ KEY          │ VALUE       │ │  │                   │  │
│ 🔍搜索   │  │ ANTHROPIC_.. │ sk-4****79c │ │  │  $ ai claude ...  │  │
│ ⭐默认   │  │ ANTHROPIC_.. │ https://... │ │  │                   │  │
│          │  └────────────────────────────┘ │  └───────────────────┘  │
│          │  ┌─ Commands ─────────────────┐ │                         │
│          │  │ $ ai claude deepseek   [▶] │ │                         │
│          │  └────────────────────────────┘ │                         │
│          │  ┌─ Session History ──────────┐ │                         │
│          │  │ 06-09 14:32  ~/project  [▶]│ │                         │
│          │  └────────────────────────────┘ │                         │
├──────────┴─────────────────────────────────┴─────────────────────────┤
│  Bottom Terminal (Ctrl+` 切换)                                        │
│  ┌── tab1 ── tab2 ─────────────────────────────────────────────────┐ │
│  │  $ _                                                             │ │
│  └──────────────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────────┘
```

### 核心功能

#### Profile 管理
- **4 步创建向导** — 名称 → CLI 类型 → 环境变量 → 完成
- **可视化编辑** — 表格展示，敏感 key 自动打码，双击编辑
- **批量操作** — Cmd/Ctrl 多选，批量删除/导出
- **导入/导出** — JSON 格式，支持 Claude Code / Codex / Qoder 自动识别
- **系统扫描** — 从 `~/.claude/settings.json`、`~/.codex/auth.json`、`~/.qoder-cn/` 自动发现已有配置

#### 双终端面板

两个独立 PTY 终端，各有独立 tab 和会话：

| 特性 | Right Terminal | Bottom Terminal |
|------|---------------|-----------------|
| 打开方式 | profile「运行」按钮 | 工具栏 / `` Ctrl+` `` |
| 位置 | 主面板右侧 | 主面板下方 (VS Code 风格) |
| 终端搜索 | `⌘F` | `⌘F` |
| 6 套主题 | Dracula / Solarized / Monokai / One Dark / GitHub Light / Nord |

每个终端支持多 Tab、工作目录切换、会话历史恢复。PTY 使用 login + interactive shell (`zsh -i -l`)，确保完整用户 PATH。

#### Quick Switcher (`⌘K`)

全局快速启动器，输入即搜：
- **Profile 搜索** — 模糊匹配，自动按使用频率排序（常用 profile 优先显示 + ⭐标记）
- **项目搜索** — 模糊匹配项目目录，回车直接启动
- **最近使用** — 基于终端会话历史自动推荐

---

## 高级功能

### 🔧 Hook 管理

Hook 是 Claude Code / Codex / Qoder 的事件触发器。Desktop 应用提供完整的可视化管理：

- **查看** — 按 CLI 类型 + 事件类型浏览所有 Hook
- **创建** — 向导式创建 Hook（文本命令 / 脚本文件），支持日志包装
- **编辑** — 直接修改 Hook 命令，保留其他配置不变
- **复制/移动** — 在用户级和项目级之间复制 Hook
- **禁用/启用** — 一键开关，无需删除
- **执行日志** — 通过 `run-with-log.sh` 包装后，记录每次执行的输出、耗时、退出码
- **多项目** — 添加项目目录，管理项目级 Hook

支持的 Hook 事件类型：`Stop`、`SessionEnd`、`PreTool`、`PostTool`、`Notification` 等。

### 🤖 Agent 管理

扫描和查看 Claude Code / Codex / Qoder 的 Agent 配置：

- **用户级 Agent** — `~/.claude/agents/`、`~/.codex/agents/`、`~/.qoder-cn/agents/`
- **项目级 Agent** — `<project>/.claude/agents/`、`<project>/.codex/agents/`、`<project>/.qoder/agents/`
- **内置 Agent** — 系统内置 Agent 只读展示，不可修改

### 📊 Token 用量追踪

自动追踪每个 `ai` 调用的 token 消耗，通过 Hook 机制无感记录：

- **用量仪表盘** — 按模型 / 按项目两种维度查看
- **项目归因** — 自动关联 token 消耗到具体项目目录
- **成本估算** — 可配置各模型价格，自动计算费用
- **每日趋势** — 折线图展示近期用量变化

用量数据存储在 `~/.claude-profiles/usage.jsonl`，项目归因数据存储在 `~/.claude-profiles/projects.json`。

---

## 项目结构

```
kn/
├── README.md
├── install.sh                  ← 一键安装脚本 (含补全配置)
├── LICENSE                     ← MIT
├── RELEASE.md                  ← 发布流程指南
├── CLAUDE.md                   ← AI 辅助开发文档
│
├── bin/
│   └── profile                 ← CLI 入口 (Python 3)
│
├── lib/
│   └── config.py               ← 共享模块 (YAML 解析 + fcntl 文件锁)
│
├── shell/
│   ├── ai-profile.sh           ← Shell Wrapper (Bash)
│   ├── ai-profile.ps1          ← Shell Wrapper (PowerShell)
│   └── completions/
│       ├── _ai                  ← Zsh completion
│       └── ai.bash              ← Bash completion
│
├── templates/
│   └── config.yaml             ← 默认配置模板
│
├── tests/
│   ├── test_config.py          ← 配置模块单元测试
│   ├── test_json_output.py     ← JSON 输出集成测试
│   └── test_shell_smoke.sh     ← Shell Wrapper 冒烟测试
│
├── desktop/                    ← Desktop GUI (Tauri v2)
│   ├── src/                    # React 前端
│   │   ├── App.tsx             # 顶层布局 + 事件路由
│   │   ├── components/
│   │   │   ├── Toolbar.tsx     # 工具栏
│   │   │   ├── Sidebar.tsx     # Profile 列表
│   │   │   ├── MainPanel.tsx   # Profile 详情
│   │   │   ├── TerminalPanel.tsx / XTerm.tsx  # 终端
│   │   │   ├── ProfileDialog.tsx     # 创建向导
│   │   │   ├── QuickSwitcher.tsx     # ⌘K 快速启动
│   │   │   ├── HookWizard.tsx        # Hook 创建向导
│   │   │   ├── HookDetail.tsx        # Hook 详情 + 执行日志
│   │   │   ├── HookList.tsx          # Hook 列表
│   │   │   ├── UsagePanel.tsx        # 用量仪表盘
│   │   │   ├── ScanPreview.tsx       # 系统扫描
│   │   │   └── common/               # 通用组件
│   │   ├── hooks/
│   │   │   ├── useTerminal.ts  # PTY 多实例管理
│   │   │   ├── useProfiles.ts  # Profile CRUD
│   │   │   ├── useUsage.ts     # 用量数据
│   │   │   └── useTheme.ts     # 主题切换
│   │   └── lib/
│   │       ├── types.ts        # TypeScript 类型
│   │       ├── tauri-api.ts    # Tauri invoke 封装
│   │       └── terminalThemes.ts
│   ├── src-tauri/              # Rust 后端
│   │   ├── src/
│   │   │   ├── lib.rs          # Tauri Builder + 命令注册
│   │   │   ├── commands.rs     # Profile CRUD + 系统扫描 + 更新
│   │   │   ├── profile_cmd.rs  # Shell RC + 补全 + Hook Recorder
│   │   │   ├── pty.rs          # PTY 终端管理
│   │   │   ├── hook_manager.rs  # Hook CRUD (Claude/Codex/Qoder)
│   │   │   ├── hook_logs.rs    # Hook 执行日志
│   │   │   ├── hook_meta.rs    # Hook 元数据存储
│   │   │   ├── hook_store.rs   # Hook 市场管理
│   │   │   ├── agent_manager.rs # Agent 扫描 (Claude/Codex/Qoder)
│   │   │   ├── skill_manager.rs # Skill/Plugin 扫描
│   │   │   └── usage.rs        # Token 用量统计
│   │   ├── Cargo.toml
│   │   ├── tauri.conf.json
│   │   └── capabilities/default.json
│   ├── update/                 # 更新配置
│   └── CLAUDE.md
│
├── site/                       ← 产品官网 (Vue 3 + Vite)
│   └── src/
│
└── .github/workflows/
    ├── build-desktop.yml       ← 全平台构建 + Release
    └── deploy-site.yml         ← 官网部署
```

---

## 开发指南

### Desktop 应用

#### 环境要求
- **Node.js** >= 22
- **Rust** (stable)
- **macOS:** Xcode Command Line Tools
- **Linux:** `libwebkit2gtk-4.1-dev` 等 Tauri 系统依赖
- **Windows:** WebView2 (通常预装)

#### 常用命令

```bash
cd desktop

# 开发模式 (前端热更新 + Rust 监听)
npm run tauri dev

# 仅类型检查
npx tsc --noEmit

# 仅构建前端
npx vite build

# 仅检查 Rust
cd src-tauri && cargo check

# 全量检查
npx tsc --noEmit && npx vite build && cd src-tauri && cargo check
```

### CLI

```bash
# 直接运行
PYTHONPATH=lib python3 bin/profile list

# 运行测试
PYTHONPATH=lib python3 -m pytest tests/ -v

# Shell 脚本语法检查
bash -n install.sh
bash -n shell/ai-profile.sh
```

### 架构要点

- **数据流：** 四个组件共享 `~/.claude-profiles/config.yaml`，通过 `fcntl.flock`（Unix）/ `fs2`（Rust）保证并发写入安全
- **PTY 终端：** `-i -l` 启动 login + interactive shell，确保完整用户 PATH；`TERM=xterm-256color` 显式设置
- **配置写入安全：** 3 代轮转备份 (`.bak → .bak.1 → .bak.2 → .bak.3`)，防止错误覆盖丢失恢复路径
- **跨平台：** `find_binary()` 按平台查找系统命令，`home_dir()` 统一 HOME/USERPROFILE 回退链
- **Shell 包装器：** Rust 端通过 `include_str!` 嵌入编译，内容变更时自动更新

---

## 构建与发布

```bash
# macOS 本地构建
cd desktop
npm run tauri:build:prod        # 当前架构
npm run tauri:build:prod:arm    # Apple Silicon
npm run tauri:build:prod:intel  # Intel Mac
```

全平台构建由 GitHub Actions 自动完成（`.github/workflows/build-desktop.yml`）。

**发布流程：** 更新版本号 → 打 tag → push → CI 自动构建 + 生成 Release Notes。
详见 [CLAUDE.md](CLAUDE.md) 和 [RELEASE.md](RELEASE.md)。

---

## FAQ

**Q: 多个终端同时改 profile 会冲突吗？**
不会。写操作通过文件锁保护，同时写入会排队等待。

**Q: API key 安全吗？**
key 明文存储在 `~/.claude-profiles/config.yaml` 中。建议将该目录权限设为 700。

**Q: 如何让 profile 自动对某个项目生效？**
在项目根目录创建 `.ai-profile` 文件，写入 profile 名。该目录及子目录中 `ai claude` 会自动使用。

**Q: Shell 补全怎么配置？**
安装脚本自动配置。如需手动：zsh 在 `.zshrc` 中添加 `fpath`，bash source `ai.bash`。

**Q: 如何查看 token 用量？**
Desktop 应用中打开用量面板，支持按模型/按项目维度查看。CLI 目前暂无用量查看命令。

**Q: `ai` 命令和原生命令的关系？**
`ai claude` 经过 wrapper 注入 profile 环境变量后启动 `claude`。直接执行 `claude` 不经过 wrapper，使用系统默认配置。

---

[MIT License](LICENSE)
