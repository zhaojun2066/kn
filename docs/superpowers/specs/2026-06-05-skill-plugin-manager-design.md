# Skill & Plugin 统一管理面板 — 设计规格

**日期:** 2026-06-05
**状态:** 设计中
**目标:** 扫描 Claude Code / Codex / Qoder 全局 skill 和 plugin，统一管理启用/禁用

---

## 1. 动机

目前 Claude Code 和 Codex 各自维护独立的 skill/plugin 体系：
- 用户不知道自己装了多少 skill/plugin
- 不知道哪些在哪个 CLI 下
- 禁用某个 plugin/skill 需要手动编辑 JSON/TOML 配置文件
- 没有统一视图

kn 作为 AI CLI 的 profile 管理器，天然适合做这件事。

---

## 2. 统一数据模型

### 2.1 核心抽象

```
CLI Tool
├── Plugin（启用/禁用最小单位）
│   ├── id: "claude:plugin:superpowers@superpowers-marketplace"
│   ├── cli: "claude" | "codex" | "qoder"
│   ├── type: "plugin"
│   ├── name: "superpowers"
│   ├── marketplace: "superpowers-marketplace"
│   ├── enabled: boolean
│   ├── version: string?
│   ├── source: "marketplace" | "bundled" | "user"
│   ├── install_path: string
│   └── skills: Skill[]          ← 只读子项，不可单独开关
│
├── Standalone Skill（可单独启用/禁用）
│   ├── id: "claude:skill:remotion-best-practices"
│   ├── cli: "claude" | "codex" | "qoder"
│   ├── type: "skill"
│   ├── name: "remotion-best-practices"
│   ├── enabled: boolean
│   ├── link_type: "symlink" | "file" | "directory"
│   ├── path: string
│   └── source: "user" | "unknown"
│
└── System Skill（内置，不可操作）
    ├── id: "codex:system:imagegen"
    ├── cli: "codex"
    ├── type: "system"
    ├── name: "imagegen"
    ├── enabled: true（永真）
    └── path: string
```

### 2.2 关键设计决策

1. **Plugin 是启用/禁用的最小单位** — Plugin 下的 skill 不能单独切换，跟着 Plugin 走
2. **Standalone Skill 可以独立切换** — 不归属任何 Plugin 的 skill，有自己的 enable/disable
3. **禁用 = 不改内容、不删文件** — 统一用非破坏性方式实现（重命名、改配置值）
4. **System Skill 不可操作** — 只读展示

---

## 3. 扫描策略

### 3.1 Claude Code

#### Plugin 扫描

**数据源 1: `~/.claude/plugins/installed_plugins.json`**

```json
{
  "version": 2,
  "plugins": {
    "superpowers@superpowers-marketplace": [
      { "scope": "user", "installPath": "/path/to/cache", "version": "5.1.0" }
    ],
    "ecc@ecc": [...]
  }
}
```

**数据源 2: `~/.claude/settings.json` → `enabledPlugins`**

```json
{
  "enabledPlugins": {
    "superpowers@superpowers-marketplace": true
  }
}
```

**扫描逻辑：**
```
installed_plugins.json 中的所有 plugin → 基础列表
settings.json 的 enabledPlugins → 判断状态：
  - 在 enabledPlugins 中且值为 true → enabled
  - 不在 enabledPlugins 中 或 值为 false → disabled

每个 plugin 的 installPath + "skills/" → 枚举子 skill
  - Claude skill 格式: <name>.md（单个 markdown 文件）
```

#### Standalone Skill 扫描

**数据源: `~/.claude/skills/` 目录**

```
for each entry in ~/.claude/skills/:
    if entry is symlink:
        resolve → 判断是否指向某个 plugin 的 skills/ 目录
        if 是 → 跳过（它是 Plugin 的子 skill，已在 Plugin 扫描中覆盖）
        if 否 → 标记为 Standalone Skill
    if entry is real file/directory:
        if name is ".DS_Store" or starts with "." → 跳过
        else → 标记为 Standalone Skill
```

**启用判断：**
```
<name>.md           → enabled
<name>.md.disabled  → disabled
```

### 3.2 Codex

#### Plugin 扫描

Codex plugin 有三个来源：

**数据源 1: `~/.codex/plugins/` — 用户自行安装的 plugin**

