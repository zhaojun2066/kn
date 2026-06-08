import React, { useState, useMemo, useRef, useEffect, useCallback } from "react";
import { SearchInput } from "./common/SearchInput";
import { ConfirmDialog } from "./ConfirmDialog";
import { Dialog } from "./common/Dialog";
import { ContextMenu, type MenuItem } from "./ContextMenu";
import { ListRow } from "./common/ListRow";
import { SectionHeader } from "./common/SectionHeader";
import { CliBadge } from "./common/CliBadge";
import { FilterDropdown } from "./common/FilterDropdown";
import { ScopeTabs } from "./ScopeTabs";
import { ProjectSelector } from "./ProjectSelector";
import { CLI_CSS_COLORS, CLI_LABELS, CLI_HEX_COLORS, CLI_FILTER_OPTIONS } from "../lib/cli-constants";
import type { ProjectInfo, ScopeTab } from "../lib/types";
import {
  ChevronRight, Circle, Filter, Puzzle,
  FileText, Lock, CheckSquare, Square, Play, Ban, X,
  RefreshCw, Ellipsis, Trash2, Bot, Terminal,
  Folder, Plus, Copy, ArrowRight, ArrowLeft,
} from "lucide-react";

/* ──────────────────── Data Types ──────────────────── */

export type CliKind = "claude" | "codex" | "qoder";

export interface SkillEntry {
  name: string;
  path: string;
  description?: string;
}

export interface PluginEntry {
  id: string;
  cli: CliKind;
  name: string;
  marketplace: string;
  enabled: boolean;
  version?: string;
  source: "marketplace" | "bundled" | "user";
  skills: SkillEntry[];
  agents: AgentEntry[];
  commands: CommandEntry[];
}

export interface StandaloneSkill {
  id: string;
  cli: CliKind;
  name: string;
  enabled: boolean;
  linkType: "symlink" | "file" | "directory";
  path: string;
  projectName?: string;
}

export interface CommandEntry {
  id: string;
  cli: CliKind;
  name: string;
  path: string;
  description: string;
  enabled: boolean;
  projectName?: string;
}

export interface SkillManagerData {
  plugins: PluginEntry[];
  standaloneSkills: StandaloneSkill[];
  systemSkills: StandaloneSkill[];
  commands: CommandEntry[];
}

// ── Agent types ──

export interface AgentEntry {
  id: string;           // "claude:agent:security-auditor"
  cli: CliKind;
  name: string;
  description: string;
  enabled: boolean;
  source: "builtin" | "user" | "project" | "plugin";
  model?: string;
  tools: string[];
  color?: string;
  path: string;
  skills: string[];
  sandboxMode?: string; // Codex only
  projectName?: string;
}

export interface AgentManagerData {
  agents: AgentEntry[];
}

export type SelectedItem =
  | { type: "plugin"; data: PluginEntry }
  | { type: "standalone"; data: StandaloneSkill }
  | { type: "system"; data: StandaloneSkill }
  | { type: "agent"; data: AgentEntry }
  | { type: "plugin-skill"; data: { skill: { name: string; path: string; description?: string }; cli: string; parentPlugin: PluginEntry } }
  | { type: "plugin-agent"; data: AgentEntry & { parentPlugin: PluginEntry } }
  | { type: "command"; data: CommandEntry }
  | { type: "plugin-command"; data: CommandEntry & { parentPlugin: PluginEntry } };

export interface BatchToggleItem {
  cli: CliKind;
  id: string;
  enabled: boolean;
  path?: string;
}

export interface PluginUpdateInfo {
  pluginId: string;
  currentVersion: string;
  currentSha: string;
  latestSha: string;
  hasUpdate: boolean;
}

export interface MarketplacePluginEntry {
  name: string;
  marketplace: string;
  cli: CliKind;
  version?: string;
  description?: string;
  installed: boolean;
  skillCount: number;
}

export interface MarketplaceData {
  plugins: MarketplacePluginEntry[];
  marketplaces: string[];
}

interface SkillManagerProps {
  data: SkillManagerData | null;
  agentData?: AgentManagerData | null;
  loading: boolean;
  selectedId: string | null;
  onSelect: (item: SelectedItem | null) => void;
  onTogglePlugin: (cli: CliKind, pluginId: string, enabled: boolean) => void;
  onToggleStandaloneSkill: (cli: CliKind, skillId: string, enabled: boolean, path?: string) => void;
  onBatchToggle: (items: BatchToggleItem[], enabled: boolean) => void;
  onBatchUninstall: (items: BatchToggleItem[]) => void;
  onDeleteAgent?: (cli: CliKind, name: string, path?: string) => void;
  checkingUpdates: boolean;
  updateInfos: PluginUpdateInfo[];
  onCheckUpdates: () => void;
  onCancelCheckUpdates: () => void;
  onOpenMarketplace: () => void;
  onOpenGraph?: () => void;
  // Scope management
  activeScope: ScopeTab;
  onScopeChange: (scope: ScopeTab) => void;
  projects: ProjectInfo[];
  activeProject: ProjectInfo | null;
  onProjectChange: (project: ProjectInfo | null) => void;
  baselineUserCount?: number;
  baselineProjectCount?: number;
  onAddProject: () => void;
  onMoveResource?: (item: SelectedItem, toScope: "user" | "project", targetProject?: ProjectInfo) => void;
  onCopyResource?: (item: SelectedItem, toScope: "user" | "project", targetProject?: ProjectInfo) => void;
  onBatchMove?: (items: SelectedItem[], toScope: "user" | "project", targetProject?: ProjectInfo) => void;
  onBatchCopy?: (items: SelectedItem[], toScope: "user" | "project", targetProject?: ProjectInfo) => void;
  onToast?: (type: "error" | "success", message: string) => void;
}

/* ── CliBadge, SectionHeader, ListRow, DropFilter are now imported from common/ ── */
/* ── CLI constants are now imported from lib/cli-constants ── */

const STATUS_OPTIONS: { value: string; label: string }[] = [
  { value: "all", label: "全部状态" },
  { value: "enabled", label: "已启用" },
  { value: "disabled", label: "已禁用" },
];

/* ═══════════════════════════════════════════════════════════════
   MAIN COMPONENT
   ═══════════════════════════════════════════════════════════════ */

