# Project Workspace — Design Spec

> 2026-06-10 | Status: draft | Branch: feat/project-workspace

## Overview

将「项目」提升为一等公民，新增独立的项目工作区视图。用户可以注册项目目录、设置项目级默认 profile、浏览文件树、查看和管理 CLI 原生会话历史（Claude Code / Codex / Qoder）、在项目上下文中运行 profile 并恢复历史会话。

## Motivation

当前 `ProjectSelector` 只是一个轻量 dropdown，仅在 skills/hooks 面板用作过滤器。用户缺少：
- 项目为中心的视图（项目列表 + 详情）
- 项目级默认 profile 管理
- 文件树浏览 + 预览
- 真正的 CLI 原生会话历史（含标题、可恢复）

## Architecture

```
ActivityBar: [profile] [skills] [hooks] [projects]  ← 第 4 个入口
         │ 切换到 projects
         ▼
┌─ Sidebar (300px) ──────────────────────────────────┐
│  ProjectSelector (下拉 + 注册新项目)                 │
│  ─────────────────────────────────────────────────  │
│  项目列表 (可搜索、右键菜单)                         │
│    📁 my-project    默认: work-openai               │
│    📁 another-app   默认: openai                     │
└────────────────────────────────────────────────────┘
         │ 选中项目
         ▼
┌─ MainPanel ─────────────────────────────────────────┐
│  Header: 项目名 + 路径                               │
│  [运行] → 选择 profile → 右侧终端                     │
│  [在 Finder 中打开] [在终端中打开]                    │
│  ─────────────────────────────────────────────────── │
│  Tabs: [文件] [会话]                                  │
│                                                      │
│  [文件] → FileTree(左) + FileContentBlock(右)         │
│  [会话] → SessionList(点击恢复/新开会话)              │
└──────────────────────────────────────────────────────┘
```

**Profile 列表保持在原有位置，与项目列表是两个独立并行区域。** 项目列表和注册在所有地方（skills/hooks/projects）完全同步，共用 `project_manager.rs` 作为唯一数据源。

## Data Model

### ProjectInfo 扩展 (projects.json)

```json
[
  {
    "name": "my-project",
    "path": "/Users/xxx/code/my-project",
    "defaultProfile": "work-openai"
  }
]
```

存储于 `~/.claude-profiles/projects.json`。

### .ai-profile 文件 (项目根目录)

```yaml
default_profile: work-openai
```

单行 YAML，仅记录该项目默认使用的 profile 名称引用。实际 env vars 仍在 `~/.claude-profiles/config.yaml`。

**同步规则**: 设置项目默认 profile 时，同时写入 `projects.json` 和 `.ai-profile`。读取时以 `projects.json` 为准（`.ai-profile` 作为辅助渠道，方便 shell wrapper 和外部工具直接读取）。两者不一致时 `projects.json` 覆盖 `.ai-profile`。

### 两级默认体系

| 优先级 | 来源 | 说明 |
|--------|------|------|
| 1 (高) | `.ai-profile` → `default_profile` | 项目级默认，运行时优先使用 |
| 2 (低) | `config.yaml` → `default` | 全局默认，项目无设置时降级使用 |
| 3 (兜底) | 弹出选择器 | 项目默认失效（profile 被删）或全局默认也不存在时 |

- 项目默认指向的 profile 被删除 → 自动降级到全局默认 + Toast 警告"项目默认 profile 已失效"
- 两个都没有 → 运行按钮弹出 profile 选择器
- **与现有全局默认的关系**: 全局默认 (`config.yaml` 的 `default:`) 保持不变，项目默认只是覆盖层。无项目上下文时（如直接在终端运行 `ai`），行为完全不受影响。

### SessionRecord (扩展)

```typescript
interface SessionInfo {
  sessionId: string;              // CLI 原生 session UUID
  title: string;                  // 首条提示词 / thread_name
  cli: "claude" | "codex" | "qoder";
  profile: string | null;         // 可能未知（外部会话）
  projectPath: string;
  workDir: string;
  timestamp: number;              // updated_at as unix ms
  status: "active" | "ended";     // 从 CLI 存储推断
}
```

## Feature Details

### 1. ActivityBar 新增入口

- 第 4 个 icon: 项目 (文件夹图标 `Folder`)
- 点击切换到 projects 视图
- Sidebar 变为项目列表，MainPanel 变为项目详情
- 与 profile/skills/hooks 同级

### 2. Sidebar — 项目列表

