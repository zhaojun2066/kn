import type { DocPage, DocGroup } from '../types/docs'

export const docGroups: DocGroup[] = [
  { id: 'getting-started', icon: '🚀', label: '入门指南', pages: ['introduction', 'installation', 'quickstart'] },
  { id: 'cli-reference', icon: '📋', label: 'CLI 参考', pages: ['config-management', 'command-reference', 'shell-wrapper', 'config-structure'] },
  { id: 'desktop', icon: '🖥️', label: 'Desktop 应用', pages: ['desktop-install', 'desktop-features'] },
  { id: 'scenarios', icon: '📖', label: '场景示例', pages: ['scenario-multi-account', 'scenario-project-keys', 'scenario-openai-proxy'] },
  { id: 'more', icon: '💡', label: '更多', pages: ['faq', 'troubleshooting', 'uninstall'] },
]

export const docPages: Record<string, DocPage> = {
  introduction: {
    id: 'introduction', group: 'getting-started', groupIcon: '🚀', title: '简介', next: 'installation',
    content: `AI Profile Manager 是一个**多 profile 管理系统**，让你在不同终端会话中为 \`claude\` / \`codex\` 使用不同的 API key、base URL 和模型配置。

:::tip
**核心理念：** 每个终端会话独立注入环境变量，退出后自动清除，不影响其他窗口。
:::

## 为什么要用？

- 你同时使用 **DeepSeek** 和**官方 Anthropic** 两个 API，不想频繁改配置
- 不同项目需要用**不同的 API Key**
- 想用**第三方 OpenAI 兼容中转**跑 Codex
- 手动改 \`settings.json\` 太繁琐，容易出错

## 核心组成

| 组件 | 说明 |
|------|------|
| \`profile\` CLI | 命令行管理工具，增删改查 profile |
| Shell Wrapper | 拦截 \`claude\` / \`codex\` 命令，自动注入环境变量 |
| Desktop GUI | 可视化界面，管理与终端一体化 |
| \`config.yaml\` | 所有 profile 数据存储在一个 YAML 文件中 |`,
  },

  installation: {
    id: 'installation', group: 'getting-started', groupIcon: '🚀', title: '安装', prev: 'introduction', next: 'quickstart',
    content: `## 1. 执行安装脚本

\`\`\`bash
cd ~/workspace/me/shark/kn
bash install.sh
\`\`\`

安装脚本会自动：
- 复制文件到 \`~/.claude-profiles/\`
- 将 \`~/.claude-profiles/bin\` 加入 PATH
- 在 \`~/.zshrc\` 中自动激活 Shell Wrapper

## 2. 激活

\`\`\`bash
source ~/.zshrc
\`\`\`

> install.sh 已自动在 \`~/.zshrc\` 中添加了 PATH 和 source 配置。

## 3. 导入现有配置（可选）

如果你已经有 \`~/.claude/settings.json\`：

\`\`\`bash
profile init
\`\`\`

## 4. 确认安装成功

\`\`\`bash
profile list
\`\`\`

应该能看到至少一个 profile。

## 文件结构

\`\`\`
~/.claude-profiles/
├── bin/profile          ← 管理 CLI
├── lib/config.py        ← 共享模块
├── shell-rc             ← Shell Wrapper
├── config.yaml          ← profile 数据
└── .config.lock         ← 文件锁
\`\`\``,
  },

  quickstart: {
    id: 'quickstart', group: 'getting-started', groupIcon: '🚀', title: '快速开始', prev: 'installation', next: 'config-management',
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

# 原生命令不受影响
claude                   # 无 profile 注入，照常使用
\`\`\`

## 工作流程

1. \`profile add <name> -i\` — 交互式创建 profile
2. \`ai claude <name>\` — 使用该 profile 启动
3. 退出后环境变量自动清除

## 设置默认 Profile

\`\`\`bash
profile default deepseek   # 设置默认
ai claude                  # 直接使用默认，无需选择
\`\`\``,
  },

  'config-management': {
    id: 'config-management', group: 'cli-reference', groupIcon: '📋', title: '配置管理', prev: 'quickstart', next: 'command-reference',
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

  'command-reference': {
    id: 'command-reference', group: 'cli-reference', groupIcon: '📋', title: '命令参考', prev: 'config-management', next: 'shell-wrapper',
    content: `## profile CLI 命令

| 命令 | 说明 |
|------|------|
| \`profile list\` | 列出所有 profile |
| \`profile show <name>\` | 查看 profile 详情 |
| \`profile env <name>\` | 输出 env 变量（shell eval 格式） |
| \`profile names\` | 输出 profile 名列表（供 fzf 使用） |
| \`profile add <name> [desc]\` | 新增 profile，\`-i\` 为交互式 |
| \`profile remove <name>\` | 删除 profile |
| \`profile set <name> <K>=<V>\` | 设置环境变量 |
| \`profile unset <name> <K>\` | 删除环境变量 |
| \`profile default [name]\` | 查看/设置默认 profile |
| \`profile init\` | 从 \`~/.claude/settings.json\` 导入 |

## Shell Wrapper 命令

| 用法 | 说明 |
|------|------|
| \`ai claude <profile>\` | 用指定 profile 启动 Claude Code |
| \`ai codex <profile>\` | 用指定 profile 启动 Codex |
| \`ai claude\` | 交互式选择 profile 后启动 |
| \`ai profile list\` | 列出所有 profile（标注默认） |
| \`ai profile env <name>\` | 查看 profile 环境变量 |
| \`ai profile switch <name>\` | 切换默认 profile |
| \`ai\` / \`ai --help\` | 显示帮助信息 |`,
  },

  'shell-wrapper': {
    id: 'shell-wrapper', group: 'cli-reference', groupIcon: '📋', title: 'Shell Wrapper', prev: 'command-reference', next: 'config-structure',
    content: `Shell Wrapper 是一组 Shell 函数，安装时自动添加到 \`~/.zshrc\`。

## 工作原理

1. 定义 \`ai\` 函数，拦截 \`claude\` / \`codex\` 子命令
2. 读取对应 profile 的 \`config.yaml\` 中的 \`env\` 字段
3. 以 \`export VAR=VAL\` 形式注入环境变量
4. 启动真实的 \`claude\` / \`codex\` 进程
5. 进程退出后，环境变量自动清除（Session 级隔离）

## 特性

- **多终端互不影响** — 每个终端窗口独立注入，互不干扰
- **原生命令不受影响** — 直接调用 \`claude\` / \`codex\` 不经过 wrapper
- **支持交互式选择** — 无参数调用时弹出 fzf 选择器
- **默认 profile** — 设了默认后无需每次选择`,
  },

  'config-structure': {
    id: 'config-structure', group: 'cli-reference', groupIcon: '📋', title: '配置结构', prev: 'shell-wrapper', next: 'desktop-install',
    content: `配置文件位于 \`~/.claude-profiles/config.yaml\`。

## 格式示例

\`\`\`yaml
default: deepseek

profiles:
  deepseek:
    desc: "DeepSeek 中转"
    env:
      ANTHROPIC_AUTH_TOKEN: sk-xxxxxxxx
      ANTHROPIC_BASE_URL: https://api.deepseek.com/anthropic
      ANTHROPIC_MODEL: deepseek-v4-pro
      ANTHROPIC_DEFAULT_HAIKU_MODEL: deepseek-v4-flash
      DISABLE_AUTOUPDATER: "1"

  codex-default:
    desc: "OpenAI 官方 Codex"
    env:
      OPENAI_API_KEY: sk-proj-xxxxxxxx
\`\`\`

## env 字段说明

| 变量 | 用途 | 适用工具 |
|------|------|----------|
| \`ANTHROPIC_AUTH_TOKEN\` | API key | Claude Code |
| \`ANTHROPIC_BASE_URL\` | 自定义 API 端点 | Claude Code |
| \`ANTHROPIC_MODEL\` | 默认模型 | Claude Code |
| \`ANTHROPIC_DEFAULT_HAIKU_MODEL\` | Haiku 模型 | Claude Code |
| \`ANTHROPIC_DEFAULT_SONNET_MODEL\` | Sonnet 模型 | Claude Code |
| \`ANTHROPIC_DEFAULT_OPUS_MODEL\` | Opus 模型 | Claude Code |
| \`OPENAI_API_KEY\` | API key | Codex |
| \`OPENAI_BASE_URL\` | 自定义 API 端点 | Codex |
| \`OPENAI_MODEL\` | 默认模型 | Codex |
| \`DISABLE_AUTOUPDATER\` | 禁用自动更新 | Claude Code |

\`env\` 是自由 key-value，可以添加任何环境变量。`,
  },

  'desktop-install': {
    id: 'desktop-install', group: 'desktop', groupIcon: '🖥️', title: '安装与启动', prev: 'config-structure', next: 'desktop-features',
    content: `从 [GitHub Releases](https://github.com/zhaojun2066/ai-profile-manager/releases/latest) 下载 \`.dmg\` 安装包。

## macOS 首次打开问题

由于应用未经过 Apple 开发者签名，首次打开会提示"已损坏"。

### 解决方法一：清除隔离属性

\`\`\`bash
xattr -d com.apple.quarantine "/Applications/AI Profile Manager.app"
\`\`\`

权限不足时加 \`sudo\`：

\`\`\`bash
sudo xattr -d com.apple.quarantine "/Applications/AI Profile Manager.app"
\`\`\`

### 解决方法二：右键打开

在 Finder 中找到 App，**右键 → 打开**，弹出的对话框里点「打开」。

## 开发模式启动

\`\`\`bash
cd desktop
npm run tauri dev
\`\`\`

Desktop 应用与 CLI **共享同一份** \`~/.claude-profiles/config.yaml\`，数据实时同步。`,
  },

  'desktop-features': {
    id: 'desktop-features', group: 'desktop', groupIcon: '🖥️', title: '功能与快捷键', prev: 'desktop-install', next: 'scenario-multi-account',
    content: `## 主要功能

- **可视化 profile 管理** — 新建、编辑、删除、复制、导入/导出
- **内置终端** — 点击「运行」直接在右侧终端启动 \`ai claude <profile>\`
- **批量操作** — Cmd/Ctrl+点击多选，批量删除/导出
- **会话历史** — 自动记录运行历史，一键恢复之前的会话
- **终端搜索** — Cmd/Ctrl+F 搜索终端输出
- **配色主题** — 6 套终端配色独立切换（Dracula / Solarized / Monokai 等）
- **首次引导** — 自动检测环境、扫描已有配置

## 快捷键

| 快捷键 | 功能 |
|--------|------|
| \`⌘N\` | 新建 Profile |
| \`⌘B\` | 切换侧边栏 |
| \`Ctrl+\`\` / \`⌘J\` | 切换底部终端 |
| \`⌘⇧M\` | 最大化终端 |
| \`⌘F\` | 搜索终端输出 |
| \`⌘K\` | 快捷键帮助 |
| \`Backspace\` | 删除选中的 Profile |`,
  },

  'scenario-multi-account': {
    id: 'scenario-multi-account', group: 'scenarios', groupIcon: '📖', title: '多账号切换', prev: 'desktop-features', next: 'scenario-project-keys',
    content: `## 场景：DeepSeek + 官方 Anthropic 双开

\`\`\`bash
# 终端 A：使用 DeepSeek
ai claude deepseek

# 终端 B：使用官方 API
ai claude anthropic-official
\`\`\`

两个终端互不影响，各自的 key 各自用。

## 准备工作

确保两个 profile 都已创建：

\`\`\`bash
profile add deepseek -i
profile add anthropic-official -i
\`\`\`

## 技巧

- 设为默认避免每次选择：\`profile default deepseek\`
- 不同项目目录用不同默认（通过 direnv 或其他工具）`,
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

## 高级：用 direnv 自动切换

在项目根目录创建 \`.envrc\`：

\`\`\`bash
export ANTHROPIC_AUTH_TOKEN=$(profile env client-a 2>/dev/null | grep AUTH_TOKEN | cut -d= -f2 | tr -d "'")
\`\`\``,
  },

  'scenario-openai-proxy': {
    id: 'scenario-openai-proxy', group: 'scenarios', groupIcon: '📖', title: 'OpenAI 兼容中转', prev: 'scenario-project-keys', next: 'faq',
    content: `## 场景：用第三方 OpenAI 兼容 API 跑 Codex

\`\`\`bash
profile add codex-third "第三方 OpenAI 兼容中转"
profile set codex-third OPENAI_API_KEY=sk-xxx
profile set codex-third OPENAI_BASE_URL=https://api.third-party.com/v1
profile set codex-third OPENAI_MODEL=gpt-5

ai codex codex-third
\`\`\`

## 常见第三方兼容 API

任何兼容 OpenAI API 格式的服务都可以：

- 各种 AI Gateway / 中转服务
- 自部署的 vLLM / Ollama 等兼容端点
- 其他云厂商的 OpenAI 兼容接口

## 注意事项

- 确保 \`OPENAI_BASE_URL\` 路径正确（通常以 \`/v1\` 结尾）
- 模型名要和第三方服务支持的名称一致`,
  },

  faq: {
    id: 'faq', group: 'more', groupIcon: '💡', title: 'FAQ', prev: 'scenario-openai-proxy', next: 'troubleshooting',
    content: `## 多终端冲突

**Q: 多个终端同时改 profile 会冲突吗？**

不会。写操作通过文件锁（\`fcntl.flock\`）保护，同时写入会排队等待。

## API Key 安全

**Q: API key 安全吗？**

key 明文存储在 \`~/.claude-profiles/config.yaml\`。建议确保目录权限为 700：

\`\`\`bash
chmod 700 ~/.claude-profiles
\`\`\`

## Project 级别

**Q: 怎么让 profile 对整个项目目录生效？**

使用 direnv，在项目根目录创建 \`.envrc\` 导出环境变量。

## 修改描述

**Q: 如何修改 profile 的描述？**

编辑 \`~/.claude-profiles/config.yaml\`，直接改 \`desc:\` 字段即可。

## 重新导入

**Q: profile init 导入后还能重新导入吗？**

不会覆盖已有 profile。手动删除后再导入：

\`\`\`bash
profile remove deepseek
profile init
\`\`\``,
  },

  troubleshooting: {
    id: 'troubleshooting', group: 'more', groupIcon: '💡', title: '故障排查', prev: 'faq', next: 'uninstall',
    content: `## 安装问题

### \`profile: command not found\`

确保 PATH 包含 \`~/.claude-profiles/bin\`：

\`\`\`bash
echo $PATH | grep claude-profiles
\`\`\`

如果没有，手动添加：

\`\`\`bash
export PATH="$HOME/.claude-profiles/bin:$PATH"
\`\`\`

### Shell Wrapper 未激活

检查 \`~/.zshrc\` 中是否有 wrapper source 行：

\`\`\`bash
grep "claude-profiles" ~/.zshrc
\`\`\`

## 权限问题

### 文件锁冲突

如果遇到 \`PermissionError\` 相关的文件锁问题：

\`\`\`bash
rm ~/.claude-profiles/.config.lock
\`\`\`

### 目录权限不足

\`\`\`bash
chmod 700 ~/.claude-profiles
\`\`\`

## Desktop App 问题

### "已损坏，无法打开"

参考 [安装与启动](/docs/desktop-install) 中的 macOS 签名解决方案。

### 闪退

尝试命令行启动查看错误日志：

\`\`\`bash
/Applications/AI\ Profile\ Manager.app/Contents/MacOS/ai-profile-manager
\`\`\``,
  },

  uninstall: {
    id: 'uninstall', group: 'more', groupIcon: '💡', title: '卸载与重置', prev: 'troubleshooting',
    content: `## 完全卸载

\`\`\`bash
rm -rf ~/.claude-profiles
\`\`\`

## 清理 Shell 配置

编辑 \`~/.zshrc\`，删除以下标记之间的内容：

\`\`\`
# >>> AI Profile Manager >>>
...
# <<< AI Profile Manager <<<
\`\`\`

## 重置为初始状态

\`\`\`bash
# 备份现有配置
cp ~/.claude-profiles/config.yaml ~/.claude-profiles/config.yaml.bak

# 重新初始化
rm ~/.claude-profiles/config.yaml
profile init
\`\`\``,
  },
}