```
for each directory in ~/.codex/plugins/:
    if 包含 .codex-plugin/plugin.json → 解析 metadata（name, version, description）
    判断启用: 查 config.toml [plugins] 中是否有此 plugin
        - 有且 enabled = true → enabled
        - 有且 enabled = false → disabled
        - 不在 config.toml 中 → 默认 disabled
```

**数据源 2: `~/.codex/config.toml` → `[plugins]` section**

```toml
[plugins]
[plugins.'browser-use@openai-bundled']
enabled = true
[plugins.'documents@openai-primary-runtime']
enabled = true
```

**数据源 3: Marketplace 目录**
- `~/.codex/.tmp/bundled-marketplaces/` — 本地 bundled marketplace
- `~/.cache/codex-runtimes/codex-primary-runtime/plugins/` — 运行时 marketplace

**扫描逻辑：**
```
config.toml [plugins] 中列出的 → Plugin 列表
每个 plugin 对应的 marketplace 目录中的 .codex-plugin/plugin.json → metadata

enabled 判断：
  - config.toml 中 enabled = true → enabled
  - enabled = false 或 条目不存在 → disabled

每个 plugin 目录下的 skills/ 子目录 → 枚举子 skill
  - Codex skill 格式: <name>/SKILL.md（文件夹 + SKILL.md）
```

#### Standalone Skill 扫描

**数据源: `~/.codex/skills/` 顶层目录（排除 `.system/`）**

```
for each entry in ~/.codex/skills/ (excluding .system/):
    if entry 是目录 且 包含 SKILL.md:
        if SKILL.md → enabled
        if SKILL.md.disabled → disabled
        标记为 Standalone Skill
    if entry 是 symlink:
        同上判断
```

#### System Skill 扫描

**数据源: `~/.codex/skills/.system/`**

```
for each directory in .system/:
    if 包含 SKILL.md → System Skill（只读，不可操作）
    if 包含 .codex-system-skills.marker → 跳过标记文件
```

### 3.3 Qoder

**已实现。** Qoder 的 Skill 格式与 Codex 完全相同：目录包含 `SKILL.md`。

#### Skill 扫描

**数据源：** `~/.qoder-cn/skills/` — 国内版（qoderclicn）

**格式：** 每个子目录包含 `SKILL.md`（与 Codex 格式一致）

```
~/.qoder-cn/skills/
├── api-doc-generator/
│   └── SKILL.md          ← YAML frontmatter + Markdown 正文
└── my-other-skill/
    └── SKILL.md.disabled ← 已禁用
```

**扫描逻辑：**
```
for each directory in skills/:
    if SKILL.md 存在 → enabled
    if SKILL.md.disabled 存在 → disabled
    标记为 Standalone Skill (qoder:skill:<name>)
```

**重点：Qoder 没有 Plugin 概念。** 其扩展体系是 Skills + MCP Servers + Commands + Hooks 四者独立，没有统一的 Plugin 容器。所有 Qoder skill 都映射为 Standalone Skill。

#### 启用/禁用策略

与 Codex 完全一致：`SKILL.md` ↔ `SKILL.md.disabled` 文件重命名。

---

## 4. 启用/禁用写回策略

| 资产类型 | CLI | 启用操作 | 禁用操作 | 原子性 |
|---------|-----|---------|---------|--------|
| Plugin | Claude | 在 `settings.json` 的 `enabledPlugins` 中添加 `"name": true` | 从 `enabledPlugins` 中移除条目 | JSON 整体读取→修改→写回 |
| Plugin | Codex | 在 `config.toml` 中设 `enabled = true` | 设 `enabled = false` | TOML 行级编辑（保留注释和格式） |
| Standalone Skill | Claude | `xxx.md.disabled` → `xxx.md` | `xxx.md` → `xxx.md.disabled` | 文件重命名 |
| Standalone Skill | Codex | `SKILL.md.disabled` → `SKILL.md` | `SKILL.md` → `SKILL.md.disabled` | 文件重命名 |
| Standalone Skill | Qoder | `SKILL.md.disabled` → `SKILL.md` | `SKILL.md` → `SKILL.md.disabled` | 文件重命名 |

### 4.1 TOML 安全编辑

Codex 的 `config.toml` 是用户手动编辑的文件，包含注释和个人配置。修改时必须：
1. 用行级字符串替换（不经过 TOML 解析器），保持注释和格式不变
2. 只改 `enabled = true/false` 这一行
3. 写入前备份 `config.toml.bak`

### 4.2 JSON 安全编辑