- **ProjectSelector** (顶部): 复用现有组件，支持选择"全部项目"或特定项目
- **项目列表**: 每行显示项目名 + 默认 profile tag (`📁 my-project  默认: work-openai`)
- **右键菜单**: 
  - 删除项目 (仅从 registry 移除，不删目录)
  - 修改名称
  - 修改路径
- **注册新项目**: 点击 → 文件对话框选择目录 → 自动取目录名作为项目名，校验唯一性
- 数据与 skills/hooks 面板的项目列表完全同步

### 3. MainPanel — 项目详情

#### Header

- 项目名 + 完整路径（monospace, 可点击复制）
- **[运行] 按钮** (split-button):
  - **主按钮** (点左侧): 使用项目默认 profile 直接运行。默认不存在 → 降级全局默认 → 都没有则弹出选择器
  - **下拉箭头** (点右侧): 弹出 profile 选择器，用户可以主动选择非默认 profile 运行
  - 运行命令: `ai <cli-type> <profile>`
  - 工作目录 = 项目路径
  - 右侧终端新 tab
- **[在 Finder 中打开]** / **[在终端中打开]** 快捷按钮

#### 文件 Tab

- **FileTree** (左侧 ~200px): 复用已有 `FileTree.tsx` 组件
- **FileContentBlock** (右侧): 复用已有 `FileContentBlock.tsx` 组件
- 点击文件 → 异步加载预览内容（只读，语法高亮）

#### 会话 Tab

- **SessionList**: 从 CLI 原生存储扫描的真实会话
- 每项显示:
  - 状态指示器（🟢 active / ⚪ ended）
  - 标题（首条 prompt，过长截断至 80 字符）
  - CLI 类型图标 + profile 名
  - 相对时间（"刚刚" / "5分钟前" / "今天 14:30" / "昨天" / "3天前"）
- **点击会话 → 恢复**:
  - Claude: `ai claude <profile> --resume <session-id>`
  - Codex: `ai codex <profile> resume <session-id>`
  - Qoder: `ai qoder <profile> -r <session-id>`
  - 右侧终端新 tab 执行，workDir = 项目路径

### 4. 会话发现（混合模式 C）

扫描时机：项目被选中时触发，结果缓存 30s。

```
后端扫描流程 (按 CLI 分别处理):

  1. Claude Code — 手动扫描文件系统
     - 扫描 ~/.claude/projects/<encoded-path>/ 目录
     - 读 PID.json → 获取 sessionId, status, updatedAt
     - 读 {uuid}.jsonl → 提取首条 user prompt 作为标题
     - 路径编码: / 替换为 - (如 /Users/xxx/p → -Users-xxx-p)

  2. Codex — 解析 session_index.jsonl
     - 读 ~/.codex/session_index.jsonl (JSONL 格式,每行一个 session)
     - thread_name 直接用做标题
     - 过滤 workDir 匹配当前项目路径的 session

  3. Qoder — 优先用 CLI 命令 (推荐)
     - 调用 qoderclicn --list-sessions (在项目目录下执行)
     - 输出自带: 序号、标题(首条prompt)、相对时间、UUID
     - 解析输出即可,无需手动读 jsonl
     - 降级方案: 如 CLI 不可用,扫描 ~/.qoder-cn/projects/<path>/ (同 Claude 格式)
     - 额外支持: qoderclicn --delete-session <index> 删除会话

  4. 合并去重 + 按时间倒序
  5. 状态修正: PID.json 中 status "busy" 但 updated_at > 24h → 强制 "ended"
```

**三种 CLI 会话能力对比:**

| 能力 | Claude Code | Codex | Qoder (qoderclicn) |
|------|------------|-------|---------------------|
| 列出会话 | ❌ 无 CLI 命令 | `resume` picker | ✅ `--list-sessions` |
| 恢复会话 | `-r [id]` / `-c` | `resume [id]` / `--last` | `-r [id]` / `-c` |
| 删除会话 | ❌ | ❌ | ✅ `--delete-session <index>` |
| 存储格式 | `{uuid}.jsonl` | `session_index.jsonl` | `{uuid}.jsonl` (同 Claude) |
| 标题来源 | jsonl 首条 prompt | `thread_name` 字段 | `--list-sessions` 直接输出 |

**CLI 存储路径映射:**

| CLI | 用户级 | 项目级 Session 路径 | 扫描方式 |
|------|--------|---------------------|---------|
| Claude Code | `~/.claude/` | `~/.claude/projects/<encoded-path>/` | 文件系统扫描 |
| Codex | `~/.codex/` | `~/.codex/session_index.jsonl` | 解析 JSONL 索引 |
| Qoder | `~/.qoder-cn/` | `~/.qoder-cn/projects/<encoded-path>/` | **CLI 命令优先** → 降级文件扫描 |

