# Agent 管理功能设计

## 背景

kn 目前已实现 Skill 和 Plugin 的管理（扫描、启用/禁用、安装/卸载、批量操作）。
调研确认三个 CLI 都支持 Agent（子代理）概念，且 Claude Code 和 Qoder CLI 格式一致。

## 四者关系

```
Plugin (包) ── 可包含 ──→ Skill / Command / Agent
                              │
                              │  也可 standalone 独立存在
                              ▼

Command ── 快捷操作，用户显式触发 /cmd，无独立上下文
Agent   ── 子代理，独立上下文+权限，可被用户显式唤醒或被父 Agent 自动调度
Skill   ── 知识/能力注入，Agent 自动匹配 description 加载到当前上下文
```

### 触发关系

| 触发方式 | 谁 → 谁 | 说明 |
|----------|---------|------|
| 显式调用 | 用户 → Command | `/review-pr`，直接执行 |
| 显式调用 | 用户 → Agent | `@security-auditor`，独立上下文运行 |
| 自动匹配 | Agent → Skill | Agent 读 description，命中则加载 |
| 自动调度 | Agent → Agent | 父 Agent spawn 子 Agent |
| 包含关系 | Plugin → {Skill,Command,Agent} | 随 Plugin 启用/禁用 |

### 将 Command/Agent/Skill 统一视角来看

三种扩展本质上都是 `.md` 文件 + YAML frontmatter：

| | Command | Agent | Skill |
|---|--------|-------|-------|
| 文件 | `commands/*.md` | `agents/*.md` | `skills/*/SKILL.md` |
| frontmatter | description, arguments, model | name, description, tools, model, color | name, description, allowed-tools, model |
| 触发 | 用户显式 `/name` | 用户显式或 Agent 自动 | Agent 自动或用户显式 |
| 上下文 | 共享当前 | 独立隔离 | 注入当前 |
| 启用/禁用 | 无标准机制 | 无标准机制（可通过改名 `xxx.md.disabled` 实现） | `.md` ↔ `.md.disabled` 重命名 |

## 为什么加 Agent，暂不加 Command

- **Command**：数量少（通常 5-10 个），自解释，不需要集中管理面板
- **Agent**：容易积累到 10-30 个，元数据丰富（tools/model/color 等），需要集中查看和管理
- **格式统一**：Claude Code 和 Qoder CLI 的 Agent 都是 `.md` + YAML frontmatter，可复用同一套扫描逻辑

## 各 CLI Agent 支持详情

### Claude Code
- **路径**：`.claude/agents/*.md`（项目级），`~/.claude/agents/*.md`（用户级）
- **格式**：YAML frontmatter + Markdown body
- **关键 frontmatter**：name, description, tools, disallowedTools, model, color, permissionMode, memory, skills, context
- **内置 Agent**：Explore (Haiku, 只读), Plan, general-purpose, statusline-setup, claude-code-guide
- **启用/禁用**：无官方机制。可采用 `xxx.md.disabled` 重命名方式

### Codex CLI
- **路径**：`.codex/agents/*.toml`（项目级），`~/.codex/agents/*.toml`（用户级）
- **格式**：TOML
- **关键字段**：name, description, developer_instructions, model, model_reasoning_effort, sandbox_mode, mcp_servers
- **内置 Agent**：default, worker, explorer
- **启用/禁用**：已内置在 config.toml 中 `[agents.xxx]` 段

### Qoder CLI (CN)
- **路径**：`~/.qoder-cn/agents/*.md`（用户级），`.qoder/agents/*.md`（项目级）
- **格式**：YAML frontmatter + Markdown body（与 Claude Code 一致）
- **内置 Agent**：Explore, Plan, general-purpose, qoder-guide, statusline-setup（5 个）
- **注意**：Qoder CN 没有 Plugin 概念；内置 skill 编译在二进制中，外部不可扫描
- **Command 支持**：`~/.qoder-cn/commands/*.md`

## 扫描策略

```
scan_agents()
  ├── Claude: 扫描 ~/.claude/agents/ + .claude/agents/
  │     格式: .md + YAML frontmatter
  │     解析: name, description, tools, model, color
  │     内置: 5 个（Explore, Plan, general-purpose, statusline-setup, claude-code-guide）
  │
  ├── Codex: 扫描 ~/.codex/agents/ + .codex/agents/
  │     格式: .toml
  │     解析: name, description, model, sandbox_mode
  │     内置: 3 个（default, worker, explorer）
  │
  └── Qoder: 扫描 ~/.qoder-cn/agents/ + .qoder/agents/
        格式: .md + YAML frontmatter（同 Claude）
        解析: name, description, tools, model, color
        内置: 5 个（Explore, Plan, general-purpose, qoder-guide, statusline-setup）
```

