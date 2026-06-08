# File Tree Detail View — Design Spec

**Date**: 2026-06-07
**Status**: Approved

## Problem

The current detail pages for skill/plugin/command/agent show only flat file content in `<pre>` blocks. A skill or plugin can be an entire directory with subdirectories and multiple files, but users have no way to browse them. They can only see the single "core" file that gets auto-loaded (e.g. `SKILL.md`, the agent system prompt file).

## Goal

Add a VS Code–style file tree + content browser to every resource detail page (skill, plugin, command, agent). Users enter a detail page → left side shows the directory tree with the core file selected by default → right side shows the core file's rendered content → clicking any other file loads and displays its content.

## Non-goals

- Hook detail pages are NOT in scope
- No editing of files (read-only browsing)
- No file operations (create, rename, delete)
- No multi-tab—single active file at a time

## Architecture

```
┌──────────────────────────────────────────────────────┐
│  Detail Page (SkillDetail / AgentDetail)              │
│  ┌─────────────┬────────────────────────────────────┐ │
│  │ FileTree    │  Content Panel                       │ │
│  │ (250px)     │  ┌────────────────────────────────┐ │ │
│  │             │  │ Hero (name, badges, status)    │ │ │
│  │  📁 dir/    │  ├────────────────────────────────┤ │ │
│  │    📄 a.md  │  │ Metadata (CLI, version, path)  │ │ │
│  │    📄 b.sh  │  ├────────────────────────────────┤ │ │
│  │  📁 sub/    │  │ Action Buttons                 │ │ │
│  │    📄 c.py  │  ├────────────────────────────────┤ │ │
│  │             │  │ File Content (markdown/raw)    │ │ │
│  │             │  │                                │ │ │
│  │             │  │                                │ │ │
│  └─────────────┴────────────────────────────────────┘ │
└──────────────────────────────────────────────────────┘
```

## Backend: `list_directory_tree` Command

**File**: `desktop/src-tauri/src/commands.rs`

New Tauri command:

```rust
#[derive(Serialize)]
struct FileTreeNode {
    name: String,
    path: String,
    is_dir: bool,
    children: Vec<FileTreeNode>,
}

#[tauri::command]
fn list_directory_tree(path: String) -> Result<FileTreeNode, String>
```

**Behavior**:
- Accepts a file or directory path; if file, scans its parent directory
- Recursively scans all subdirectories
- Skips: `.git`, `node_modules`, `__pycache__`, `.DS_Store`, hidden files (`.xxx`)
- Sorts: directories first, then alphabetical by name (case-insensitive)
- Safety: reuses existing `is_safe_path` (only HOME and TEMP allowed)
- Registration: added to `invoke_handler` in `lib.rs`

## Frontend: `FileTree.tsx` Component

**File**: `desktop/src/components/FileTree.tsx` (new)

**Props**:
```typescript
interface FileTreeProps {
  rootPath: string;               // base directory to scan
  onSelect: (node: FileTreeNode) => void;  // file click callback
  activePath?: string;            // currently selected file path
  defaultOpenFile?: string;       // file to auto-select on mount (e.g. "SKILL.md")
}
```

**States**:
- `loading` — spinner while fetching tree
- `error` — error message if directory can't be read
- `empty` — "empty directory" message
- `ready` — tree displayed

**Tree rendering**:
- Recursive: `FileTree` → maps children → `FileTreeNode` component
- Collapse/expand: click arrow or folder name toggles children visibility
- Indentation: 16px per level via `paddingLeft`
- Icons: folder-open/folder-closed for dirs; file-type icons for files
  - `.md` → markdown icon
  - `.toml`/`.yaml`/`.json` → config/gear icon
  - `.sh`/`.py`/`.js`/`.ts` → code/terminal icon
  - default → document icon
- Active file: highlighted row with accent background
- Default behavior: root + first level expanded; `defaultOpenFile` selected
- Scrollable with `overflow-y: auto`

**Design**: frontend-design aesthetic — match existing dark theme, use CSS variables (`var(--app-bg)`, `var(--accent)`, etc.), subtle hover states, smooth expand/collapse transitions.

### PluginDetail View Toggle

Plugin 详情页特殊处理：插件本质上是一个"包"，包含多个子资源（skills/agents/commands），现有列表模式（tabs 列出子项）是更直观的浏览方式。因此 PluginDetail 提供两种视图模式，用户通过顶部的 Segmented Control 切换：

- **列表模式**（默认）：现有 tabs 形式 — Skills / Agents / Commands 三个标签页，每页列出对应子项，点击进入子项详情
- **文件模式**：FileTree 左面板 + 文件内容右面板 — 用于直接浏览插件目录下的源文件

切换状态由组件内部的 `useState` 管理，不影响其他类型详情页。

## Frontend: Detail Component Layout Changes

### SkillDetail.tsx Changes

Each internal detail function gets the FileTree layout:

| Internal Component | rootPath source | defaultOpenFile |
|---|---|---|
| `PluginDetail` | Plugin install directory (need path from backend) | `README.md` or first `.md` — **has view toggle** |
| `PluginSkillDetail` | `skill.path` parent dir | `skill.path` basename |
| `StandaloneDetail` | `item.data.path` | `item.data.path` basename |
| `CommandDetail` | `item.data.path` parent dir | `item.data.path` basename |

**Content panel behavior**:
- Default: shows existing content (hero + metadata + core file's parsed content)
- When user clicks a different file in tree: calls `invoke("read_file", { path })`, replaces the content area with raw file content (or markdown-rendered if `.md`)
- A small breadcrumb/path indicator above content shows which file is being viewed

### AgentDetail.tsx Changes

- Add FileTree left panel: `rootPath = parent directory of agent.path`
- `defaultOpenFile = basename of agent.path`
- Same content switching behavior on tree click
- Preserve existing agent-specific sections (tools, skills, reverse refs, impact analysis)

## Data Flow

```
User clicks skill in SkillManager
  → App sets selectedSkillItem
  → SkillDetail renders the appropriate internal component
  → Internal component mounts FileTree with rootPath
  → FileTree calls invoke("list_directory_tree", { path })
  → Rust scans directory recursively, returns FileTreeNode
  → FileTree renders tree, auto-selects defaultOpenFile
  → Content panel loads default file via read_skill_content/read_agent_content/read_file
  → User clicks another file in tree
  → FileTree calls onSelect(node)
  → Internal component calls invoke("read_file", { path: node.path })
  → Content panel updates to show new file content
```

## Verification

1. `cargo check` in `desktop/src-tauri/` — Rust compiles
2. `npx tsc --noEmit` in `desktop/` — TypeScript type-checks
3. `npm run tauri dev` — launch app
4. Navigate to Skills → click a plugin → see left file tree + right content
5. Click a standalone skill → see file tree with core file selected
6. Click an agent → see file tree with agent file selected
7. Click different files in tree → content updates correctly
8. Test edge cases: empty directory, symlink skill, single-file skill
