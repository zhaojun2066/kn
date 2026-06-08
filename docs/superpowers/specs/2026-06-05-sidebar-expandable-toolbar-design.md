# Sidebar 可水平展开式工具栏 — 设计规格

## 概述

将顶部 Toolbar 中的 profile 操作按钮（新增、默认、复制、导入）和设置菜单中的配置管理操作（刷新、备份、恢复）统一移到左侧 Sidebar 内部，采用可水平展开的工具栏模式。

## 交互设计

### 两种状态

**收起（默认）**：`[+ 新增] [★] [⋯]`

**展开（点击 ⋯）**：`[+ 新增] [★] [⋯] | [⧉] [↓▾] [📤] [🗑] | [↻] [💾] [↺]`

### 动画

- 隐藏按钮用 `max-width: 0` → `max-width: 40px` + `opacity: 0` → `opacity: 1`
- `transition: all 200ms ease`
- 分隔线同理

### 收起触发

- 点击 `⋯` toggle
- 点击 Sidebar 外部（mousedown listener）
- 选中 profile 后自动收起

### 按钮状态

| 按钮 | 始终可用 | 需选中 | 需选中 + 非默认 |
|------|:---:|:---:|:---:|
| 新增 | ✓ | | |
| 默认 | | | ✓ |
| 复制 | | ✓ | |
| 导入 ▾ | ✓ | | |
| 导出 | | ✓ | |
| 删除 | | ✓ | |
| 刷新 | ✓ | | |
| 备份 | ✓ | | |
| 恢复 | ✓ | | |

删除按钮红色，点击后弹出 ConfirmDialog。

## 组件结构

### 新组件：ExpandableToolbar

```tsx
interface ExpandableToolbarProps {
  selectedName: string | null;
  isDefault: boolean;
  hasSelection: boolean;
  backupExists: boolean;
  onAdd: () => void;
  onSetDefault: (name: string) => void;
  onCopyProfile: () => void;
  onInit: () => void;        // 扫描系统配置
  onImport: () => void;       // 从文件导入
  onExport: () => void;
  onDelete: (name: string) => void;
  onRefresh: () => void;
  onBackup: () => void;
  onRestore: () => void;
}
```

内部状态：
- `expanded: boolean` — 展开/收起

### 修改：Sidebar

新增 props 传递上述回调，在搜索栏和列表头之间插入 `<ExpandableToolbar />`。

### 修改：Toolbar

移除左侧 profile 操作区和设置菜单中的配置管理项。

### 修改：App.tsx

将 profile 操作回调从 Toolbar props 移到 Sidebar props。

## 视觉参考

- VS Code Source Control 面板头部的操作图标栏
- 蓝色 primary 按钮表示主操作
- 图标按钮 hover 时浅色背景
- ⋯ 按钮展开时高亮（与 active 状态一致）
- 删除按钮红色，与其他操作形成视觉分隔