## 内置 Agent 只读原则

**所有 `source = "builtin"` 的 Agent 必须只读展示**：

| 操作 | builtin | user | project | plugin |
|------|---------|------|---------|--------|
| 查看详情 | ✅ | ✅ | ✅ | ✅ |
| 启用/禁用 | ❌ 隐藏 | ✅ | ✅ | ✅ |
| 删除 | ❌ 隐藏 | ✅ | ✅ | ✅ |
| 编辑 | ❌ 隐藏 | ✅ | ✅ | ❌（随 plugin 管理） |

**各 CLI 内置 Agent 汇总**：

| CLI | 内置 Agent | 数量 |
|-----|-----------|------|
| Claude Code | Explore, Plan, general-purpose, statusline-setup, claude-code-guide | 5 |
| Codex CLI | default, worker, explorer | 3 |
| Qoder CLI | Explore, Plan, general-purpose, qoder-guide, statusline-setup | 5 |

**UI 表现**：内置 Agent 行加 "Built-in" 徽章，整行略微变灰示意不可操作，不显示切换开关和删除按钮。批量操作自动跳过 builtin 条目。

## 依赖关系分析

这是 Agent 管理的核心亮点功能——不只是列表+开关，而是**揭示扩展之间的结构关系**。

### 五类依赖关系

```
┌──────────────────────────────────────────────────────────────┐
│  Plugin (superpowers)                                        │
│  ├── Skill: brainstorming  ──被引用──→ Skill: writing-plans  │
│  ├── Skill: tdd            ──被引用──→ Skill: verification   │
│  ├── Agent: code-reviewer  ──spawn──→ Agent: security-auditor│
│  └── Agent: planner        ──match──→ Skill: brainstorming   │
│                                                              │
│  Standalone Agent                                            │
│  ├── my-reviewer           ──needs──→ Tools: [Read,Grep,Bash]│
│  └── my-deployer           ──needs──→ MCP: [github, vercel]  │
└──────────────────────────────────────────────────────────────┘
```

| # | 依赖类型 | 方向 | 检测方式 | 示例 |
|---|---------|------|---------|------|
| 1 | **Plugin → Skill/Agent** | 包含 | 解析 Plugin manifest/目录结构 | superpowers 包含 brainstorming skill |
| 2 | **Agent → Skill** | 引用 | 解析 Agent frontmatter 的 `skills` 字段 | planner agent 声明 skills: [brainstorming] |
| 3 | **Agent → Agent** | 调度 | 解析 body 中的 `subagent_type` 或 `@name` 引用 | general-purpose spawn code-reviewer |
| 4 | **Agent/Skill → Tools** | 能力 | 解析 frontmatter 的 `tools` / `allowed-tools` | code-reviewer needs [Read, Grep, Bash] |
| 5 | **Agent/Skill → Model** | 模型 | 解析 frontmatter 的 `model` 字段 | planner requires claude-opus-4-8 |

### 五项分析能力

#### ① 影响分析 — "禁用这个会影响谁？"

```
禁用 brainstorming
  ├── ⚠️ writing-plans 的流程依赖它（plan 前要先 brainstorm）
  ├── ⚠️ tdd 内部引用了 brainstorming 的输出格式
  └── ⚠️ Agent "planner" 的 skills 列表中包含它
```

**算法**：反向遍历依赖图，从目标节点 BFS/DFS 找出所有反向依赖方。

#### ② 冲突检测 — "两个 Plugin 是否有冲突？"

```
Plugin A 和 Plugin B 同时提供名为 "code-review" 的 Skill
  → 🔴 命名冲突：后加载的覆盖先加载的

Agent "security-auditor" 同时存在于用户级和项目级
  → 🟡 层级覆盖：项目级覆盖用户级，用户级覆盖内置
```

**算法**：按 name+type 分组，检测同组内 source 不同的多个条目。

#### ③ 孤岛检测 — "哪些 Agent/Skill 不属于任何 Plugin？"

```
Standalone Agents (3):
  ├── my-custom-reviewer    (用户级, ~/.claude/agents/)
  ├── team-linter           (项目级, .claude/agents/)
  └── deploy-checker        (用户级, ~/.claude/agents/)
```

**算法**：遍历所有条目，筛选 `source != "plugin"` 且不在任何 Plugin 声明范围内的。

