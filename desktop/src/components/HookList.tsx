import { useState, useMemo, useRef, useEffect, useCallback } from "react";
import {
  ChevronRight, Terminal, Lock, Plus, Search, Store,
  Circle, Filter, ChevronDown, X, Play, Ban, Trash2,
  Ellipsis, CheckSquare, Square, Copy, ArrowRight, ArrowLeft, Folder,
} from "lucide-react";
import { SearchInput } from "./common/SearchInput";
import { ConfirmDialog } from "./ConfirmDialog";
import { Dialog } from "./common/Dialog";
import { ContextMenu, type MenuItem } from "./ContextMenu";
import { CliBadge } from "./common/CliBadge";
import { FilterDropdown } from "./common/FilterDropdown";
import { CLI_LABELS, CLI_FILTER_OPTIONS } from "../lib/cli-constants";
import { ScopeTabs } from "./ScopeTabs";
import { ProjectSelector } from "./ProjectSelector";
import type { HookEntry } from "./HookDetail";
import type { ProjectInfo, ScopeTab } from "../lib/types";

/* ──────────────────── Types ──────────────────── */

export interface HookManagerData {
  hooks: HookEntry[];
}

/* ──────────────────── Props ─────────────────── */

interface HookListProps {
  data: HookManagerData | null;
  loading: boolean;
  selectedId: string | null;
  onSelect: (hook: HookEntry) => void;
  onAddHook: () => void;
  onOpenStore: () => void;
  onRefresh?: () => void;
  onToggleHook?: (hook: HookEntry, enabled: boolean) => void;
  onDeleteHook?: (hook: HookEntry) => void;
  // Copy / Move
  onMoveHook?: (hook: HookEntry, toScope: "user" | "project", targetProject?: ProjectInfo) => void;
  onCopyHook?: (hook: HookEntry, toScope: "user" | "project", targetProject?: ProjectInfo) => void;
  onBatchMoveHooks?: (hooks: HookEntry[], toScope: "user" | "project", targetProject?: ProjectInfo) => void;
  onBatchCopyHooks?: (hooks: HookEntry[], toScope: "user" | "project", targetProject?: ProjectInfo) => void;
  // Scope management
  activeScope: ScopeTab;
  onScopeChange: (scope: ScopeTab) => void;
  projects: ProjectInfo[];
  activeProject: ProjectInfo | null;
  onProjectChange: (project: ProjectInfo | null) => void;
  onAddProject: () => void;
  // When true, hide scope tabs & project selector — used when HookList is
  // embedded in a drawer that only deals with user-level hooks.
  hideScopeTabs?: boolean;
}

/* ──────────────────── Constants ──────────────────── */

const EVENT_TYPES = [
  "UserPromptSubmit",
  "PreToolUse",
  "PermissionRequest",
  "PostToolUse",
  "PostToolUseFailure",
  "PostToolBatch",
  "Stop",
  "StopFailure",
  "Notification",
  "SessionStart",
  "SessionEnd",
  "PreCompact",
  "PostCompact",
  "SubagentStart",
  "SubagentStop",
] as const;

const STATUS_OPTIONS: { value: string; label: string }[] = [
  { value: "all", label: "全部状态" },
  { value: "enabled", label: "已启用" },
  { value: "disabled", label: "已禁用" },
];

const PROJECT_HOOK_SOURCE_OPTIONS: { value: string; label: string }[] = [
  { value: "all", label: "全部来源" },
  { value: "project", label: "本项目" },
  { value: "inherited", label: "继承" },
];

const EVENT_LABELS: Record<string, string> = {
  UserPromptSubmit: "用户提交提示词",
  PreToolUse: "工具调用前",
  PermissionRequest: "权限请求",
  PostToolUse: "工具调用后",
  PostToolUseFailure: "工具调用失败",
  PostToolBatch: "批量工具完成",
  Stop: "会话回合结束",
  StopFailure: "回合异常结束",
  Notification: "系统通知",
  SessionStart: "会话开始",
  SessionEnd: "会话结束",
  PreCompact: "上下文压缩前",
  PostCompact: "上下文压缩后",
  SubagentStart: "子 Agent 启动",
  SubagentStop: "子 Agent 结束",
};

/* DropFilter and CliBadge are now imported from common/ */

/* ──────────────────── SectionHeader ──────────────────── */

function SectionHeader({
  label,
  count,
  collapsed,
  onToggle,
}: {
  label: string;
  count: number;
  collapsed: boolean;
  onToggle: () => void;
}) {
  const displayLabel = EVENT_LABELS[label] || label;
  const showEnglish = EVENT_LABELS[label] ? label : null; // only show English name when there's a Chinese label
  return (
    <div className="flex items-center gap-1 pl-2 pr-1 pt-2.5 pb-1">
      <button
        onClick={onToggle}
        className="flex items-center gap-1.5 flex-1 min-w-0 text-left
          hover:bg-[var(--app-hover)] transition-colors duration-fast"
      >
        <ChevronRight
          size={10}
          className={`shrink-0 text-[var(--app-text-muted)] transition-transform duration-200
            ${collapsed ? "" : "rotate-90"}`}
        />
        <span className="text-2xs text-[var(--app-text)] font-mono flex-1 truncate">
          {displayLabel}
          {showEnglish && (
            <span className="text-[var(--app-text-muted)] opacity-60 ml-1.5">
              {showEnglish}
            </span>
          )}
        </span>
        <span className="text-2xs text-[var(--app-text-muted)] font-mono tabular-nums shrink-0">
          {count}
        </span>
      </button>
    </div>
  );
}

/* ──────────────────── ListRow ──────────────────── */