**恢复命令构建:**

| CLI | 恢复指定会话 | 恢复最近会话 |
|------|------------|------------|
| Claude | `ai claude <profile> --resume <id>` | `ai claude <profile> -c` |
| Codex | `ai codex <profile> resume <id>` | `ai codex <profile> resume --last` |
| Qoder | `ai qoder <profile> -r <id>` | `ai qoder <profile> -c` |

### 5. Rust 后端新增命令

| 命令 | 输入 | 输出 | 说明 |
|------|------|------|------|
| `scan_project_sessions` | `project_path: string, cli: string` | `SessionInfo[]` | 扫描指定 CLI 在项目下的会话 |
| `write_ai_profile` | `project_path: string, default_profile: string` | `Ok` | 写 `.ai-profile` 文件 |
| `read_ai_profile` | `project_path: string` | `string \| null` | 读 `.ai-profile` 获取 default_profile |
| `update_project` | `name: string, path: string, default_profile: string \| null` | `Ok` | 更新项目配置（名称/路径/默认profile） |

### 6. 项目右键菜单

| 操作 | 行为 |
|------|------|
| 删除项目 | 从 `projects.json` 移除，不删除实际目录 |
| 修改名称 | 弹出 NameDialog，校验唯一性 |
| 修改路径 | 弹出目录选择器 |

### 7. 运行流程

```
选中项目 → 点击 [运行]
         → 读取 .ai-profile → 获取 default_profile
         → default_profile 存在? 对应 profile 在 config.yaml 中存在?
            ✅ → 获取 profile 的 cli_type → 构建 ai <cli> <profile>
            ❌ → 尝试全局默认 (config.yaml default)
                 ✅ → 构建命令
                 ❌ → 弹出 profile 选择器 (列表来自 config.yaml)
         → 右侧终端: 新 tab, workDir = 项目路径
         → 记录 SessionRecord
```

## Files to Touch

### Frontend (TypeScript/React)

| 文件 | 改动 |
|------|------|
| `App.tsx` | 集成 projects activity，管理 ProjectDetail 状态 |
| `ActivityBar.tsx` | 新增第 4 个入口 "project" |
| `components/ProjectSidebar.tsx` | **新增**: 项目列表 Sidebar，含右键菜单 |
| `components/ProjectDetail.tsx` | **新增**: 项目详情 MainPanel，含 Header + Tabs |
| `components/SessionList.tsx` | **新增**: 会话历史列表，状态指示 + 恢复操作 |
| `components/ProjectSelector.tsx` | 扩展: 右键菜单（删除/改名/改路径） |
| `hooks/useProjects.ts` | 扩展: updateProject, setDefaultProfile |
| `hooks/useSessionScanner.ts` | **新增**: 调用 `scan_project_sessions` 的 hook |
| `lib/types.ts` | 新增 SessionInfo; ProjectInfo 扩展 defaultProfile |

### Backend (Rust)

| 文件 | 改动 |
|------|------|
| `project_manager.rs` | 扩展: update_project, defaultProfile 管理, .ai-profile 读写 |
| `commands.rs` | 新增: scan_project_sessions (Claude/Codex/Qoder 扫描) |
| `lib.rs` | 注册新 commands |

### Shell Wrapper

| 文件 | 改动 |
|------|------|
| `shell/ai-profile.sh` | 支持 `.ai-profile` 文件读取作为默认 profile fallback |

## Edge Cases & Error Handling

1. **项目目录被删除/移动**: 扫描路径时 `std::fs::metadata` 检查，失效则在列表显示 ⚠ 图标，hover 提示"目录不存在"
2. **.ai-profile 与 projects.json 不一致**: 以 `projects.json` 为写入源；`.ai-profile` 读取值与 projects.json 不同步时以 projects.json 为准
3. **会话状态误报**: `status: "busy"` 但 `updated_at > 24h` → 强制标记 `ended`
4. **Qoder 未安装/不可用**: scan 时 catch 错误，返回空 `[]`，不阻塞其他 CLI 扫描
5. **大量会话**: SessionList 使用虚拟滚动（如有 > 100 条），扫描结果缓存 30s
6. **路径编码多平台**: Claude 的 project path 编码在 Windows 上可能不同（`\` vs `/`），需要统一处理

## Out of Scope (本期不做)

- 会话内容完整浏览器（只取标题，不展示完整对话）
- 项目内 Agent/Skill/Hook 的直接管理（已在 skills/hooks 面板支持）
- 多窗口/多项目并行
- `.ai-profile` 的 git-aware 操作