#### ④ 能力缺口 — "Agent 需要的 Tool 是否可用？"

```
Agent "my-deployer" 需要 [Bash, Write, github-mcp]
  ├── ✅ Bash      — 已在 default.json 中授权
  ├── ✅ Write     — 已在 default.json 中授权
  └── ⚠️ github-mcp — 未配置 MCP 服务器连接
```

**算法**：解析 Agent 的 tools 列表，与当前 CLI 配置中的 allowed-tools 和 MCP server 列表做差集运算。

**数据来源**：
- Claude Code: `.claude/settings.json` 的 `permissions.allow` + `.claude/mcp.json`
- Codex: `config.toml` 的 `[tools]` + `[mcp_servers]`
- Qoder: `.qoder-cn/settings.json` 的对应字段

#### ⑤ 调用链可视化 — 完整有向图

```
用户输入 "review this PR"
  → Agent: general-purpose          [入口]
    ├── spawn → Agent: code-reviewer    [tools: Read, Grep]
    ├── spawn → Agent: security-auditor [tools: Read, Grep, Bash]
    ├── match → Skill: code-review      [inject knowledge]
    └── 汇总子代理结果 → 返回用户
```

**算法**：从 root（用户输入或入口 Agent）出发，沿 spawn/match/depends 边做 DFS，构建完整调用树。

#### ⑤ 实现：CallChainPanel 组件

**位置**：`desktop/src/components/CallChainPanel.tsx`，内嵌于 `AgentDetail.tsx` 详情面板中。

**计算方式**：**前端 BFS**（无需额外 Rust IPC）。`graphData` 已在 React state 中，`buildCallChain()` 直接从 `DependencyGraphData` 计算上下游。

```
buildCallChain(targetId, graph):
  1. 在 graph.nodes 中定位 target node
  2. 上游 BFS（反向边）：从 target 出发，匹配 edge.to == current
     收集 edge.from → ancestors[]，标记 depth + edgeKind + edgeLabel
  3. 下游 BFS（正向边）：从 target 出发，匹配 edge.from == current
     收集 edge.to → descendants[]，标记 depth + edgeKind + edgeLabel
  4. maxDepth = 5，超出截断
  5. visited set 防止循环
```

**数据结构**：

```typescript
interface CallChainNode {
  id: string;           // "claude:agent:security-auditor"
  label: string;
  kind: string;         // "plugin" | "agent" | "skill" | "tool" | "mcp"
  cli: string;
  source: string;
  locked: boolean;
  depth: number;        // 距离 target 的步数
  edgeKind: string;     // 与父节点的边类型
  edgeLabel: string;    // 边 human-readable 标签
}

interface CallChain {
  target: CallChainNode;       // 选中节点（depth=0）
  ancestors: CallChainNode[];   // 上游（谁依赖我）
  descendants: CallChainNode[]; // 下游（我依赖谁）
}
```

**UI 布局**：

```
┌─ Call Chain ───────────────────────────────────┐
│  ▲ Upstream (2)                    [展开/收起]  │
│  ├── ←refs ◆ brainstorming   🔒 Claude         │
│  └── ←spawns ● general-purpose  🔒 Claude      │
│                                                  │
│  ● security-auditor   builtin   [current]        │
│                                                  │
│  ▼ Downstream (4)                  [展开/收起]  │
│  ├── → needs  ■ Read                            │
│  ├── → needs  ■ Bash                            │
│  ├── → needs  ■ Grep                            │
│  └── → model  haiku                             │
│                                                  │
│  ← upstream  → downstream  |  refs spawns ...   │
└──────────────────────────────────────────────────┘
```

**视觉效果**：
- 节点按 depth 缩进（每层 16px）
- 边类型显示为 `←refs` / `→needs` 标签，颜色 muted
- 节点图标按 kind 区分：⬡(plugin) / ●(agent) / ◆(skill) / ■(tool)
- 节点颜色按 CLI 区分：Claude=#D97706, Codex=#7C3AED, Qoder=#059669
- builtin/locked 节点灰色 + 🔒 图标
- target 节点高亮（accent 背景色 + 左侧 accent 色条 + "current" 标签）
- 上游/下游各自可折叠（点击 section header）

**与 `analyze_impact` 的区别**：

| | analyze_impact | CallChainPanel |
|---|---|---|
| 方向 | 仅反向（谁依赖我） | 反向 + 正向双向 |
| 触发 | 禁用/删除前自动运行 | AgentDetail 挂载时自动计算 |
| 返回值 | `Vec<String>` 只有 ID | 结构化节点（带 label/kind/cli/depth/edge） |
| 用途 | 操作前快速确认 | 深度理解 Agent 依赖结构 |
| 深度 | BFS 到底 | maxDepth=5 截断 |
| 位置 | Rust `agent_manager.rs` | 前端 `CallChainPanel.tsx` |