function ListRow({
  hook,
  selected,
  checked,
  showCheck,
  noCheck,
  onClick,
  onContextMenu,
}: {
  hook: HookEntry;
  selected: boolean;
  checked: boolean;
  showCheck: boolean;
  noCheck?: boolean;
  onClick: (e: React.MouseEvent) => void;
  onContextMenu?: (e: React.MouseEvent) => void;
}) {
  const cmdShort = hook.command.length > 35
    ? hook.command.slice(0, 35) + "..."
    : hook.command;

  const primaryLabel = hook.name || hook.matcher || hook.eventType;
  const secondaryLabel = hook.name
    ? (hook.matcher || "")
    : cmdShort;

  const isSystem = hook.source === "system";

  return (
    <div
      onClick={onClick}
      onContextMenu={onContextMenu}
      className={`flex items-center gap-2 h-10 cursor-pointer select-none
        transition-all duration-100 ease-out group
        ${selected
          ? "bg-[var(--app-selected)] border-l-[3px] border-l-[var(--app-accent)] text-[var(--app-text)]"
          : checked
            ? "bg-[var(--app-hover)] border-l-[3px] border-l-[var(--app-amber)] text-[var(--app-text)]"
            : "border-l-[3px] border-l-transparent text-[var(--app-text-dim)] hover:bg-[var(--app-hover)] hover:text-[var(--app-text)]"
        }
        pl-3 pr-2`}
      style={selected ? { boxShadow: "inset 0 0 8px var(--app-glow)" } : undefined}
    >
      {/* Checkbox — hidden for system/readonly hooks */}
      {!noCheck && (
        <span
          data-checkbox
          className={`shrink-0 cursor-pointer transition-opacity duration-fast
            ${checked || showCheck ? "opacity-100" : "opacity-0 group-hover:opacity-100"}`}
        >
          {checked
            ? <CheckSquare size={12} className="text-[var(--app-amber)]" />
            : <Square size={12} className="text-[var(--app-text-muted)]" />
          }
        </span>
      )}

      {/* Status dot or lock icon */}
      {isSystem ? (
        <Lock size={10} className="text-[var(--app-text-muted)] shrink-0" />
      ) : (
        <Circle
          size={5}
          className={`shrink-0 ${
            hook.enabled
              ? "fill-[var(--app-accent)] text-[var(--app-accent)]"
              : "fill-[var(--app-text-muted)] text-[var(--app-text-muted)] opacity-50"
          }`}
          style={hook.enabled ? { boxShadow: "0 0 4px var(--app-glow)" } : undefined}
        />
      )}

      {/* Content */}
      <div className="flex-1 min-w-0">
        <div className={`text-xs font-mono truncate ${!hook.enabled ? "opacity-60 line-through" : ""}`}>
          {primaryLabel}
        </div>
        <div className="text-2xs text-[var(--app-text-muted)] font-mono truncate mt-0.5">
          {secondaryLabel}
        </div>
      </div>

      {/* Badges */}
      <div className="flex items-center gap-1 shrink-0">
        {(hook.projectName || hook.source === "project") && (
          <span
            className="text-2xs text-[var(--app-amber)] bg-[var(--app-amber)]/10 px-1 py-px rounded font-mono shrink-0"
            title="项目级"
          >
            项目
          </span>
        )}
        {hook.inherited && (
          <span
            className="text-2xs text-[var(--app-accent)] bg-[var(--app-accent)]/10 px-1 py-px rounded font-mono shrink-0"
            title="继承自用户级"
          >
            继承
          </span>
        )}
        <CliBadge cli={hook.cli} />
      </div>
    </div>
  );
}

/* ──────────────────── Main ──────────────────── */