export function SkillManager({
  data,
  agentData,
  loading,
  selectedId,
  onSelect,
  onTogglePlugin,
  onToggleStandaloneSkill,
  onBatchToggle,
  onBatchUninstall,
  onDeleteAgent,
  checkingUpdates,
  updateInfos,
  onCheckUpdates,
  onCancelCheckUpdates,
  onOpenMarketplace,
  onOpenGraph,
  activeScope,
  onScopeChange,
  projects,
  activeProject,
  onProjectChange,
  onAddProject,
  baselineUserCount,
  baselineProjectCount,
  onMoveResource,
  onCopyResource,
  onBatchMove,
  onBatchCopy,
  onToast,
}: SkillManagerProps) {
  // Per-tab filter state — each tab remembers its own filters independently
  type TabFilters = { search: string; cliFilter: string; statusFilter: string };
  const [tabFilters, setTabFilters] = useState<Record<string, TabFilters>>({
    all: { search: "", cliFilter: "all", statusFilter: "all" },
    project: { search: "", cliFilter: "all", statusFilter: "all" },
    user: { search: "", cliFilter: "all", statusFilter: "all" },
  });
  const filters = tabFilters[activeScope] ?? tabFilters.all;
  const setSearch = (v: string) => setTabFilters((prev) => ({ ...prev, [activeScope]: { ...prev[activeScope], search: v } }));
  const setCliFilter = (v: string) => setTabFilters((prev) => ({ ...prev, [activeScope]: { ...prev[activeScope], cliFilter: v } }));
  const setStatusFilter = (v: string) => setTabFilters((prev) => ({ ...prev, [activeScope]: { ...prev[activeScope], statusFilter: v } }));
  const search = filters.search;
  const cliFilter = filters.cliFilter;
  const statusFilter = filters.statusFilter;

  // Per-tab project selection — each tab remembers its own project independently
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

  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [selectedSet, setSelectedSet] = useState<Set<string>>(new Set());
  const [toolbarExpanded, setToolbarExpanded] = useState(false);
  const [batchUninstallConfirm, setBatchUninstallConfirm] = useState<BatchToggleItem[] | null>(null);
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; items: MenuItem[] } | null>(null);
  const [moveConfirm, setMoveConfirm] = useState<{
    item: SelectedItem;
    targetScope: "user" | "project";
    resourceName: string;
    fromLabel: string;
    toLabel: string;
    targetProject?: ProjectInfo;
  } | null>(null);
  const [projectPicker, setProjectPicker] = useState<{
    item: SelectedItem;
    action: "copy" | "move";
    excludePath?: string; // when set, exclude this project from the picker (for cross-project copy/move)
  } | null>(null);
  const [batchPicker, setBatchPicker] = useState<{
    items: SelectedItem[];
    action: "copy" | "move";
    toScope: "user" | "project";
  } | null>(null);
  const [batchMoveConfirm, setBatchMoveConfirm] = useState<{
    items: SelectedItem[];
    action: "copy" | "move";
    toScope: "user" | "project";
    targetProject?: ProjectInfo;
  } | null>(null);
  const lastClickedRef = useRef<{ id: string; section: string } | null>(null);

  /** Extract project root path from a resource's file path.
   *  e.g. "/foo/proj/.claude/skills/hello.md" → "/foo/proj"
   *  Normalizes Windows backslashes to forward slashes before matching. */
  const getResourceProjectRoot = (resourcePath: string): string => {
    const normalized = resourcePath.replace(/\\/g, "/");
    const dirs = ["/.claude/", "/.codex/", "/.qoder/"];
    for (const dir of dirs) {
      const idx = normalized.indexOf(dir);
      if (idx !== -1) return normalized.slice(0, idx);
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

  const filterByStatus = (enabled: boolean) =>
    statusFilter === "all" || (statusFilter === "enabled" ? enabled : !enabled);

  // Truncated project badge for list rows
  const ProjectBadge = ({ name }: { name: string }) => (
    <span
      className="text-2xs text-amber-400 bg-amber-500/10 px-1 py-px rounded font-mono max-w-[72px] truncate shrink-0"
      title={name}
    >
      {name}
    </span>
  );

  const filtered = useMemo(() => {
    if (!data) return null;
    const q = search.toLowerCase();
    const filterBySearch = (name: string) => !q || name.toLowerCase().includes(q);
    const filterByCli = (cli: CliKind) => cliFilter === "all" || cli === cliFilter;
    const filterByScope = (id: string) => {
      if (activeScope === "all") return true;
      // Project-level items have ":project-" in their ID
      const isProject = id.includes(":project-");
      return activeScope === "project" ? isProject : !isProject;
    };
    // When a specific project is selected, only show items belonging to that project
    const filterByProject = (projectName?: string) => {
      if (!effectiveActiveProject || activeScope === "user") return true;
      return projectName === effectiveActiveProject.name;
    };

    return {
      // Hide plugins when viewing a specific project (plugins are not project-scoped)
      plugins: effectiveActiveProject ? [] : data.plugins.filter(
        (p) => filterByCli(p.cli) && filterByStatus(p.enabled) && filterByScope(p.id) && (filterBySearch(p.name) || p.skills.some((s) => filterBySearch(s.name)))
      ),
      standaloneSkills: data.standaloneSkills.filter(
        (s) => filterByCli(s.cli) && filterByStatus(s.enabled) && filterByScope(s.id) && filterByProject(s.projectName) && filterBySearch(s.name)
      ),
      systemSkills: data.systemSkills.filter(
        (s) => filterByCli(s.cli) && (statusFilter === "all" || statusFilter === "enabled") && filterByScope(s.id) && filterBySearch(s.name)
      ),
      commands: (data.commands || []).filter(
        (c) => filterByCli(c.cli) && filterByStatus(c.enabled) && filterByScope(c.id) && filterByProject(c.projectName) && filterBySearch(c.name)
      ),
    };
  }, [data, search, cliFilter, statusFilter, activeScope, activeProject]);

  // Filter agents based on search and CLI filter
  // Build a set of agent names that belong to installed plugins (any CLI).
  // A plugin may install Claude agents (e.g. ecc/agents/*.md) while the
  // same plugin also drops Codex agents into ~/.codex/agents/*.toml.
  // We hide standalone agents whose name matches any plugin agent name.
  const pluginAgentNames = useMemo(() => {
    const set = new Set<string>();
    if (data) {
      for (const plugin of data.plugins) {
        for (const agent of (plugin as any).agents || []) {
          set.add(agent.name);
        }
      }
    }
    return set;
  }, [data]);

  const filteredAgents = useMemo(() => {
    if (!agentData) return [];
    const q = search.toLowerCase();
    return agentData.agents.filter((a) => {
      // Exclude agents whose name matches a plugin-provided agent
      if (pluginAgentNames.has(a.name)) return false;
      if (cliFilter !== "all" && a.cli !== cliFilter) return false;
      if (q && !a.name.toLowerCase().includes(q) &&
          !a.description.toLowerCase().includes(q)) return false;
      if (!filterByStatus(a.enabled)) return false;
      // Scope filter
      if (activeScope === "project" && a.source !== "project") return false;
      if (activeScope === "user" && a.source === "project") return false;
      // Project filter: when a specific project is selected, only show its agents
      if (effectiveActiveProject && activeScope !== "user") {
        if (a.projectName !== effectiveActiveProject.name) return false;
      }
      return true;
    });
  }, [agentData, search, cliFilter, pluginAgentNames, statusFilter, activeScope, activeProject]);

  const agentSourceCounts = useMemo(() => {
    if (!agentData) return { user: 0, project: 0, builtin: 0 };
    return {
      user: filteredAgents.filter((a) => a.source === "user").length,
      project: filteredAgents.filter((a) => a.source === "project").length,
      builtin: filteredAgents.filter((a) => a.source === "builtin").length,
    };
  }, [agentData, filteredAgents]);

  // Reset batch selection when data source, scope tab, or project filter changes.
  // Does NOT reset on search/cli/status filter changes — those narrow the view but
  // the underlying data is the same; clearing selection on every keystroke is disruptive.
  useEffect(() => { setSelectedSet(new Set()); }, [data, activeScope, activeProject]);

  // Multi-select logic
  const toggleSelect = useCallback((id: string, section: string, shiftKey: boolean, metaKey: boolean) => {
    if (shiftKey && lastClickedRef.current && lastClickedRef.current.section === section) {
      // Range select within same section
      const sectionIds = section === "plugins"
        ? filtered?.plugins.map((p) => p.id) || []
        : section === "standalone"
          ? filtered?.standaloneSkills.map((s) => s.id) || []
          : [];
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
    } else {
      // Normal click — handled by onClick, don't change selection
    }
  }, [filtered]);

  // Checkbox-only click: toggle selection without triggering detail view
  const handleClick = useCallback((
    id: string,
    section: string,
    e: React.MouseEvent,
    onClickDetail: () => void,
  ) => {
    // System skills are readonly — never participate in batch selection
    if (section === "system") {
      onClickDetail();
      return;
    }
    const isCheckbox = (e.target as HTMLElement).closest("[data-checkbox]");
    if (isCheckbox) {
      e.preventDefault();
      toggleSelect(id, section, e.shiftKey, true);
    } else if (e.metaKey || e.ctrlKey || e.shiftKey) {
      e.preventDefault();
      toggleSelect(id, section, e.shiftKey, e.metaKey || e.ctrlKey);
    } else {
      setSelectedSet(new Set());
      onClickDetail();
    }
  }, [toggleSelect]);

  const showBatch = selectedSet.size > 0;

  // Global batch actions (across all sections)
  const globalBatchHandlers = useMemo(() => {
    if (!filtered) return { selectAll: () => {}, allSelected: false, anySelected: false };
    const allItems = [
      ...filtered.plugins.map((p) => ({ id: p.id, cli: p.cli })),
      ...filtered.standaloneSkills.map((s) => ({ id: s.id, cli: s.cli })),
      ...filteredAgents.filter((a) => a.source !== "builtin").map((a) => ({ id: a.id, cli: a.cli })),
      ...filtered.commands.map((c) => ({ id: c.id, cli: c.cli })),
    ];
    const allIds = allItems.map((it) => it.id);
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
      if (selectedSet.size === 0) return;
      const items: BatchToggleItem[] = [];
      selectedSet.forEach((id) => {
        const plugin = filtered.plugins.find((p) => p.id === id);
        if (plugin) { items.push({ cli: plugin.cli, id: plugin.id, enabled: true }); return; }
        const skill = filtered.standaloneSkills.find((s) => s.id === id);
        if (skill) { items.push({ cli: skill.cli, id: skill.id, enabled: true, path: skill.path }); return; }
        const agent = filteredAgents.find((a) => a.id === id);
        if (agent && agent.source !== "builtin") { items.push({ cli: agent.cli, id: agent.id, enabled: true, path: agent.path }); return; }
        const command = filtered.commands.find((c) => c.id === id);
        if (command) { items.push({ cli: command.cli, id: command.id, enabled: true, path: command.path }); }
      });
      if (items.length > 0) { onBatchToggle(items, true); setSelectedSet(new Set()); }
    };
    const disableAll = () => {
      if (selectedSet.size === 0) return;
      const items: BatchToggleItem[] = [];
      selectedSet.forEach((id) => {
        const plugin = filtered.plugins.find((p) => p.id === id);
        if (plugin) { items.push({ cli: plugin.cli, id: plugin.id, enabled: false }); return; }
        const skill = filtered.standaloneSkills.find((s) => s.id === id);
        if (skill) { items.push({ cli: skill.cli, id: skill.id, enabled: false, path: skill.path }); return; }
        const agent = filteredAgents.find((a) => a.id === id);
        if (agent && agent.source !== "builtin") { items.push({ cli: agent.cli, id: agent.id, enabled: false, path: agent.path }); return; }
        const command = filtered.commands.find((c) => c.id === id);
        if (command) { items.push({ cli: command.cli, id: command.id, enabled: false, path: command.path }); }
      });
      if (items.length > 0) { onBatchToggle(items, false); setSelectedSet(new Set()); }
    };
    const uninstallAll = () => {
      if (selectedSet.size === 0) return;
      const items: BatchToggleItem[] = [];
      selectedSet.forEach((id) => {
        const plugin = filtered.plugins.find((p) => p.id === id);
        if (plugin) { items.push({ cli: plugin.cli, id: plugin.id, enabled: plugin.enabled }); return; }
        const skill = filtered.standaloneSkills.find((s) => s.id === id);
        if (skill) { items.push({ cli: skill.cli, id: skill.id, enabled: skill.enabled, path: skill.path }); return; }
        const command = filtered.commands.find((c) => c.id === id);
        if (command) { items.push({ cli: command.cli, id: command.id, enabled: command.enabled, path: command.path }); }
      });
      if (items.length > 0) { setBatchUninstallConfirm(items); }
    };
    // Collect SelectedItem objects for all selected IDs (excluding plugins and builtin agents)
    const collectSelectedItems = (): SelectedItem[] => {
      const items: SelectedItem[] = [];
      selectedSet.forEach((id) => {
        const skill = filtered.standaloneSkills.find((s) => s.id === id);
        if (skill) { items.push({ type: "standalone", data: skill }); return; }
        const agent = filteredAgents.find((a) => a.id === id && a.source !== "builtin");
        if (agent) { items.push({ type: "agent", data: agent }); return; }
        const command = filtered.commands.find((c) => c.id === id);
        if (command) { items.push({ type: "command", data: command }); return; }
      });
      return items;
    };

    // Check if selection contains plugins (which can't be moved/copied)
    const hasPluginsSelected = (): boolean => {
      for (const id of selectedSet) {
        if (filtered.plugins.some((p) => p.id === id)) return true;
      }
      return false;
    };

    const moveAll = () => {
      if (hasPluginsSelected()) {
        onToast?.("error", "已选项中包含插件，插件不支持移动/复制。请取消选择插件后重试。");
        return;
      }
      const items = collectSelectedItems();
      if (items.length === 0) return;
      if (activeScope === "user") {
        // User → Project
        if (projects.length === 0) return; // disabled
        if (projects.length === 1) {
          setBatchMoveConfirm({ items, action: "move", toScope: "project", targetProject: projects[0] });
        } else {
          setBatchPicker({ items, action: "move", toScope: "project" });
        }
      } else if (activeScope === "project") {
        // Project → User
        setBatchMoveConfirm({ items, action: "move", toScope: "user" });
      }
    };

    const copyAll = () => {
      if (hasPluginsSelected()) {
        onToast?.("error", "已选项中包含插件，插件不支持复制/移动。请取消选择插件后重试。");
        return;
      }
      const items = collectSelectedItems();
      if (items.length === 0) return;
      if (activeScope === "user") {
        if (projects.length === 0) return;
        if (projects.length === 1) {
          setBatchMoveConfirm({ items, action: "copy", toScope: "project", targetProject: projects[0] });
        } else {
          setBatchPicker({ items, action: "copy", toScope: "project" });
        }
      } else if (activeScope === "project") {
        setBatchMoveConfirm({ items, action: "copy", toScope: "user" });
      }
    };

    // Determine if batch move/copy is available (buttons shown but click will error if plugins included)
    const canMoveCopy = selectedSet.size > 0
      && activeScope !== "all"
      && collectSelectedItems().length > 0;
    const canMoveToProject = canMoveCopy && activeScope === "user" && projects.length > 0;
    const canMoveToUser = canMoveCopy && activeScope === "project";

    return { selectAll, allSelected, anySelected, enableAll, disableAll, uninstallAll, moveAll, copyAll, canMoveToProject, canMoveToUser, collectSelectedItems };
  }, [filtered, filteredAgents, selectedSet, onBatchToggle, onBatchUninstall, activeScope, projects]);

  const clearSelection = () => setSelectedSet(new Set());

  function handleAgentClick(agent: AgentEntry, e: React.MouseEvent) {
    const target = e.target as HTMLElement;
    if (target.closest("[data-checkbox]")) {
      setSelectedSet((prev) => {
        const next = new Set(prev);
        if (next.has(agent.id)) next.delete(agent.id);
        else next.add(agent.id);
        return next;
      });
      return;
    }
    onSelect({ type: "agent", data: agent });
  }

  function ColorDot({ color }: { color: string }) {
    return (
      <span
        className="inline-block w-2.5 h-2.5 rounded-full border border-white/20"
        style={{ backgroundColor: color }}
      />
    );
  }

  if (!data && !loading) {
    return (
      <div className="flex flex-col items-center justify-center h-full gap-3 bg-[var(--app-sidebar)]">
        <Puzzle size={22} className="text-[var(--app-text-muted)] opacity-25" />
        <span className="text-xs text-[var(--app-text-dim)] font-mono">无法加载</span>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full bg-[var(--app-sidebar)] select-none">
      {/* Header */}
      <div className="px-2.5 pt-2.5 pb-2">
        <div className="flex items-center gap-1.5 mb-2.5">
          <Puzzle size={13} className="text-[var(--app-accent)] shrink-0" />
          <span className="text-2xs text-[var(--app-text)] font-mono tracking-[0.15em] uppercase flex-1">
            Skills & Plugins
          </span>
        </div>

        {/* Scope tabs */}
        <div className="mb-2">
          <ScopeTabs
            active={activeScope}
            onChange={onScopeChange}
            userCount={baselineUserCount ?? (data?.standaloneSkills?.filter((s) => !s.id.includes(":project-")).length ?? 0)}
            projectCount={baselineProjectCount ?? (data?.standaloneSkills?.filter((s) => s.id.includes(":project-")).length ?? 0)}
          />
        </div>

        {/* Project selector — only on all/project tabs */}
        {activeScope !== "user" && (
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
          <SearchInput value={search} onChange={setSearch} placeholder="搜索..." />
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
        </div>
      </div>

      {/* Batch operations toolbar — below filters, ExpandableToolbar pattern */}
      <div className="flex items-center gap-1 px-2.5 pb-2">
        {/* Marketplace — only on user-level tab */}
        {activeScope === "user" && (
        <button
          onClick={onOpenMarketplace}
          className="p-0.5 text-[var(--app-text-muted)] hover:text-[var(--app-accent)] hover:bg-[var(--app-hover)] transition-colors shrink-0"
          title="浏览 Marketplace"
        >
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <circle cx="12" cy="12" r="10"/><path d="M2 12h20"/><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"/>
          </svg>
        </button>
        )}

        {/* Update check — always visible */}
        {checkingUpdates ? (
          <div className="flex items-center gap-1 shrink-0">
            <RefreshCw size={11} className="text-[var(--app-accent)] animate-spin" />
            <button
              onClick={onCancelCheckUpdates}
              className="text-2xs text-[var(--app-text-muted)] hover:text-[var(--app-text)] font-mono"
            >
              取消
            </button>
          </div>
        ) : (
          <button
            onClick={onCheckUpdates}
            className="p-0.5 text-[var(--app-text-muted)] hover:text-[var(--app-accent)] hover:bg-[var(--app-hover)] transition-colors relative shrink-0"
            title="检查更新"
          >
            <RefreshCw size={12} />
            {updateInfos.filter((u) => u.hasUpdate).length > 0 && (
              <span className="absolute -top-0.5 -right-1 w-2 h-2 bg-[var(--app-amber)] rounded-full border border-[var(--app-bg)]" />
            )}
          </button>
        )}

        {/* Divider */}
        <div className={`h-4 border-l border-[var(--app-border)] mx-0.5 shrink-0 transition-opacity duration-200 ${toolbarExpanded ? "opacity-100" : "opacity-0"}`} />

        {/* Expandable section — batch operations */}
        <div
          className="flex items-center gap-1 overflow-hidden transition-all duration-200 ease-out"
          style={{
            maxWidth: toolbarExpanded ? "350px" : "0px",
            opacity: toolbarExpanded ? 1 : 0,
          }}
        >
          {/* Select all / deselect all */}
          <button
            onClick={globalBatchHandlers.selectAll}
            className="p-0.5 text-[var(--app-text-muted)] hover:text-[var(--app-amber)] hover:bg-[var(--app-hover)] transition-colors shrink-0"
            title={globalBatchHandlers.allSelected ? "取消全选" : "全选"}
          >
            {globalBatchHandlers.allSelected
              ? <CheckSquare size={13} className="text-[var(--app-amber)]" />
              : <Square size={13} />
            }
          </button>

          {/* Batch enable / disable */}
          {globalBatchHandlers.anySelected && (
            <>
              <button
                onClick={globalBatchHandlers.enableAll}
                className="p-0.5 text-[var(--app-text-muted)] hover:text-[var(--app-accent)] hover:bg-[var(--app-hover)] transition-colors shrink-0"
                title={`启用已选的 ${selectedSet.size} 项`}
              >
                <Play size={13} />
              </button>
              <button
                onClick={globalBatchHandlers.disableAll}
                className="p-0.5 text-[var(--app-text-muted)] hover:text-[var(--app-red)] hover:bg-[var(--app-hover)] transition-colors shrink-0"
                title={`禁用已选的 ${selectedSet.size} 项`}
              >
                <Ban size={13} />
              </button>
            </>
          )}

          {/* Batch move/copy */}
          {(globalBatchHandlers.canMoveToProject || globalBatchHandlers.canMoveToUser) && (
            <>
              <div className="h-4 border-l border-[var(--app-border)] mx-0.5 shrink-0" />
              {globalBatchHandlers.canMoveToProject && (
                <>
                  <button
                    onClick={globalBatchHandlers.copyAll}
                    className="p-0.5 text-[var(--app-text-muted)] hover:text-[var(--app-accent)] hover:bg-[var(--app-hover)] transition-colors shrink-0"
                    title={`复制已选的 ${selectedSet.size} 项到项目级`}
                  >
                    <Copy size={13} />
                  </button>
                  <button
                    onClick={globalBatchHandlers.moveAll}
                    className="p-0.5 text-[var(--app-text-muted)] hover:text-[var(--app-amber)] hover:bg-[var(--app-hover)] transition-colors shrink-0"
                    title={`移动已选的 ${selectedSet.size} 项到项目级`}
                  >
                    <ArrowRight size={13} />
                  </button>
                </>
              )}
              {globalBatchHandlers.canMoveToUser && (
                <>
                  <button
                    onClick={globalBatchHandlers.copyAll}
                    className="p-0.5 text-[var(--app-text-muted)] hover:text-[var(--app-accent)] hover:bg-[var(--app-hover)] transition-colors shrink-0"
                    title={`复制已选的 ${selectedSet.size} 项到用户级`}
                  >
                    <Copy size={13} />
                  </button>
                  <button
                    onClick={globalBatchHandlers.moveAll}
                    className="p-0.5 text-[var(--app-text-muted)] hover:text-[var(--app-amber)] hover:bg-[var(--app-hover)] transition-colors shrink-0"
                    title={`移动已选的 ${selectedSet.size} 项到用户级`}
                  >
                    <ArrowLeft size={13} />
                  </button>
                </>
              )}
            </>
          )}

          {/* Batch uninstall */}
          {globalBatchHandlers.anySelected && (
            <>
              <div className="h-4 border-l border-[var(--app-border)] mx-0.5 shrink-0" />
              <button
                onClick={globalBatchHandlers.uninstallAll}
                className="p-0.5 text-[var(--app-text-muted)] hover:text-[var(--app-red)] hover:bg-[var(--app-red-bg)] transition-colors shrink-0"
                title={`删除已选的 ${selectedSet.size} 项`}
              >
                <Trash2 size={13} />
              </button>
            </>
          )}

          {/* Clear selection */}
          {globalBatchHandlers.anySelected && (
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
      <div className="flex-1 overflow-y-auto overflow-x-hidden py-0.5 flex flex-col">
        {loading && (
          <div className="flex-1 flex items-center justify-center">
            <span className="text-xs text-[var(--app-text-muted)] font-mono animate-pulse">
              扫描中...
            </span>
          </div>
        )}

        {filtered && (
          <>
            {/* ── Plugins ── */}
            {filtered.plugins.length > 0 && (
              <div>
                <SectionHeader
                  label="Plugins"
                  count={filtered.plugins.length}
                  collapsed={collapsed.has("plugins")}
                  onToggle={() => toggleSection("plugins")}
                />
                {!collapsed.has("plugins") && filtered.plugins.map((p) => {
                    const pUpdate = updateInfos.find((u) => u.pluginId === p.id && u.hasUpdate);
                    return (
                  <ListRow
                    key={p.id}
                    label={p.name}
                    badge={
                      <div className="flex items-center gap-1 shrink-0">
                        {pUpdate && (
                          <span
                            className="text-2xs text-[var(--app-amber)] font-mono"
                            title={`${pUpdate.currentVersion} → ${pUpdate.latestSha}`}
                          >↑</span>
                        )}
                        <CliBadge cli={p.cli} />
                      </div>
                    }
                    enabled={p.enabled}
                    selected={selectedId === p.id}
                    checked={selectedSet.has(p.id)}
                    showCheck={showBatch}
                    onClick={(e) => handleClick(p.id, "plugins", e, () => onSelect({ type: "plugin", data: p }))}
                    onContextMenu={(e) => {
                      e.preventDefault();
                      setContextMenu({
                        x: e.clientX,
                        y: e.clientY,
                        items: [
                          {
                            label: p.enabled ? "禁用" : "启用",
                            icon: p.enabled ? <Ban size={13} /> : <Play size={13} />,
                            onClick: () => onTogglePlugin(p.cli, p.id, !p.enabled),
                          },
                          { separator: true },
                          {
                            label: "复制到项目级",
                            icon: <span>→</span>,
                            disabled: true,
                            onClick: () => {},
                          },
                          {
                            label: "移动到项目级",
                            icon: <span>→</span>,
                            disabled: true,
                            onClick: () => {},
                          },
                          { separator: true },
                          {
                            label: "删除",
                            icon: <Trash2 size={13} />,
                            danger: true,
                            onClick: () => onBatchUninstall([{ cli: p.cli, id: p.id, enabled: p.enabled }]),
                          },
                        ],
                      });
                    }}
                  />
                );
              })}
              </div>
            )}

            {/* ── Standalone Skills ── */}
            {filtered.standaloneSkills.length > 0 && (
              <div>
                <SectionHeader
                  label="Skills"
                  count={filtered.standaloneSkills.length}
                  collapsed={collapsed.has("standalone")}
                  onToggle={() => toggleSection("standalone")}
                />
                {!collapsed.has("standalone") && filtered.standaloneSkills.map((s) => (
                  <ListRow
                    key={s.id}
                    label={s.name}
                    badge={
                      <div className="flex items-center gap-1 shrink-0">
                        <span className="text-2xs text-[var(--app-text-muted)]">
                          {s.linkType === "symlink" ? "↗" : s.linkType === "directory" ? "📁" : "📄"}
                        </span>
                        {s.projectName && <ProjectBadge name={s.projectName} />}
                        <CliBadge cli={s.cli} />
                      </div>
                    }
                    enabled={s.enabled}
                    selected={selectedId === s.id}
                    checked={selectedSet.has(s.id)}
                    showCheck={showBatch}
                    onClick={(e) => handleClick(s.id, "standalone", e, () => onSelect({ type: "standalone", data: s }))}
                    onContextMenu={(e) => {
                      e.preventDefault();
                      const isProject = s.projectName !== undefined || s.id.includes(":project-");
                      const targetScope = (isProject ? "user" : "project") as "user" | "project";
                      const isTowardProject = targetScope === "project";
                      const fromLabel = isProject
                        ? `项目级 · ${effectiveActiveProject?.name ?? ""}`
                        : "用户级";
                      const noProject = isTowardProject && projects.length === 0;
                      const singleProject = isTowardProject && projects.length === 1;
                      const arrow = isProject ? "←" : "→";

                      setContextMenu({
                        x: e.clientX,
                        y: e.clientY,
                        items: [
                          {
                            label: s.enabled ? "禁用" : "启用",
                            icon: s.enabled ? <Ban size={13} /> : <Play size={13} />,
                            onClick: () => onToggleStandaloneSkill(s.cli, s.id, !s.enabled, s.path),
                          },
                          { separator: true },
                          {
                            label: isTowardProject ? "复制到项目级" : "复制到用户级",
                            icon: <span>{arrow}</span>,
                            disabled: noProject,
                            onClick: () => {
                              if (isTowardProject) {
                                if (singleProject) {
                                  onCopyResource?.({ type: "standalone", data: s }, "project", projects[0]);
                                } else {
                                  setProjectPicker({ item: { type: "standalone", data: s }, action: "copy" });
                                }
                              } else {
                                onCopyResource?.({ type: "standalone", data: s }, "user");
                              }
                            },
                          },
                          {
                            label: isTowardProject ? "移动到项目级" : "移动到用户级",
                            icon: <span>{arrow}</span>,
                            disabled: noProject,
                            onClick: () => {
                              if (isTowardProject) {
                                if (singleProject) {
                                  setMoveConfirm({
                                    item: { type: "standalone", data: s },
                                    targetScope: "project",
                                    resourceName: s.name,
                                    fromLabel,
                                    toLabel: `项目级 · ${projects[0].name}`,
                                    targetProject: projects[0],
                                  });
                                } else {
                                  setProjectPicker({ item: { type: "standalone", data: s }, action: "move" });
                                }
                              } else {
                                setMoveConfirm({
                                  item: { type: "standalone", data: s },
                                  targetScope: "user",
                                  resourceName: s.name,
                                  fromLabel,
                                  toLabel: "用户级",
                                });
                              }
                            },
                          },
                          // Cross-project copy/move (only for project-level skills)
                          ...(isProject ? [
                            {
                              label: "复制到其他项目…",
                              icon: <Folder size={13} />,
                              disabled: projects.length <= 1,
                              onClick: () => {
                                setProjectPicker({
                                  item: { type: "standalone", data: s },
                                  action: "copy",
                                  excludePath: getResourceProjectRoot(s.path),
                                });
                              },
                            },
                            {
                              label: "移动到其他项目…",
                              icon: <Folder size={13} />,
                              disabled: projects.length <= 1,
                              onClick: () => {
                                setProjectPicker({
                                  item: { type: "standalone", data: s },
                                  action: "move",
                                  excludePath: getResourceProjectRoot(s.path),
                                });
                              },
                            },
                          ] : []),
                          { separator: true },
                          {
                            label: "删除",
                            icon: <Trash2 size={13} />,
                            danger: true,
                            onClick: () => onBatchUninstall([{ cli: s.cli, id: s.id, enabled: s.enabled, path: s.path }]),
                          },
                        ],
                      });
                    }}
                  />
                ))}
              </div>
            )}

            {/* ── Agents Section ── */}
            {agentData && filteredAgents.length > 0 && (
              <div>
                <SectionHeader
                  label="Agents"
                  count={filteredAgents.length}
                  collapsed={collapsed.has("agents")}
                  onToggle={() => toggleSection("agents")}
                />
                {!collapsed.has("agents") && (
                  <>
                    <div className="px-3 py-1 flex items-center gap-2 text-2xs font-mono text-[var(--app-text-muted)]">
                      {agentSourceCounts.user > 0 && <span>👤 {agentSourceCounts.user} 独立</span>}
                      {agentSourceCounts.project > 0 && <span>📁 {agentSourceCounts.project} 项目</span>}
                      {agentSourceCounts.builtin > 0 && <span>🔒 {agentSourceCounts.builtin} 内置</span>}
                    </div>
                    {filteredAgents.map((agent) => {
                      const isBuiltin = agent.source === "builtin";
                      const agentProject = agent.source === "project" ? (agent.projectName || null) : null;
                      const sourceLabel = agent.source === "user" ? "独立"
                        : agent.source === "project" ? (agentProject || "项目")
                        : agent.source === "builtin" ? "内置"
                        : "";
                      return (
                        <ListRow
                          key={agent.id}
                          icon={isBuiltin
                            ? <Lock size={10} className="text-[var(--app-text-muted)] shrink-0" />
                            : <Bot size={14} />
                          }
                          label={agent.name}
                          badge={
                            <div className="flex items-center gap-1 shrink-0">
                              {!isBuiltin && (
                                <span className={`text-2xs px-1 py-px rounded font-mono max-w-[72px] truncate ${
                                  agent.source === "project"
                                    ? "bg-amber-500/10 text-amber-400"
                                    : "bg-[var(--app-accent)]/10 text-[var(--app-accent)]"
                                }`} title={agent.source === "project" ? (agentProject || sourceLabel) : undefined}>{sourceLabel}</span>
                              )}
                              {agent.model && (
                                <span className="text-2xs text-[var(--app-text-muted)] font-mono">model:{agent.model}</span>
                              )}
                              {agent.tools.length > 0 && (
                                <span className="text-2xs text-[var(--app-text-muted)] font-mono">tools:{agent.tools.length}</span>
                              )}
                              {agent.color && <ColorDot color={agent.color} />}
                              <CliBadge cli={agent.cli} />
                            </div>
                          }
                          enabled={isBuiltin ? true : agent.enabled}
                          readonly={isBuiltin}
                          noCheck={isBuiltin}
                          selected={selectedId === agent.id}
                          checked={isBuiltin ? false : selectedSet.has(agent.id)}
                          showCheck={isBuiltin ? false : showBatch}
                          onClick={(e) => handleAgentClick(agent, e)}
                          onContextMenu={isBuiltin ? undefined : (e) => {
                            e.preventDefault();
                            const isProjectAgent = agent.projectName !== undefined || agent.source === "project";
                            const targetScope = (isProjectAgent ? "user" : "project") as "user" | "project";
                            const isTowardProject = targetScope === "project";
                            const arrow = isProjectAgent ? "←" : "→";
                            const noProject = isTowardProject && projects.length === 0;
                            const singleProject = isTowardProject && projects.length === 1;
                            const fromLabel = isProjectAgent
                              ? `项目级 · ${effectiveActiveProject?.name ?? ""}`
                              : "用户级";

                            setContextMenu({
                              x: e.clientX,
                              y: e.clientY,
                              items: [
                                {
                                  label: agent.enabled ? "禁用" : "启用",
                                  icon: agent.enabled ? <Ban size={13} /> : <Play size={13} />,
                                  onClick: () => onBatchToggle(
                                    [{ cli: agent.cli, id: agent.id, enabled: !agent.enabled, path: agent.path }],
                                    !agent.enabled,
                                  ),
                                },
                                { separator: true },
                                {
                                  label: isTowardProject ? "复制到项目级" : "复制到用户级",
                                  icon: <span>{arrow}</span>,
                                  disabled: noProject,
                                  onClick: () => {
                                    if (isTowardProject) {
                                      if (singleProject) {
                                        onCopyResource?.({ type: "agent", data: agent }, "project", projects[0]);
                                      } else {
                                        setProjectPicker({ item: { type: "agent", data: agent }, action: "copy" });
                                      }
                                    } else {
                                      onCopyResource?.({ type: "agent", data: agent }, "user");
                                    }
                                  },
                                },
                                {
                                  label: isTowardProject ? "移动到项目级" : "移动到用户级",
                                  icon: <span>{arrow}</span>,
                                  disabled: noProject,
                                  onClick: () => {
                                    if (isTowardProject) {
                                      if (singleProject) {
                                        setMoveConfirm({
                                          item: { type: "agent", data: agent },
                                          targetScope: "project",
                                          resourceName: agent.name,
                                          fromLabel,
                                          toLabel: `项目级 · ${projects[0].name}`,
                                          targetProject: projects[0],
                                        });
                                      } else {
                                        setProjectPicker({ item: { type: "agent", data: agent }, action: "move" });
                                      }
                                    } else {
                                      setMoveConfirm({
                                        item: { type: "agent", data: agent },
                                        targetScope: "user",
                                        resourceName: agent.name,
                                        fromLabel,
                                        toLabel: "用户级",
                                      });
                                    }
                                  },
                                },
                                // Cross-project copy/move (only for project-level agents)
                                ...(isProjectAgent ? [
                                  {
                                    label: "复制到其他项目…",
                                    icon: <Folder size={13} />,
                                    disabled: projects.length <= 1,
                                    onClick: () => {
                                      setProjectPicker({
                                        item: { type: "agent", data: agent },
                                        action: "copy",
                                        excludePath: getResourceProjectRoot(agent.path),
                                      });
                                    },
                                  },
                                  {
                                    label: "移动到其他项目…",
                                    icon: <Folder size={13} />,
                                    disabled: projects.length <= 1,
                                    onClick: () => {
                                      setProjectPicker({
                                        item: { type: "agent", data: agent },
                                        action: "move",
                                        excludePath: getResourceProjectRoot(agent.path),
                                      });
                                    },
                                  },
                                ] : []),
                                { separator: true },
                                {
                                  label: "删除",
                                  icon: <Trash2 size={13} />,
                                  danger: true,
                                  onClick: () => onDeleteAgent?.(agent.cli, agent.name, agent.path),
                                },
                              ],
                            });
                          }}
                        />
                      );
                    })}
                  </>
                )}
              </div>
            )}

            {/* ── Commands Section ── */}
            {filtered.commands.length > 0 && (
              <div>
                <SectionHeader
                  label="Commands"
                  count={filtered.commands.length}
                  collapsed={collapsed.has("commands")}
                  onToggle={() => toggleSection("commands")}
                />
                {!collapsed.has("commands") && filtered.commands.map((cmd) => (
                  <ListRow
                    key={cmd.id}
                    icon={<Terminal size={12} />}
                    label={`/${cmd.name}`}
                    badge={
                      <div className="flex items-center gap-1 shrink-0">
                        {cmd.projectName && <ProjectBadge name={cmd.projectName} />}
                        <CliBadge cli={cmd.cli} />
                      </div>
                    }
                    enabled={cmd.enabled}
                    selected={selectedId === cmd.id}
                    checked={selectedSet.has(cmd.id)}
                    showCheck={showBatch}
                    onClick={(e) => handleClick(cmd.id, "commands", e, () => onSelect({ type: "command", data: cmd }))}
                    onContextMenu={(e) => {
                      e.preventDefault();
                      const isProjectCmd = cmd.projectName !== undefined || cmd.id.includes(":project-");
                      const targetScope = (isProjectCmd ? "user" : "project") as "user" | "project";
                      const isTowardProject = targetScope === "project";
                      const arrow = isProjectCmd ? "←" : "→";
                      const noProject = isTowardProject && projects.length === 0;
                      const singleProject = isTowardProject && projects.length === 1;
                      const fromLabel = isProjectCmd
                        ? `项目级 · ${effectiveActiveProject?.name ?? ""}`
                        : "用户级";

                      setContextMenu({
                        x: e.clientX,
                        y: e.clientY,
                        items: [
                          {
                            label: cmd.enabled ? "禁用" : "启用",
                            icon: cmd.enabled ? <Ban size={13} /> : <Play size={13} />,
                            onClick: () => onBatchToggle(
                              [{ cli: cmd.cli, id: cmd.id, enabled: !cmd.enabled, path: cmd.path }],
                              !cmd.enabled,
                            ),
                          },
                          { separator: true },
                          {
                            label: isTowardProject ? "复制到项目级" : "复制到用户级",
                            icon: <span>{arrow}</span>,
                            disabled: noProject,
                            onClick: () => {
                              if (isTowardProject) {
                                if (singleProject) {
                                  onCopyResource?.({ type: "command", data: cmd }, "project", projects[0]);
                                } else {
                                  setProjectPicker({ item: { type: "command", data: cmd }, action: "copy" });
                                }
                              } else {
                                onCopyResource?.({ type: "command", data: cmd }, "user");
                              }
                            },
                          },
                          {
                            label: isTowardProject ? "移动到项目级" : "移动到用户级",
                            icon: <span>{arrow}</span>,
                            disabled: noProject,
                            onClick: () => {
                              if (isTowardProject) {
                                if (singleProject) {
                                  setMoveConfirm({
                                    item: { type: "command", data: cmd },
                                    targetScope: "project",
                                    resourceName: cmd.name,
                                    fromLabel,
                                    toLabel: `项目级 · ${projects[0].name}`,
                                    targetProject: projects[0],
                                  });
                                } else {
                                  setProjectPicker({ item: { type: "command", data: cmd }, action: "move" });
                                }
                              } else {
                                setMoveConfirm({
                                  item: { type: "command", data: cmd },
                                  targetScope: "user",
                                  resourceName: cmd.name,
                                  fromLabel,
                                  toLabel: "用户级",
                                });
                              }
                            },
                          },
                          // Cross-project copy/move (only for project-level commands)
                          ...(isProjectCmd ? [
                            {
                              label: "复制到其他项目…",
                              icon: <Folder size={13} />,
                              disabled: projects.length <= 1,
                              onClick: () => {
                                setProjectPicker({
                                  item: { type: "command", data: cmd },
                                  action: "copy",
                                  excludePath: getResourceProjectRoot(cmd.path),
                                });
                              },
                            },
                            {
                              label: "移动到其他项目…",
                              icon: <Folder size={13} />,
                              disabled: projects.length <= 1,
                              onClick: () => {
                                setProjectPicker({
                                  item: { type: "command", data: cmd },
                                  action: "move",
                                  excludePath: getResourceProjectRoot(cmd.path),
                                });
                              },
                            },
                          ] : []),
                          { separator: true },
                          {
                            label: "删除",
                            icon: <Trash2 size={13} />,
                            danger: true,
                            onClick: () => onBatchUninstall([{ cli: cmd.cli, id: cmd.id, enabled: cmd.enabled, path: cmd.path }]),
                          },
                        ],
                      });
                    }}
                  />
                ))}
              </div>
            )}

            {/* ── System Skills ── (no batch, readonly) */}
            {filtered.systemSkills.length > 0 && (
              <div>
                <div className="flex items-center gap-1.5 px-3 pt-2.5 pb-1 text-left">
                  <ChevronRight
                    size={10}
                    className={`shrink-0 text-[var(--app-text-muted)] transition-transform duration-200 cursor-pointer
                      ${collapsed.has("system") ? "" : "rotate-90"}`}
                    onClick={() => toggleSection("system")}
                  />
                  <span
                    className="text-2xs text-[var(--app-text-muted)] uppercase tracking-[0.2em] font-mono flex-1 cursor-pointer"
                    onClick={() => toggleSection("system")}
                  >
                    System
                  </span>
                  <span className="text-2xs text-[var(--app-text-muted)] font-mono tabular-nums">
                    {filtered.systemSkills.length}
                  </span>
                </div>
                {!collapsed.has("system") && filtered.systemSkills.map((s) => (
                  <ListRow
                    key={s.id}
                    icon={<Lock size={10} className="text-[var(--app-text-muted)] shrink-0" />}
                    label={s.name}
                    badge={<CliBadge cli={s.cli} />}
                    enabled={true}
                    readonly
                    noCheck
                    selected={selectedId === s.id}
                    checked={false}
                    showCheck={false}
                    onClick={(e) => handleClick(s.id, "system", e, () => onSelect({ type: "system", data: s }))}
                  />
                ))}
              </div>
            )}

            {/* Empty */}
            {filtered.plugins.length === 0 &&
             filtered.standaloneSkills.length === 0 &&
             filtered.systemSkills.length === 0 &&
             filteredAgents.length === 0 && (
              <div className="flex-1 flex flex-col items-center justify-center gap-2">
                <Filter size={14} className="text-[var(--app-text-muted)] opacity-25" />
                <span className="text-xs text-[var(--app-text-muted)] font-mono">无匹配项</span>
              </div>
            )}
          </>
        )}
      </div>

      {/* Selection status bar — subtle, VS Code-style */}
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
        <div className="shrink-0 border-t border-[var(--app-border)] px-3 py-1.5 bg-[var(--app-statusbar)] flex items-center justify-between">
          <span className="text-2xs text-[var(--app-text-muted)] font-mono">
            {data.plugins.length} plugins · {data.standaloneSkills.length} skills
          </span>
          <span className="text-2xs text-[var(--app-text-dim)] font-mono" title="Plugin 暂不支持复制和移动操作">
            ⚠️ Plugin 不支持移动/复制
          </span>
        </div>
      )}

      {/* Batch uninstall confirmation */}
      <ConfirmDialog
        open={batchUninstallConfirm !== null}
        title="批量删除"
        message={`确定要删除已选的 ${batchUninstallConfirm?.length || 0} 个 Plugin / Skill 吗？此操作不可撤销。`}
        confirmLabel="删除"
        onConfirm={() => {
          if (batchUninstallConfirm) {
            onBatchUninstall(batchUninstallConfirm);
            setBatchUninstallConfirm(null);
            setSelectedSet(new Set());
          }
        }}
        onCancel={() => setBatchUninstallConfirm(null)}
      />

      {/* Project picker dialog — shown when multiple projects exist */}
      <Dialog
        open={projectPicker !== null}
        onClose={() => setProjectPicker(null)}
        width="340px"
        persistent
      >
        {/* Header */}
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

        {/* Project list */}
        <div className="max-h-[50vh] overflow-y-auto py-1">
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
                const item = projectPicker!.item;
                const isProject = (item.data as any).projectName !== undefined || (item.data as any).id?.includes(":project-");
                const fromLabel = isProject
                  ? `项目级 · ${effectiveActiveProject?.name ?? ""}`
                  : "用户级";
                if (projectPicker!.action === "copy") {
                  onCopyResource?.(item, "project", p);
                } else {
                  setMoveConfirm({
                    item,
                    targetScope: "project",
                    resourceName: (item.data as any).name,
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

        {/* Footer — register new project */}
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
        title={`移动 ${moveConfirm?.resourceName ?? ""}`}
        message={`此操作将从 ${moveConfirm?.fromLabel ?? ""} 移除 "${moveConfirm?.resourceName ?? ""}"，并添加到 ${moveConfirm?.toLabel ?? ""}。${moveConfirm?.targetScope === "project" ? "\n\n项目级资源可提交到 Git 与团队共享。" : ""}`}
        confirmLabel="确认移动"
        variant="primary"
        onConfirm={() => {
          if (moveConfirm) {
            onMoveResource?.(moveConfirm.item, moveConfirm.targetScope, moveConfirm.targetProject);
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
          已选 {batchPicker?.items.length ?? 0} 项资源
        </div>
        <div className="py-1 max-h-[300px] overflow-y-auto">
          {projects.map((p) => (
            <button
              key={p.name}
              onClick={() => {
                if (batchPicker) {
                  setBatchMoveConfirm({
                    items: batchPicker.items,
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
        message={`确定要将已选的 ${batchMoveConfirm?.items.length ?? 0} 项资源${batchMoveConfirm?.action === "copy" ? "复制" : "移动"}到 ${batchMoveConfirm?.toScope === "project" ? `项目级 · ${batchMoveConfirm?.targetProject?.name ?? ""}` : "用户级"} 吗？\n\n${batchMoveConfirm?.action === "move" ? "移动后原位置资源将被删除，可通过撤销恢复。" : ""}`}
        confirmLabel={`确认${batchMoveConfirm?.action === "copy" ? "复制" : "移动"}`}
        variant="primary"
        onConfirm={() => {
          if (batchMoveConfirm) {
            if (batchMoveConfirm.action === "copy") {
              onBatchCopy?.(batchMoveConfirm.items, batchMoveConfirm.toScope, batchMoveConfirm.targetProject);
            } else {
              onBatchMove?.(batchMoveConfirm.items, batchMoveConfirm.toScope, batchMoveConfirm.targetProject);
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