### 依赖图数据结构

```rust
struct DependencyGraph {
    nodes: HashMap<String, DepNode>,   // id → node
    edges: Vec<DepEdge>,
}

struct DepNode {
    id: String,           // "claude:agent:security-auditor"
    kind: DepNodeKind,    // Plugin | Agent | Skill | Tool | MCP
    label: String,
    cli: String,
    source: String,       // builtin | user | project | plugin
}

struct DepEdge {
    from: String,         // source node id
    to: String,           // target node id
    kind: DepEdgeKind,    // Contains | References | Spawns | NeedsTool | NeedsModel
    label: String,        // human-readable description
}

enum DepNodeKind { Plugin, Agent, Skill, Tool, McpServer }
enum DepEdgeKind { Contains, References, Spawns, NeedsTool, NeedsModel }
```

### UI 呈现

两种视图切换，默认力导向图：

**图视图（主视图）**：
- 力导向布局（D3-force 或 cytoscape.js）
- 节点形状区分类型：Plugin=六边形, Agent=圆形, Skill=菱形, Tool=方形
- 节点颜色区分 CLI：Claude=#D97706, Codex=#7C3AED, Qoder=#059669
- 边颜色/样式区分依赖类型：实线=包含, 虚线=引用, 点线=spawn, 箭头=需要
- `source=builtin` 节点加锁图标 + 灰色边框
- hover 节点高亮所有相邻边，显示 tooltip 摘要
- 点击节点打开详情侧边栏

**表视图（辅助）**：
- 行 = 消费者（谁依赖别人），列 = 提供者（被谁依赖）
- 交叉单元格显示依赖类型图标
- 支持按 CLI 过滤、按类型过滤
- 适合快速扫描"谁在用 X"

**影响分析面板（侧边栏抽屉）**：
- 选中一个条目 → 点击 "Analyze Impact"
- 展示反向依赖链（谁会被影响）
- 展示正向依赖链（它依赖谁）
- 遇到 builtin 节点特别标注（不可修改）

### 分析触发时机

| 时机 | 分析类型 | 说明 |
|------|---------|------|
| 打开依赖图视图 | 全量图构建 | 基于当前扫描结果构建完整图 |
| 禁用/删除前确认 | 影响分析 | 弹窗展示受影响的其他条目 |
| 安装 Plugin 后 | 冲突检测 | 自动检测命名冲突并提示 |
| Plugin 安装前（预检） | 能力缺口 | 检测依赖的 Tool/MCP 是否满足 |
| 手动点击 "Analyze" | 调用链分析 | 选中节点的上下游追踪 |

### 实现优先级

| 阶段 | 内容 | 优先级 |
|------|------|--------|
| 1 | 全量依赖图构建（5 类边） + 力导向图渲染 | **亮点功能，高** |
| 2 | 影响分析（反向依赖） + 禁用/删除前确认弹窗 | **亮点功能，高** |
| 3 | 冲突检测 + 安装 Plugin 后自动提示 | 中 |
| 4 | 能力缺口检测 + 安装前预检 | 中 |
| 5 | 表视图 + 过滤 | 低 |
| 6 | 调用链分析面板 | 低 |

## UI 设计

复用 Skill & Plugin 面板的布局模式：

- 在 SkillManager 同级的 ActivityBar 新增 "Agents" 标签，或
- 在 SkillManager 列表中新增 "Agents" 分组（与 Plugins / Skills / System 并列）

推荐后者——四者统一在一个面板中管理，分组区分：

```
Skills & Plugins
├── Plugins (3)
│   ├── superpowers      Claude  ✅
│   └── ...
├── Skills (5)
│   ├── my-skill         Claude  ✅
│   └── ...
├── Agents (8)
│   ├── security-auditor Claude  [tools:Read,Grep]  🔵
│   ├── code-reviewer    Codex   [sandbox:read-only] 🟢
│   ├── my-agent         Qoder   [tools:Read,Grep]  🔵
│   └── ...
└── System (2)
    └── ...
```

### ListRow 展示
- 图标：Agent 用 `Bot` / `Cpu` 图标
- 标签：CLI badge（同现有 Skill 列表）
- 元数据行：model 名称 + tools 数量 + color 色点