export function HookList({
  data,
  loading,
  selectedId,
  onSelect,
  onAddHook,
  onOpenStore,
  onRefresh,
  onToggleHook,
  onDeleteHook,
  onMoveHook,
  onCopyHook,
  onBatchMoveHooks,
  onBatchCopyHooks,
  activeScope,
  onScopeChange,
  projects,
  activeProject,
  onProjectChange,
  onAddProject,
  hideScopeTabs,
}: HookListProps) {
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());

  // Per-tab filter state — each scope tab remembers its own filters independently,
  // matching the ResourceList pattern so switching scopes doesn't leak search state.
  type HookFilters = { search: string; cliFilter: string; statusFilter: string; sourceFilter: string };
  const [tabFilters, setTabFilters] = useState<Record<string, HookFilters>>({
    all: { search: "", cliFilter: "all", statusFilter: "all", sourceFilter: "all" },
    user: { search: "", cliFilter: "all", statusFilter: "all", sourceFilter: "all" },
    project: { search: "", cliFilter: "all", statusFilter: "all", sourceFilter: "all" },
  });
  const filters = tabFilters[activeScope] ?? tabFilters.all;
  const setSearch = (v: string) => setTabFilters((prev) => ({ ...prev, [activeScope]: { ...prev[activeScope], search: v } }));
  const setCliFilter = (v: string) => setTabFilters((prev) => ({ ...prev, [activeScope]: { ...prev[activeScope], cliFilter: v } }));
  const setStatusFilter = (v: string) => setTabFilters((prev) => ({ ...prev, [activeScope]: { ...prev[activeScope], statusFilter: v } }));
  const setSourceFilter = (v: string) => setTabFilters((prev) => ({ ...prev, [activeScope]: { ...prev[activeScope], sourceFilter: v } }));
  const search = filters.search;
  const cliFilter = filters.cliFilter;
  const statusFilter = filters.statusFilter;
  const sourceFilter = filters.sourceFilter;

  // Per-tab project selection — each tab remembers its own project independently.
  // This mirrors ResourceList's tabProjects pattern so switching scope tabs
  // doesn't leak project selection from one tab to another.
  const [tabProjects, setTabProjects] = useState<Record<string, ProjectInfo | null>>({
    all: null,
    project: null,
    user: null,
  });
  const effectiveActiveProject = tabProjects[activeScope] ?? null;
  const handleProjectChange = useCallback((p: ProjectInfo | null) => {
    setTabProjects((prev) => ({ ...prev, [activeScope]: p }));
  }, [activeScope]);

  // Sync current tab's project to parent (for triggering re-scan with correct project path)
  useEffect(() => {
    onProjectChange(effectiveActiveProject);
  }, [effectiveActiveProject, onProjectChange]);

  const [toolbarExpanded, setToolbarExpanded] = useState(false);
  const [selectedSet, setSelectedSet] = useState<Set<string>>(new Set());
  const [batchDeleteConfirm, setBatchDeleteConfirm] = useState<HookEntry[] | null>(null);
  const [singleDeleteConfirm, setSingleDeleteConfirm] = useState<HookEntry | null>(null);
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; items: MenuItem[] } | null>(null);
  const [moveConfirm, setMoveConfirm] = useState<{
    hook: HookEntry;
    targetScope: "user" | "project";
    fromLabel: string;
    toLabel: string;
    targetProject?: ProjectInfo;
  } | null>(null);
  const [projectPicker, setProjectPicker] = useState<{
    hook: HookEntry;
    action: "copy" | "move";
    excludePath?: string; // when set, exclude this project from the picker (for cross-project copy/move)
  } | null>(null);
  const [batchPicker, setBatchPicker] = useState<{
    hooks: HookEntry[];
    action: "copy" | "move";
    toScope: "user" | "project";
  } | null>(null);
  const [batchMoveConfirm, setBatchMoveConfirm] = useState<{
    hooks: HookEntry[];
    action: "copy" | "move";
    toScope: "user" | "project";
    targetProject?: ProjectInfo;
  } | null>(null);
  const lastClickedRef = useRef<{ id: string; section: string } | null>(null);

  /** Extract project root path from a hook's config file path.
   *  e.g. "/foo/proj/.claude/settings.json" → "/foo/proj" */
  const getHookProjectRoot = (hookPath: string): string => {
    const dirs = ["/.claude/", "/.codex/", "/.qoder/"];
    for (const dir of dirs) {
      const idx = hookPath.indexOf(dir);
      if (idx !== -1) return hookPath.slice(0, idx);
    }
    return "";
  };

  const toggleSection = (key: string) => {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };

  // Filter hooks by search + CLI filter + status + scope + project
  const filteredHooks = useMemo(() => {
    if (!data) return [];
    let hooks = data.hooks;
    const q = search.toLowerCase();
    if (q) {
      hooks = hooks.filter(
        (h) =>
          (h.name && h.name.toLowerCase().includes(q)) ||
          (h.description && h.description.toLowerCase().includes(q)) ||
          h.eventType.toLowerCase().includes(q) ||
          h.command.toLowerCase().includes(q) ||
          h.cli.toLowerCase().includes(q) ||
          (h.matcher && h.matcher.toLowerCase().includes(q))
      );
    }
    if (cliFilter !== "all") {
      hooks = hooks.filter((h) => h.cli === cliFilter);
    }
    if (statusFilter === "enabled") {
      hooks = hooks.filter((h) => h.enabled);
    } else if (statusFilter === "disabled") {
      hooks = hooks.filter((h) => !h.enabled);
    }
    // Scope filter — explicit whitelist (never use negation), matching ResourceList pattern
    if (hideScopeTabs && activeProject) {
      hooks = hooks.filter((h) => h.source !== "system");
      if (sourceFilter === "project") {
        hooks = hooks.filter((h) => h.source === "project" || h.projectName);
      } else if (sourceFilter === "inherited") {
        hooks = hooks.filter((h) => h.inherited || (h.source !== "project" && !h.projectName));
      }
    } else if (activeScope === "user") {
      hooks = hooks.filter((h) => h.source === "user" || h.source === "system");
    } else if (activeScope === "project") {
      hooks = hooks.filter((h) => h.source === "project");
    }
    // Project filter — when a specific project is selected, only show hooks from that project.
    // Explicitly skip on "user" scope tab (matches ResourceList's filterByProject guard).
    if (effectiveActiveProject && activeScope !== "user") {
      const pp = effectiveActiveProject.path.endsWith("/")
        ? effectiveActiveProject.path
        : effectiveActiveProject.path + "/";
      hooks = hooks.filter((h) => {
        if (h.source !== "project" && !h.projectName) return true;
        return h.path.startsWith(pp);
      });
    }
    return hooks;
  }, [data, search, cliFilter, statusFilter, sourceFilter, activeScope, effectiveActiveProject, hideScopeTabs, activeProject]);

  // Group hooks by event type; catch unknowns in "Other"
  const grouped = useMemo(() => {
    const map: Record<string, HookEntry[]> = {};
    for (const et of EVENT_TYPES) {
      map[et] = [];
    }
    map["Other"] = [];
    for (const hook of filteredHooks) {
      const key = EVENT_TYPES.includes(hook.eventType as any) ? hook.eventType : "Other";
      if (!map[key]) map[key] = [];
      map[key].push(hook);
    }
    return map;
  }, [filteredHooks]);

  // Build ordered list of sections
  const sectionOrder: string[] = useMemo(() => {
    const known: string[] = EVENT_TYPES.filter((et) => (grouped[et] || []).length > 0) as string[];
    if ((grouped["Other"] || []).length > 0) known.push("Other");
    return known;
  }, [grouped]);

  const totalHooks = filteredHooks.length;

  // Reset batch selection when data source, scope tab, or project filter changes.
  // Does NOT reset on search/cli/status filter changes — those narrow the view but
  // the underlying data is the same; clearing selection on every keystroke is disruptive.
  useEffect(() => { setSelectedSet(new Set()); }, [data, activeScope, effectiveActiveProject]);

  // Multi-select logic
  const toggleSelect = useCallback((id: string, section: string, shiftKey: boolean, metaKey: boolean) => {
    if (shiftKey && lastClickedRef.current && lastClickedRef.current.section === section) {
      const sectionIds = filteredHooks.filter((h) => {
        const key = EVENT_TYPES.includes(h.eventType as any) ? h.eventType : "Other";
        return key === section;
      }).map((h) => h.id);
      const start = sectionIds.indexOf(lastClickedRef.current.id);
      const end = sectionIds.indexOf(id);
      if (start >= 0 && end >= 0) {
        const [lo, hi] = start < end ? [start, end] : [end, start];
        const rangeIds = sectionIds.slice(lo, hi + 1);
        setSelectedSet((prev) => {
          const next = new Set(prev);
          rangeIds.forEach((rid) => next.add(rid));
          return next;
        });
      }
    } else if (metaKey) {
      setSelectedSet((prev) => {
        const next = new Set(prev);
        if (next.has(id)) next.delete(id);
        else next.add(id);
        return next;
      });
      lastClickedRef.current = { id, section };
    }
  }, [filteredHooks]);

  // Click handler with checkbox/range-select support
  const handleClick = useCallback((
    hook: HookEntry,
    section: string,
    e: React.MouseEvent,
  ) => {
    const isSystem = hook.source === "system";
    const isCheckbox = (e.target as HTMLElement).closest("[data-checkbox]");
    if (isCheckbox && !isSystem) {
      e.preventDefault();
      toggleSelect(hook.id, section, e.shiftKey, true);
    } else if ((e.metaKey || e.ctrlKey || e.shiftKey) && !isSystem) {
      e.preventDefault();
      toggleSelect(hook.id, section, e.shiftKey, e.metaKey || e.ctrlKey);
    } else {
      setSelectedSet(new Set());
      onSelect(hook);
    }
  }, [toggleSelect, onSelect]);

  const showBatch = selectedSet.size > 0;

  // Batch operation helpers
  const batchHandlers = useMemo(() => {
    const allIds = filteredHooks.map((h) => h.id);
    const allSelected = allIds.length > 0 && allIds.every((id) => selectedSet.has(id));
    const anySelected = selectedSet.size > 0;

    const selectAll = () => {
      setSelectedSet((prev) => {
        const next = new Set(prev);
        if (allSelected) {
          allIds.forEach((id) => next.delete(id));
        } else {
          allIds.forEach((id) => next.add(id));
        }
        return next;
      });
    };

    const enableAll = () => {
      if (!anySelected || !onToggleHook) return;
      filteredHooks.forEach((h) => {
        if (selectedSet.has(h.id) && !h.enabled) {
          onToggleHook(h, true);
        }
      });
      setSelectedSet(new Set());
    };

    const disableAll = () => {
      if (!anySelected || !onToggleHook) return;
      filteredHooks.forEach((h) => {
        if (selectedSet.has(h.id) && h.enabled) {
          onToggleHook(h, false);
        }
      });
      setSelectedSet(new Set());
    };

    const deleteAll = () => {
      if (!anySelected || !onDeleteHook) return;
      const hooks = filteredHooks.filter((h) => selectedSet.has(h.id));
      if (hooks.length > 0) setBatchDeleteConfirm(hooks);
    };

    // Batch copy/move
    const collectSelectedHooks = (): HookEntry[] => {
      return filteredHooks.filter((h) => selectedSet.has(h.id) && h.source !== "system");
    };

    const canMoveCopy = selectedSet.size > 0
      && activeScope !== "all"
      && collectSelectedHooks().length > 0;
    const canMoveToProject = canMoveCopy && activeScope === "user" && projects.length > 0;
    const canMoveToUser = canMoveCopy && activeScope === "project";

    const moveAll = () => {
      const hooks = collectSelectedHooks();
      if (hooks.length === 0) return;
      if (activeScope === "user") {
        if (projects.length === 0) return;
        if (projects.length === 1) {
          setBatchMoveConfirm({ hooks, action: "move", toScope: "project", targetProject: projects[0] });
        } else {
          setBatchPicker({ hooks, action: "move", toScope: "project" });
        }
      } else if (activeScope === "project") {
        setBatchMoveConfirm({ hooks, action: "move", toScope: "user" });
      }
    };

    const copyAll = () => {
      const hooks = collectSelectedHooks();
      if (hooks.length === 0) return;
      if (activeScope === "user") {
        if (projects.length === 0) return;
        if (projects.length === 1) {
          setBatchMoveConfirm({ hooks, action: "copy", toScope: "project", targetProject: projects[0] });
        } else {
          setBatchPicker({ hooks, action: "copy", toScope: "project" });
        }
      } else if (activeScope === "project") {
        setBatchMoveConfirm({ hooks, action: "copy", toScope: "user" });
      }
    };

    return { selectAll, allSelected, anySelected, enableAll, disableAll, deleteAll, canMoveToProject, canMoveToUser, moveAll, copyAll, collectSelectedHooks };
  }, [filteredHooks, selectedSet, onToggleHook, onDeleteHook, activeScope, projects]);

  const clearSelection = () => setSelectedSet(new Set());

  if (loading) {
    return (
      <div className="flex-1 flex items-center justify-center bg-[var(--app-sidebar)]">
        <div className="flex flex-col items-center gap-2">
          <div className="w-5 h-5 border-2 border-[var(--app-accent)] border-t-transparent rounded-full animate-spin" />
          <span className="text-xs text-[var(--app-text-muted)] font-mono">扫描中...</span>
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full bg-[var(--app-sidebar)] select-none">
      {/* Header */}
      <div className="px-2.5 pt-2.5 pb-2">
        {!hideScopeTabs && (
        <div className="flex items-center gap-1.5 mb-2.5">
          <Terminal size={13} className="text-[var(--app-accent)] shrink-0" />
          <span className="text-2xs text-[var(--app-text)] font-mono tracking-[0.15em] uppercase flex-1">
            Hooks
          </span>
        </div>
        )}

        {/* Scope tabs */}
        {!hideScopeTabs && (
        <div className="mb-2">
          <ScopeTabs
            active={activeScope}
            onChange={onScopeChange}
            userCount={data?.hooks?.filter((h) => h.source !== "project").length ?? 0}
            projectCount={data?.hooks?.filter((h) => h.source === "project").length ?? 0}
          />
        </div>
        )}

        {/* Project selector — only on all/project tabs (hidden when scope tabs are hidden) */}
        {!hideScopeTabs && activeScope !== "user" && (
          <div className="mb-2">
            <ProjectSelector
              projects={projects}
              active={effectiveActiveProject}
              onSelect={handleProjectChange}
              onAddProject={onAddProject}
            />
          </div>
        )}

        <div className="flex flex-col gap-1.5">
          <SearchInput
            value={search}
            onChange={setSearch}
            placeholder="搜索 hooks..."
          />
          <div className="flex gap-1.5">
            <div className="flex-1">
              <FilterDropdown
                value={cliFilter}
                options={CLI_FILTER_OPTIONS}
                onChange={setCliFilter}
                bordered
              />
            </div>
            <div className="flex-1">
              <FilterDropdown
                value={statusFilter}
                options={STATUS_OPTIONS}
                onChange={setStatusFilter}
                bordered
              />
            </div>
          </div>
          {hideScopeTabs && activeProject && (
            <div className="flex">
              <FilterDropdown
                value={sourceFilter}
                options={PROJECT_HOOK_SOURCE_OPTIONS}
                onChange={setSourceFilter}
                bordered
              />
            </div>
          )}
        </div>
      </div>

      {/* Toolbar — expandable pattern */}
      <div className="flex items-center gap-1 px-2.5 pb-2">
        {/* Add Hook — always visible */}
        <button
          onClick={onAddHook}
          className="flex items-center gap-1 px-2 py-0.5 bg-[var(--app-accent)] text-[var(--app-bg)]
            text-2xs font-mono hover:opacity-90 transition-opacity shrink-0"
          title="添加 Hook"
        >
          <Plus size={12} />
          <span>新建</span>
        </button>

        {/* Hook Store — only on user-level tab (project-level hooks come from project configs). Always shown when scope tabs are hidden (user-only mode). */}
        {(hideScopeTabs || activeScope === "user") && (
        <button
          onClick={onOpenStore}
          className="p-0.5 text-[var(--app-text-muted)] hover:text-[var(--app-accent)] hover:bg-[var(--app-hover)] transition-colors shrink-0"
          title="Hook 市场"
        >
          <Store size={13} />
        </button>
        )}

        {/* Refresh — if provided */}
        {onRefresh && (
          <button
            onClick={onRefresh}
            className="p-0.5 text-[var(--app-text-muted)] hover:text-[var(--app-accent)] hover:bg-[var(--app-hover)] transition-colors shrink-0"
            title="刷新"
          >
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <polyline points="23 4 23 10 17 10"/><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"/>
            </svg>
          </button>
        )}

        {/* Divider */}
        <div className={`h-4 border-l border-[var(--app-border)] mx-0.5 shrink-0 transition-opacity duration-200 ${toolbarExpanded ? "opacity-100" : "opacity-0"}`} />

        {/* Expandable section — batch operations */}
        <div
          className="flex items-center gap-1 overflow-hidden transition-all duration-200 ease-out"
          style={{
            maxWidth: toolbarExpanded ? "250px" : "0px",
            opacity: toolbarExpanded ? 1 : 0,
          }}
        >
          {/* Select all / deselect all */}
          <button
            onClick={batchHandlers.selectAll}
            className="p-0.5 text-[var(--app-text-muted)] hover:text-[var(--app-amber)] hover:bg-[var(--app-hover)] transition-colors shrink-0"
            title={batchHandlers.allSelected ? "取消全选" : "全选"}
          >
            {batchHandlers.allSelected
              ? <CheckSquare size={13} className="text-[var(--app-amber)]" />
              : <Square size={13} />
            }
          </button>

          {/* Batch enable / disable */}
          {batchHandlers.anySelected && (
            <>
              <button
                onClick={batchHandlers.enableAll}
                className="p-0.5 text-[var(--app-text-muted)] hover:text-[var(--app-accent)] hover:bg-[var(--app-hover)] transition-colors shrink-0"
                title={`启用已选的 ${selectedSet.size} 项`}
              >
                <Play size={13} />
              </button>
              <button
                onClick={batchHandlers.disableAll}
                className="p-0.5 text-[var(--app-text-muted)] hover:text-[var(--app-red)] hover:bg-[var(--app-hover)] transition-colors shrink-0"
                title={`禁用已选的 ${selectedSet.size} 项`}
              >
                <Ban size={13} />
              </button>
            </>
          )}

          {/* Batch copy/move */}
          {(batchHandlers.canMoveToProject || batchHandlers.canMoveToUser) && (
            <>
              <div className="h-4 border-l border-[var(--app-border)] mx-0.5 shrink-0" />
              {batchHandlers.canMoveToProject && (
                <>
                  <button
                    onClick={batchHandlers.copyAll}
                    className="p-0.5 text-[var(--app-text-muted)] hover:text-[var(--app-accent)] hover:bg-[var(--app-hover)] transition-colors shrink-0"
                    title={`复制已选的 ${selectedSet.size} 项到项目级`}
                  >
                    <Copy size={13} />
                  </button>
                  <button
                    onClick={batchHandlers.moveAll}
                    className="p-0.5 text-[var(--app-text-muted)] hover:text-[var(--app-amber)] hover:bg-[var(--app-hover)] transition-colors shrink-0"
                    title={`移动已选的 ${selectedSet.size} 项到项目级`}
                  >
                    <ArrowRight size={13} />
                  </button>
                </>
              )}
              {batchHandlers.canMoveToUser && (
                <>
                  <button
                    onClick={batchHandlers.copyAll}
                    className="p-0.5 text-[var(--app-text-muted)] hover:text-[var(--app-accent)] hover:bg-[var(--app-hover)] transition-colors shrink-0"
                    title={`复制已选的 ${selectedSet.size} 项到用户级`}
                  >
                    <Copy size={13} />
                  </button>
                  <button
                    onClick={batchHandlers.moveAll}
                    className="p-0.5 text-[var(--app-text-muted)] hover:text-[var(--app-amber)] hover:bg-[var(--app-hover)] transition-colors shrink-0"
                    title={`移动已选的 ${selectedSet.size} 项到用户级`}
                  >
                    <ArrowLeft size={13} />
                  </button>
                </>
              )}
            </>
          )}

          {/* Batch delete */}
          {batchHandlers.anySelected && (
            <>
              <div className="h-4 border-l border-[var(--app-border)] mx-0.5 shrink-0" />
              <button
                onClick={batchHandlers.deleteAll}
                className="p-0.5 text-[var(--app-text-muted)] hover:text-[var(--app-red)] hover:bg-[var(--app-red-bg)] transition-colors shrink-0"
                title={`删除已选的 ${selectedSet.size} 项`}
              >
                <Trash2 size={13} />
              </button>
            </>
          )}

          {/* Clear selection */}
          {batchHandlers.anySelected && (
            <button
              onClick={clearSelection}
              className="p-0.5 text-[var(--app-text-muted)] hover:text-[var(--app-text)] hover:bg-[var(--app-hover)] transition-colors shrink-0"
              title="清除选择"
            >
              <X size={13} />
            </button>
          )}
        </div>

        {/* Expand toggle */}
        <button
          onClick={() => setToolbarExpanded(!toolbarExpanded)}
          className={`shrink-0 p-0.5 transition-all duration-fast
            ${toolbarExpanded
              ? "text-[var(--app-accent)] bg-[var(--app-hover)]"
              : "text-[var(--app-text-dim)] hover:text-[var(--app-text)] hover:bg-[var(--app-hover)]"
            }`}
          title={toolbarExpanded ? "收起" : "批量操作"}
        >
          <Ellipsis size={13} />
        </button>
      </div>

      <div className="mx-2.5 border-b border-[var(--app-border-light)]" />

      {/* List */}
      {totalHooks === 0 ? (
        <div className="flex-1 flex items-center justify-center px-6">
          <div className="flex flex-col items-center gap-3 text-center">
            <Terminal size={24} className="text-[var(--app-text-muted)] opacity-20" />
            <div>
              <p className="text-xs text-[var(--app-text-dim)] font-mono">
                {search || cliFilter !== "all" ? "未找到匹配的 Hook" : "未找到任何 Hook 配置"}
              </p>
              {!search && cliFilter === "all" && (
                <p className="text-2xs text-[var(--app-text-muted)] font-mono mt-1 opacity-60">
                  点击「新建」按钮创建第一个 Hook
                </p>
              )}
            </div>
          </div>
        </div>
      ) : (
        <div className="flex-1 overflow-y-auto overflow-x-hidden py-0.5">
          {sectionOrder.map((et) => {
            const hooks = grouped[et] || [];
            const isCollapsed = collapsed.has(et);
            return (
              <div key={et}>
                <SectionHeader
                  label={et}
                  count={hooks.length}
                  collapsed={isCollapsed}
                  onToggle={() => toggleSection(et)}
                />
                {!isCollapsed && hooks.map((hook) => {
                  const isSystem = hook.source === "system";
                  return (
                  <ListRow
                    key={hook.id}
                    hook={hook}
                    selected={selectedId === hook.id}
                    checked={isSystem ? false : selectedSet.has(hook.id)}
                    showCheck={isSystem ? false : showBatch}
                    noCheck={isSystem}
                    onClick={(e) => handleClick(hook, et, e)}
                    onContextMenu={isSystem ? undefined : (e) => {
                      e.preventDefault();
                      const isProject = hook.source === "project";
                      const targetScope = (isProject ? "user" : "project") as "user" | "project";
                      const isTowardProject = targetScope === "project";
                      const fromLabel = isProject
                        ? `项目级 · ${effectiveActiveProject?.name ?? ""}`
                        : "用户级";
                      const noProject = isTowardProject && projects.length === 0;
                      const singleProject = isTowardProject && projects.length === 1;
                      const arrow = isProject ? "←" : "→";
                      const menuItems: MenuItem[] = [
                        {
                          label: hook.enabled ? "禁用" : "启用",
                          icon: hook.enabled ? <Ban size={13} /> : <Play size={13} />,
                          onClick: () => onToggleHook?.(hook, !hook.enabled),
                        },
                        // Copy/move between scopes.
                        // When hideScopeTabs is set, we may still have project-level hooks
                        // (e.g. ProjectWorkspace shows only its own project hooks).
                        // Always allow project↔user direction regardless of hideScopeTabs.
                        { separator: true as const },
                        {
                          label: isTowardProject ? "复制到项目级" : "复制到用户级",
                          icon: <span>{arrow}</span>,
                          disabled: noProject || (hideScopeTabs && !isTowardProject && !isProject),
                          onClick: () => {
                            if (isTowardProject) {
                              if (singleProject) {
                                onCopyHook?.(hook, "project", projects[0]);
                              } else {
                                setProjectPicker({ hook, action: "copy" });
                              }
                            } else {
                              // project→user (when hideScopeTabs, isProject must be true to reach here)
                              onCopyHook?.(hook, "user");
                            }
                          },
                        },
                        {
                          label: isTowardProject ? "移动到项目级" : "移动到用户级",
                          icon: <span>{arrow}</span>,
                          disabled: noProject || (hideScopeTabs && !isTowardProject && !isProject),
                          onClick: () => {
                            if (isTowardProject) {
                              if (singleProject) {
                                setMoveConfirm({
                                  hook,
                                  targetScope: "project",
                                  fromLabel,
                                  toLabel: `项目级 · ${projects[0].name}`,
                                  targetProject: projects[0],
                                });
                              } else {
                                setProjectPicker({ hook, action: "move" });
                              }
                            } else {
                              // project→user (when hideScopeTabs, isProject must be true to reach here)
                              setMoveConfirm({
                                hook,
                                targetScope: "user",
                                fromLabel,
                                toLabel: "用户级",
                              });
                            }
                          },
                        },
                        // Cross-project copy/move (for project-level hooks, even when scope tabs hidden)
                        ...(isProject ? [
                            {
                              label: "复制到其他项目…",
                              icon: <Folder size={13} />,
                              disabled: projects.length <= 1,
                              onClick: () => {
                                setProjectPicker({ hook, action: "copy", excludePath: getHookProjectRoot(hook.path) });
                              },
                            },
                            {
                              label: "移动到其他项目…",
                              icon: <Folder size={13} />,
                              disabled: projects.length <= 1,
                              onClick: () => {
                                setProjectPicker({ hook, action: "move", excludePath: getHookProjectRoot(hook.path) });
                              },
                            },
                          ] : []),
                        { separator: true },
                        {
                          label: "删除",
                          icon: <Trash2 size={13} />,
                          danger: true,
                          onClick: () => setSingleDeleteConfirm(hook),
                        },
                      ];
                      setContextMenu({ x: e.clientX, y: e.clientY, items: menuItems });
                    }}
                  />
                );
                })}
              </div>
            );
          })}
        </div>
      )}

      {/* Selection status bar */}
      {showBatch && (
        <div className="shrink-0 border-t border-[var(--app-border)] bg-[var(--app-statusbar)] px-3 py-1 flex items-center gap-2">
          <span className="text-2xs text-[var(--app-amber)] font-mono tabular-nums">
            {selectedSet.size} 项已选
          </span>
          <div className="flex-1" />
          <button
            onClick={clearSelection}
            className="flex items-center gap-0.5 text-2xs text-[var(--app-text-muted)] hover:text-[var(--app-text)] font-mono transition-colors"
          >
            <X size={10} />
            <span>清除</span>
          </button>
        </div>
      )}

      {/* Footer summary */}
      {data && !loading && (
        <div className="shrink-0 border-t border-[var(--app-border)] px-3 py-1.5 bg-[var(--app-statusbar)]">
          <span className="text-2xs text-[var(--app-text-muted)] font-mono">
            {data.hooks.length} 个 hooks
          </span>
        </div>
      )}

      {/* Batch delete confirmation */}
      <ConfirmDialog
        open={batchDeleteConfirm !== null}
        title="批量删除 Hook"
        message={`确定要删除已选的 ${batchDeleteConfirm?.length || 0} 个 Hook 吗？此操作不可撤销。`}
        confirmLabel="删除"
        onConfirm={() => {
          if (batchDeleteConfirm && onDeleteHook) {
            batchDeleteConfirm.forEach((h) => onDeleteHook(h));
            setBatchDeleteConfirm(null);
            setSelectedSet(new Set());
          }
        }}
        onCancel={() => setBatchDeleteConfirm(null)}
      />

      {/* Single delete confirmation (context menu) */}
      <ConfirmDialog
        open={singleDeleteConfirm !== null}
        title="删除 Hook"
        message={`确定要删除 "${singleDeleteConfirm?.name || singleDeleteConfirm?.eventType || ""}" 吗？此操作不可撤销。`}
        confirmLabel="删除"
        onConfirm={() => {
          if (singleDeleteConfirm && onDeleteHook) {
            onDeleteHook(singleDeleteConfirm);
            setSingleDeleteConfirm(null);
          }
        }}
        onCancel={() => setSingleDeleteConfirm(null)}
      />

      {/* Project picker dialog — shown when multiple projects exist */}
      <Dialog
        open={projectPicker !== null}
        onClose={() => setProjectPicker(null)}
        width="340px"
        persistent
      >
        <div className="flex items-center justify-between px-4 py-3 border-b border-app-border">
          <div className="flex items-center gap-2">
            <Folder size={15} className="text-[var(--app-accent)]" />
            <h3 className="font-semibold text-sm text-app-text font-mono">
              {projectPicker?.action === "copy" ? "复制到项目" : "移动到项目"}
            </h3>
          </div>
          <button
            onClick={() => setProjectPicker(null)}
            className="p-1 text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)] transition-colors"
          >
            <X size={14} />
          </button>
        </div>
        <div className="py-1 max-h-[300px] overflow-y-auto">
          {(() => {
            const excludePath = projectPicker?.excludePath;
            const filtered = excludePath
              ? projects.filter((p) => p.path !== excludePath)
              : projects;
            if (filtered.length === 0) {
              return (
                <div className="px-4 py-4 text-center text-2xs text-[var(--app-text-muted)] font-mono">
                  没有其他项目可选
                </div>
              );
            }
            return filtered.map((p) => (
            <button
              key={p.name}
              onClick={() => {
                const hook = projectPicker!.hook;
                const isProject = hook.source === "project";
                const fromLabel = isProject
                  ? `项目级 · ${activeProject?.name ?? ""}`
                  : "用户级";
                if (projectPicker!.action === "copy") {
                  onCopyHook?.(hook, "project", p);
                } else {
                  setMoveConfirm({
                    hook,
                    targetScope: "project",
                    fromLabel,
                    toLabel: `项目级 · ${p.name}`,
                    targetProject: p,
                  });
                }
                setProjectPicker(null);
              }}
              className="w-full text-left px-4 py-2.5 flex items-center gap-3 text-xs font-mono
                text-[var(--app-text-dim)] hover:bg-[var(--app-hover)] hover:text-[var(--app-text)]
                transition-colors group"
            >
              <Folder size={13} className="shrink-0 text-[var(--app-text-muted)] group-hover:text-[var(--app-accent)] transition-colors" />
              <div className="flex-1 min-w-0">
                <div className="truncate">{p.name}</div>
                <div className="text-2xs text-[var(--app-text-muted)] truncate mt-0.5 opacity-60">{p.path}</div>
              </div>
              <span className="text-2xs text-[var(--app-accent)] opacity-0 group-hover:opacity-100 transition-opacity shrink-0 font-mono">
                {projectPicker?.action === "copy" ? "复制" : "移动"}
              </span>
            </button>
            ));
          })()}
        </div>
        <div className="border-t border-[var(--app-border-light)] px-4 py-2">
          <button
            onClick={() => {
              setProjectPicker(null);
              onAddProject();
            }}
            className="w-full text-left px-2 py-1.5 flex items-center gap-2 text-xs font-mono
              text-[var(--app-text-muted)] hover:text-[var(--app-accent)] hover:bg-[var(--app-hover)]
              transition-colors"
          >
            <Plus size={13} />
            <span>注册新项目...</span>
          </button>
        </div>
      </Dialog>

      {/* Move confirmation dialog */}
      <ConfirmDialog
        open={moveConfirm !== null}
        title={`移动 ${(moveConfirm?.hook.name || moveConfirm?.hook.eventType) ?? ""}`}
        message={`此操作将从 ${moveConfirm?.fromLabel ?? ""} 移除 "${(moveConfirm?.hook.name || moveConfirm?.hook.eventType) ?? ""}"，并添加到 ${moveConfirm?.toLabel ?? ""}。${moveConfirm?.targetScope === "project" ? "\n\n项目级资源可提交到 Git 与团队共享。" : ""}`}
        confirmLabel="确认移动"
        variant="primary"
        onConfirm={() => {
          if (moveConfirm) {
            onMoveHook?.(moveConfirm.hook, moveConfirm.targetScope, moveConfirm.targetProject);
            setMoveConfirm(null);
          }
        }}
        onCancel={() => setMoveConfirm(null)}
      />

      {/* Batch project picker dialog */}
      <Dialog
        open={batchPicker !== null}
        onClose={() => setBatchPicker(null)}
        width="340px"
        persistent
      >
        <div className="flex items-center justify-between px-4 py-3 border-b border-app-border">
          <div className="flex items-center gap-2">
            <Folder size={15} className="text-[var(--app-accent)]" />
            <h3 className="font-semibold text-sm text-app-text font-mono">
              批量{batchPicker?.action === "copy" ? "复制" : "移动"}到项目
            </h3>
          </div>
          <button onClick={() => setBatchPicker(null)} className="p-1 text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)] transition-colors">
            <X size={14} />
          </button>
        </div>
        <div className="px-4 py-2 text-2xs text-[var(--app-text-muted)] font-mono border-b border-[var(--app-border-light)]">
          已选 {batchPicker?.hooks.length ?? 0} 项 Hook
        </div>
        <div className="py-1 max-h-[300px] overflow-y-auto">
          {projects.map((p) => (
            <button
              key={p.name}
              onClick={() => {
                if (batchPicker) {
                  setBatchMoveConfirm({
                    hooks: batchPicker.hooks,
                    action: batchPicker.action,
                    toScope: batchPicker.toScope,
                    targetProject: p,
                  });
                }
                setBatchPicker(null);
              }}
              className="w-full text-left px-4 py-2.5 flex items-center gap-3 text-xs font-mono text-[var(--app-text-dim)] hover:bg-[var(--app-hover)] hover:text-[var(--app-text)] transition-colors group"
            >
              <Folder size={13} className="shrink-0 text-[var(--app-text-muted)] group-hover:text-[var(--app-accent)] transition-colors" />
              <div className="flex-1 min-w-0">
                <div className="truncate">{p.name}</div>
                <div className="text-2xs text-[var(--app-text-muted)] truncate mt-0.5 opacity-60">{p.path}</div>
              </div>
              <span className="text-2xs text-[var(--app-accent)] opacity-0 group-hover:opacity-100 transition-opacity shrink-0 font-mono">
                选择
              </span>
            </button>
          ))}
        </div>
        <div className="border-t border-[var(--app-border-light)] px-4 py-2">
          <button
            onClick={() => { setBatchPicker(null); onAddProject(); }}
            className="w-full text-left px-2 py-1.5 flex items-center gap-2 text-xs font-mono text-[var(--app-text-muted)] hover:text-[var(--app-accent)] hover:bg-[var(--app-hover)] transition-colors"
          >
            <Plus size={13} />
            <span>注册新项目...</span>
          </button>
        </div>
      </Dialog>

      {/* Batch move/copy confirmation dialog */}
      <ConfirmDialog
        open={batchMoveConfirm !== null}
        title={`批量${batchMoveConfirm?.action === "copy" ? "复制" : "移动"}`}
        message={`确定要将已选的 ${batchMoveConfirm?.hooks.length ?? 0} 项 Hook${batchMoveConfirm?.action === "copy" ? "复制" : "移动"}到 ${batchMoveConfirm?.toScope === "project" ? `项目级 · ${batchMoveConfirm?.targetProject?.name ?? ""}` : "用户级"} 吗？\n\n${batchMoveConfirm?.action === "move" ? "移动后原位置 Hook 将被删除，可通过撤销恢复。" : ""}`}
        confirmLabel={`确认${batchMoveConfirm?.action === "copy" ? "复制" : "移动"}`}
        variant="primary"
        onConfirm={() => {
          if (batchMoveConfirm) {
            if (batchMoveConfirm.action === "copy") {
              onBatchCopyHooks?.(batchMoveConfirm.hooks, batchMoveConfirm.toScope, batchMoveConfirm.targetProject);
            } else {
              onBatchMoveHooks?.(batchMoveConfirm.hooks, batchMoveConfirm.toScope, batchMoveConfirm.targetProject);
            }
            setBatchMoveConfirm(null);
            setSelectedSet(new Set());
          }
        }}
        onCancel={() => setBatchMoveConfirm(null)}
      />

      {/* Context menu */}
      {contextMenu && (
        <ContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          items={contextMenu.items}
          onClose={() => setContextMenu(null)}
        />
      )}
    </div>
  );
}