Claude 的 `settings.json` 需要：
1. 完整读取 → 修改 → 格式化写回（保持缩进）
2. 写入前备份 `settings.json.bak`

---

## 5. 前端 UI 设计

### 5.1 布局方案：VS Code Activity Bar

采用 VS Code 式的左侧活动栏（Activity Bar）+ 侧边栏内容区切换：

```
┌──┬──────────────────────────────────────────────┐
│🐱│ Toolbar          [搜索] [主题] [终端] [设置] │
│  ├──────────────────────────────────────────────┤
│  │                                              │
│  │          主内容区（MainPanel）                │
│  │     ┌─ Profile 列表 ────────────────────┐   │
│  │     │  🔍 搜索 profile...               │   │
│  │     │                                   │   │
│  │     │  🧑 deepseek          Claude    → │   │
│  │     │  🧑 codex-default     Codex     → │   │
│  │     │  ...                              │   │
│  │     └────────────────────────────────────┘   │
│  │                                              │
│  │        或（切换后）                           │
│  │     ┌─ Skill & Plugin Manager ──────────┐   │
│  │     │  筛选 [Claude ▼] [全部 ▼] 🔍     │   │
│  │     │                                   │   │
│  │     │  🔌 superpowers  ✅  [禁用]       │   │
│  │     │  🔌 browser-use  ✅  [禁用]       │   │
│  │     │  ...                              │   │
│  │     └────────────────────────────────────┘   │
│  │                                              │
│  ├──────────────────────────────────────────────┤
│  │ StatusBar  ● Claude ✅  ● Codex ✅           │
│  └──┴───────────────────────────────────────────┘
    ▲
    Activity Bar（左侧竖排图标栏）
    ├─ 👤 Profile
    ├─ 📦 Skills
    └─ ⚙️ (未来可扩展)
```

**核心交互：**
- 左侧 Activity Bar 是固定窄竖条（类似 VS Code 左侧图标栏）
- 点击 Profile 图标 → 侧边栏显示 profile 列表（现有行为不变）
- 点击 Skills 图标 → 侧边栏切换为 Skill & Plugin Manager
- 状态：当前激活的图标高亮，便于用户知道自己在哪
- Profile 图标始终在顶部，Skills 图标紧随其后

### 5.2 Skill & Plugin Manager 面板内容

```
┌─ Activity Bar ─┬──────────────────────────────────────────────┐
│                │  📦 Plugin & Skill Manager                    │
│                │                                              │
│                │  筛选: [全部 CLI ▼] [全部类型 ▼]  🔍 [...]  │
│                │                                              │
│                │  ── Plugins ─────────────────────── 3 个 ──  │
│                │                                              │
│                │  ┌─ ✅ superpowers  @superpowers-market  ─┐  │
│                │  │   Claude · v5.1.0          [禁用]      │  │
│                │  │   ├─ 📄 brainstorming                  │  │
│                │  │   ├─ 📄 systematic-debugging           │  │
│                │  │   └─ ... 共 14 个 skill    [展开 ▼]    │  │
│                │  └─────────────────────────────────────────┘  │
│                │                                              │
│                │  ┌─ ✅ browser-use  @openai-bundled      ─┐  │
│                │  │   Codex · v0.1.0           [禁用]      │  │
│                │  │   └─ 📄 browser                        │  │
│                │  └─────────────────────────────────────────┘  │
│                │                                              │
│                │  ┌─ ❌ ecc  @ecc                        ─┐  │
│                │  │   Claude · v2.0.0          [启用]      │  │
│                │  │   └─ ... 共 60+ 个 skill   [展开 ▼]    │  │
│                │  └─────────────────────────────────────────┘  │
│                │                                              │
│                │  ── Standalone Skills ──────────── 2 个 ──  │
│                │                                              │
│                │  ┌─ ✅ remotion-best-practices          ─┐  │
│                │  │   Claude · symlink           [禁用]    │  │
│                │  └─────────────────────────────────────────┘  │
│                │                                              │
│                │  ── System Skills（只读）─────────── 5 个 ── │
│                │                                              │
│                │  ┌─ 🔒 imagegen                  内置  ─┐  │
│                │  │   Codex · .system                      │  │
│                │  └─────────────────────────────────────────┘  │
│                │  ... 其余 4 个 ...                           │
└────────────────┴──────────────────────────────────────────────┘
```

### 5.3 交互细节