### 详情面板
- name + description（hero 区）
- model / tools / color / sandbox_mode 等元数据
- 所属 CLI + 路径
- 启用/禁用按钮

### 批量操作
- 启用/禁用选中 Agent（Claude/Qoder 通过重命名 xxx.md.disabled，Codex 通过 config.toml）
- 删除（卸载）选中 Agent

## 数据模型

### Agent 条目

```rust
struct AgentEntry {
    id: String,           // "claude:agent:security-auditor"
    cli: String,          // "claude" | "codex" | "qoder"
    name: String,
    description: String,
    enabled: bool,
    source: AgentSource,  // builtin | user | project | plugin
    model: Option<String>,
    tools: Vec<String>,
    color: Option<String>,
    path: String,         // 文件路径
    // Agent → Skill 引用
    skills: Vec<String>,  // frontmatter skills 字段
    // Codex 特有
    sandbox_mode: Option<String>,
}

enum AgentSource { Builtin, User, Project, Plugin }
```

### 依赖图

```rust
struct DependencyGraph {
    nodes: HashMap<String, DepNode>,
    edges: Vec<DepEdge>,
}

struct DepNode {
    id: String,           // "claude:agent:security-auditor"
    kind: DepNodeKind,    // Plugin | Agent | Skill | Tool | McpServer
    label: String,
    cli: String,
    source: AgentSource,
    locked: bool,         // source == Builtin → true，只读标记
}

struct DepEdge {
    from: String,
    to: String,
    kind: DepEdgeKind,
    label: String,
}

enum DepNodeKind { Plugin, Agent, Skill, Tool, McpServer }
enum DepEdgeKind { Contains, References, Spawns, NeedsTool, NeedsModel }
```

## 实施计划

### 第一阶段：基础扫描 + 列表展示（Agent 管理基础）

| # | 内容 | 说明 |
|---|------|------|
| 1.1 | Rust 端 `scan_agents()` — 扫描 Claude/Qoder 的 `.md` Agent | 复用现有 Skill frontmatter 解析，加 `source` 标记 |
| 1.2 | Tauri command `get_agents` | 返回 `Vec<AgentEntry>`，前端可获取 |
| 1.3 | SkillManager 面板新增 "Agents" 分组 | 与 Plugins / Skills / System 并列 |
| 1.4 | Agent 列表行组件 — name, CLI badge, model, tools, color 点 | builtin 行灰色 + "Built-in" 徽章，无开关 |
| 1.5 | Agent 详情面板 — 展示所有 frontmatter 字段 + 路径 | builtin 隐藏编辑/删除按钮 |

### 第二阶段：依赖分析（亮点功能）

| # | 内容 | 说明 |
|---|------|------|
| 2.1 | Rust 端 `build_dependency_graph()` | 构建完整 DepNode + DepEdge，5 类边全覆盖 |
| 2.2 | Tauri command `get_dependency_graph` | 返回序列化后的图数据给前端 |
| 2.3 | 力导向图渲染（D3-force / cytoscape.js） | 节点形状区分类型，颜色区分 CLI，边样式区分依赖类型 |
| 2.4 | 点击节点 → 详情侧边栏 | 展示该节点的上下游关系 |
| 2.5 | 影响分析：禁用/删除前弹窗 | "禁用 XX 会影响以下 3 项..." 确认对话框 |
| 2.6 | 孤岛检测面板 | 列出不属于任何 Plugin 的 standalone Agent/Skill |

### 第三阶段：冲突 & 缺口检测

| # | 内容 | 说明 |
|---|------|------|
| 3.1 | 命名冲突检测 | 安装 Plugin 后自动扫描同组冲突 |
| 3.2 | 能力缺口检测 | Agent tools 差集 → 缺失的 Tool/MCP 列表 |
| 3.3 | 安装前预检弹窗 | Plugin 安装前展示依赖是否满足 |

### 第四阶段：扩展能力

| # | 内容 | 说明 |
|---|------|------|
| 4.1 | Codex `.toml` Agent 扫描 | 需要 TOML 解析器 |
| 4.2 | 启用/禁用 Agent（`.md.disabled` 重命名） | Claude/Qoder |
| 4.3 | 表视图 + 过滤 | 行=消费者，列=提供者 |
| 4.4 | 批量操作（启用/禁用/删除） | 自动跳过 builtin |
| 4.5 | ✅ 调用链分析面板 | `CallChainPanel.tsx`：前端 BFS 双向追踪，上游+下游树形展示，depth 缩进，edge 标签，target 高亮，可折叠 |
| 4.6 | 新建/编辑 Agent | 表单编辑器 |
