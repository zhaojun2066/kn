# AI Profile Manager 使用指南

多 profile 管理系统，让你在不同终端会话中为 `claude` / `codex` 使用不同的 API key、base URL 和模型配置。

> 🌐 官网：[https://zhaojun2066.github.io/ai-profile-manager/](https://zhaojun2066.github.io/ai-profile-manager/)

## 目录

- [安装](#安装)
- [快速开始](#快速开始)
- [配置管理](#配置管理)
- [日常使用](#日常使用)
- [命令参考](#命令参考)
- [配置结构](#配置结构)
- [常见场景](#常见场景)
- [FAQ](#faq)
- [Desktop GUI 应用](#desktop-gui-应用)

---

## 安装

### 1. 执行安装脚本

```bash
cd ~/workspace/me/shark/kn   # 或你的项目路径
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

### 2. 激活

```bash
source ~/.zshrc
```

> install.sh 已自动在 `~/.zshrc` 中添加了 PATH 和 source 配置，只需重新加载即可。

### 3. 导入现有配置（可选）

如果你已经有 `~/.claude/settings.json`：

```bash
profile init
```

这会自动把你的 DeepSeek 等现有配置导入为 profile。

### 4. 确认安装成功

```bash
profile list
```

应该能看到至少一个 profile。

---

## 快速开始

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

---

## 配置管理

### 新增 profile

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

### 修改 profile

```bash
# 修改已有的 key
profile set deepseek ANTHROPIC_AUTH_TOKEN=sk-new-key

# 删除某个配置项
profile unset deepseek ANTHROPIC_DEFAULT_OPUS_MODEL_NAME
```

### 删除 profile

```bash
profile remove my-provider
```

### 设置默认 profile

```bash
# 设为默认
profile default deepseek

# 查看当前默认
profile default
```

设了默认后，`claude` / `codex` 无参数启动时直接使用默认 profile，不再弹出选择器。

### 查看 profile 详情

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

---

## 日常使用

### Claude Code 多账号

```bash
# 终端 1：用 DeepSeek
ai claude deepseek

# 终端 2：用另一个中转
ai claude another

# 终端 3：交互式选
ai claude
```

### Codex 多后端

```bash
# 官方 OpenAI
ai codex codex-default

# 第三方兼容 API
ai codex codex-custom
```

### 不用 profile 时

直接调原始命令即可，`ai` wrapper 不会拦截它们：

```bash
claude    # 原生 Claude Code
codex     # 原生 Codex
```

---

## 命令参考

### `profile` CLI

| 命令 | 说明 | 示例 |
|------|------|------|
| `profile list` | 列出所有 profile | `profile list` |
| `profile show <name>` | 查看 profile 详情 | `profile show deepseek` |
| `profile env <name>` | 输出 env 变量（shell eval 格式） | `profile env deepseek` |
| `profile names` | 输出 profile 名列表（fzf 用） | `profile names` |
| `profile add <name> [desc]` | 新增 profile | `profile add work "工作账号"` |
| `profile remove <name>` | 删除 profile | `profile remove work` |
| `profile set <name> <K=V>` | 设置环境变量 | `profile set work ANTHROPIC_MODEL=opus` |
| `profile unset <name> <K>` | 删除环境变量 | `profile unset work ANTHROPIC_MODEL` |
| `profile default [name]` | 查看/设置默认 profile | `profile default work` |
| `profile init` | 从 `~/.claude/settings.json` 导入 | `profile init` |

### Shell Wrapper

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
  # ── DeepSeek ──
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

  # ── OpenAI 官方 Codex ──
  codex-default:
    desc: "OpenAI 官方 Codex"
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
| `ANTHROPIC_AUTH_TOKEN` | Anthropic API key | Claude Code |
| `ANTHROPIC_BASE_URL` | 自定义 API 端点 | Claude Code |
| `ANTHROPIC_MODEL` | 默认模型 | Claude Code |
| `ANTHROPIC_DEFAULT_HAIKU_MODEL` | Haiku 对应模型 | Claude Code |
| `ANTHROPIC_DEFAULT_SONNET_MODEL` | Sonnet 对应模型 | Claude Code |
| `ANTHROPIC_DEFAULT_OPUS_MODEL` | Opus 对应模型 | Claude Code |
| `OPENAI_API_KEY` | OpenAI API key | Codex |
| `OPENAI_BASE_URL` | 自定义 API 端点 | Codex |
| `OPENAI_MODEL` | 默认模型 | Codex |
| `DISABLE_AUTOUPDATER` | 禁用自动更新 | Claude Code |
| 任意自定义 key | 自由扩展 | 所有工具 |

`env` 是自由 key-value，可以添加任何环境变量，不受字段限制。

---

## 常见场景

### 场景 1：DeepSeek + 官方 Anthropic 双开

```bash
# 终端 A：DeepSeek
ai claude deepseek

# 终端 B：官方
ai claude anthropic-official
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

## Desktop GUI 应用

除了命令行，还可以使用桌面应用管理 profile。桌面应用与 CLI **共享同一份 `~/.claude-profiles/config.yaml`**，两边数据实时同步。

### 下载安装

从 [GitHub Releases](https://github.com/zhaojun2066/ai-profile-manager/releases/latest) 下载 `.dmg` 安装包。

> **macOS 用户注意**：由于应用未经过 Apple 开发者签名和公证，首次打开时会提示「已损坏，无法打开」或被阻止。
>
> **解决方法（任选一种）：**
>
> **方法一**：清除隔离属性后打开
>
> ```bash
> # 解除 macOS 安全限制
> xattr -d com.apple.quarantine "/Applications/AI Profile Manager.app"
> # 然后双击打开即可
> ```
>
> 如果提示权限不足，加 `sudo`：
>
> ```bash
> sudo xattr -d com.apple.quarantine "/Applications/AI Profile Manager.app"
> ```
>
> **方法二**：右键打开
>
> 在 Finder 中找到 App，**右键 → 打开**（不要双击），弹出的对话框里点「打开」。

### 启动桌面应用

```bash
cd desktop
npm run tauri dev
```

### 主要功能

- **可视化 profile 管理** — 新建、编辑、删除、复制、导入/导出
- **内置终端** — 点击「运行」直接在右侧终端启动 `ai claude <profile>`
- **批量操作** — Cmd/Ctrl+点击多选，批量删除/导出
- **会话历史** — 自动记录运行历史，一键恢复之前的会话
- **终端搜索** — Cmd/Ctrl+F 搜索终端输出
- **配色主题** — 6 套终端配色独立切换（Dracula / Solarized / Monokai 等）
- **首次引导** — 自动检测环境、扫描已有配置
- **配置备份** — 自动备份 + 手动恢复

### 快捷键

| 快捷键 | 功能 |
|--------|------|
| `⌘N` | 新建 Profile |
| `⌘B` | 切换侧边栏 |
| `` Ctrl+` `` / `⌘J` | 切换底部终端 |
| `⌘⇧M` | 最大化终端 |
| `⌘F` | 搜索终端输出 |
| `⌘K` | 快捷键帮助 |
| `Backspace` | 删除选中的 Profile |

桌面应用与 CLI 使用同一份配置文件和文件锁，并发安全。