| 操作 | 行为 |
|------|------|
| 点击 Plugin 的 [启用/禁用] | 确认对话框 → 修改配置 → 刷新列表 |
| 点击 Standalone Skill 的 [启用/禁用] | 确认对话框 → 文件重命名 → 刷新列表 |
| 展开 Plugin | 显示子 skill 列表（只读） |
| 搜索 | 实时过滤 Plugin 名、Skill 名、marketplace 名 |
| CLI 筛选 | Claude / Codex / 全部 |
| 类型筛选 | Plugin / Standalone Skill / 全部 |
| 点击 Skill 路径 | 在文件管理器中打开 |
| Activity Bar 切换 | Profile ↔ Skills 内容区平滑切换，无闪烁 |

---

## 6. 实现架构

### 6.1 Rust 后端

```
desktop/src-tauri/src/
  └── skill_manager.rs     ← 新增模块

skill_manager.rs:
  ├── struct UnifiedPlugin { id, cli, name, marketplace, enabled, version, skills }
  ├── struct UnifiedSkill { id, cli, name, enabled, link_type, path }
  ├── struct ScanResult { plugins: Vec<UnifiedPlugin>, standalone_skills: Vec<UnifiedSkill>, system_skills: Vec<UnifiedSkill> }
  │
  ├── fn scan_all() -> ScanResult
  │   ├── scan_claude_plugins()
  │   ├── scan_claude_standalone_skills()
  │   ├── scan_codex_plugins()
  │   ├── scan_codex_standalone_skills()
  │   └── scan_codex_system_skills()
  │
  ├── fn set_plugin_enabled(cli, plugin_id, enabled) -> Result
  │   ├── claude: modify settings.json enabledPlugins
  │   └── codex: modify config.toml [plugins] enabled
  │
  └── fn set_skill_enabled(cli, skill_id, enabled) -> Result
      ├── claude: rename .md <-> .md.disabled
      └── codex: rename SKILL.md <-> SKILL.md.disabled
```

### 6.2 Tauri Commands

```rust
#[tauri::command]
fn scan_skills() -> ScanResult { ... }

#[tauri::command]
fn toggle_plugin(cli: String, plugin_id: String, enabled: bool) -> Result<(), String> { ... }

#[tauri::command]
fn toggle_standalone_skill(cli: String, skill_id: String, enabled: bool) -> Result<(), String> { ... }
```

### 6.3 前端

```
desktop/src/
  ├── components/
  │   └── SkillManager.tsx       ← 新增：主面板
  ├── hooks/
  │   └── useSkillManager.ts     ← 新增：数据获取 + 切换操作
  ├── lib/
  │   └── types.ts               ← 新增 ScanResult 等类型
  └── App.tsx                    ← 修改：添加入口
```

---

## 7. 边界情况

| 场景 | 处理 |
|------|------|
| CLI 未安装 | 该 CLI 的扫描返回空列表，不报错 |
| Plugin 的 skills 目录为空 | 显示「此 Plugin 不包含 skill」 |
| settings.json / config.toml 不存在 | 视为该 CLI 无 plugin |
| 同时安装了 Claude 和 Codex 的同名 plugin | 分开显示，按 CLI 区分 |
| Symlink 损坏（指向不存在） | 标记为「已损坏」+ ⚠️ 图标，不提供开关 |
| config.toml 格式意外（非标准 TOML） | 解析失败时提示用户手动检查，不做任何修改 |
| 用户在外部手动改了文件 | 每次打开面板时重新扫描 |
| `.system/` 目录缺失（Codex 旧版本） | 跳过 system skill 扫描，不报错 |

---

## 8. 未解决问题

- [ ] Qoder 的 skill/plugin 存储位置和格式待确认
- [ ] Claude `learned/` 目录的用途和是否需要管理
- [ ] 跨 CLI 同名 Skill 是否需要「关联」提示（如 remotion-best-practices 在 Claude 和 Codex 都有）
- [ ] 是否需要在 skill 层面做「最近使用」统计（依赖 transcript 分析，与 workflow mining 功能重叠）
- [ ] Plugin 启用/禁用后是否需要重启 PTY 会话才能生效

---

## 9. 与现有功能的关系

| 现有功能 | 关系 |
|---------|------|
| Token 用量追踪 | 独立，不重叠 |
| Profile 管理 | 独立，不重叠 |
| Workflow Mining（规划中） | 互补 — Workflow Mining 自动生成 skill，本功能管理 skill |
| 终端面板 | 独立，但 Skill 的启用/禁用可能影响终端中的 CLI 行为 |
