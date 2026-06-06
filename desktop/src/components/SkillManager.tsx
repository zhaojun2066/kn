import React, { useState, useMemo, useRef, useEffect, useCallback } from "react";
import { SearchInput } from "./common/SearchInput";
import { ConfirmDialog } from "./ConfirmDialog";
import {
  ChevronRight, ChevronDown, Circle, Filter, Puzzle,
  FileText, Lock, CheckSquare, Square, Play, Ban, X,
  RefreshCw, Ellipsis, Trash2, Bot,
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
}

export interface StandaloneSkill {
  id: string;
  cli: CliKind;
  name: string;
  enabled: boolean;
  linkType: "symlink" | "file" | "directory";
  path: string;
}

export interface SkillManagerData {
  plugins: PluginEntry[];
  standaloneSkills: StandaloneSkill[];
  systemSkills: StandaloneSkill[];
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
}

export interface AgentManagerData {
  agents: AgentEntry[];
}

export type SelectedItem =
  | { type: "plugin"; data: PluginEntry }
  | { type: "standalone"; data: StandaloneSkill }
  | { type: "system"; data: StandaloneSkill }
  | { type: "agent"; data: AgentEntry };

export interface BatchToggleItem {
  cli: CliKind;
  id: string;
  enabled: boolean;
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
  onToggleStandaloneSkill: (cli: CliKind, skillId: string, enabled: boolean) => void;
  onBatchToggle: (items: BatchToggleItem[], enabled: boolean) => void;
  onBatchUninstall: (items: BatchToggleItem[]) => void;
  checkingUpdates: boolean;
  updateInfos: PluginUpdateInfo[];
  onCheckUpdates: () => void;
  onCancelCheckUpdates: () => void;
  onOpenMarketplace: () => void;
}

/* ──────────────────── Helpers ──────────────────── */

const CLI_LABEL: Record<CliKind, string> = { claude: "Claude", codex: "Codex", qoder: "Qoder" };
const CLI_COLOR: Record<CliKind, string> = { claude: "var(--app-accent)", codex: "var(--app-blue)", qoder: "var(--app-purple)" };
const CLI_OPTIONS: { value: string; label: string }[] = [
  { value: "all", label: "全部 CLI" },
  { value: "claude", label: "Claude" },
  { value: "codex", label: "Codex" },
  { value: "qoder", label: "Qoder" },
];

function CliBadge({ cli }: { cli: CliKind }) {
  return (
    <span
      className="text-2xs font-mono px-1 py-0 leading-none border whitespace-nowrap shrink-0"
      style={{ color: CLI_COLOR[cli], borderColor: CLI_COLOR[cli], opacity: 0.75 }}
    >
      {CLI_LABEL[cli]}
    </span>
  );
}

/* ──────────────────── DropFilter ──────────────────── */

function DropFilter({
  active,
  options,
  onChange,
}: {
  active: string;
  options: { value: string; label: string }[];
  onChange: (value: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const h = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", h);
    return () => document.removeEventListener("mousedown", h);
  }, [open]);

  const activeLabel = options.find((o) => o.value === active)?.label || active;

  return (
    <div ref={ref} className="relative">
      <button
        onClick={() => setOpen(!open)}
        className="flex items-center gap-1 w-full px-2 py-1 text-2xs font-mono
          border border-[var(--app-border)] bg-[var(--app-input)]
          text-[var(--app-text-dim)] hover:text-[var(--app-text)] transition-colors"
      >
        <Filter size={9} className="shrink-0" />
        <span className="flex-1 text-left truncate">{activeLabel}</span>
        {active !== "all" && (
          <span
            onClick={(e) => { e.stopPropagation(); onChange("all"); }}
            className="text-[var(--app-text-muted)] hover:text-[var(--app-red)] shrink-0"
          >✕</span>
        )}
        <ChevronDown size={9} className={`shrink-0 transition-transform ${open ? "rotate-180" : ""}`} />
      </button>
      {open && (
        <div className="absolute top-full left-0 right-0 z-50 bg-[var(--app-panel)] border border-[var(--app-border)] shadow-dialog py-0.5 max-h-[200px] overflow-y-auto">
          {options.map((opt) => (
            <button
              key={opt.value}
              onClick={() => { onChange(opt.value); setOpen(false); }}
              className={`w-full text-left px-3 py-1 text-2xs font-mono transition-colors ${
                active === opt.value
                  ? "bg-[var(--app-accent)] text-[var(--app-bg)]"
                  : "text-[var(--app-text-dim)] hover:bg-[var(--app-hover)]"
              }`}
            >
              {opt.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

/* ──────────────────── Collapsible Section ──────────────────── */

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
        <span className="text-2xs text-[var(--app-text-muted)] uppercase tracking-[0.2em] font-mono flex-1">
          {label}
        </span>
        <span className="text-2xs text-[var(--app-text-muted)] font-mono tabular-nums">
          {count}
        </span>
      </button>
    </div>
  );
}

/* ──────────────────── List Row ──────────────────── */

function ListRow({
  icon,
  label,
  badge,
  enabled,
  selected,
  checked,
  showCheck,
  noCheck,
  indent,
  readonly,
  onClick,
}: {
  icon?: React.ReactNode;
  label: string;
  badge?: React.ReactNode;
  enabled: boolean;
  selected: boolean;
  checked: boolean;
  showCheck: boolean;
  noCheck?: boolean;
  indent?: boolean;
  readonly?: boolean;
  onClick: (e: React.MouseEvent) => void;
}) {
  return (
    <div
      onClick={onClick}
      className={`flex items-center gap-2 h-8 cursor-pointer select-none
        transition-all duration-100 ease-out group
        ${selected
          ? "bg-[var(--app-selected)] border-l-[3px] border-l-[var(--app-accent)] text-[var(--app-text)]"
          : checked
            ? "bg-[var(--app-hover)] border-l-[3px] border-l-[var(--app-amber)] text-[var(--app-text)]"
            : "border-l-[3px] border-l-transparent text-[var(--app-text-dim)] hover:bg-[var(--app-hover)] hover:text-[var(--app-text)]"
        }
        ${indent ? "pl-10" : "pl-3"}
        pr-2`}
      style={selected ? { boxShadow: "inset 0 0 8px var(--app-glow)" } : undefined}
    >
      {/* Checkbox — hidden for system/readonly rows */}
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

      {/* Status dot or icon */}
      {icon || (
        <Circle
          size={5}
          className={`shrink-0 ${
            readonly
              ? "fill-[var(--app-text-muted)] text-[var(--app-text-muted)]"
              : enabled
                ? "fill-[var(--app-accent)] text-[var(--app-accent)]"
                : "fill-[var(--app-text-muted)] text-[var(--app-text-muted)] opacity-50"
          }`}
          style={!readonly && enabled ? { boxShadow: "0 0 4px var(--app-glow)" } : undefined}
        />
      )}

      <span className={`flex-1 text-xs font-mono truncate ${!enabled && !readonly ? "opacity-60" : ""}`}>
        {label}
      </span>
      {badge}
    </div>
  );
}

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
  checkingUpdates,
  updateInfos,
  onCheckUpdates,
  onCancelCheckUpdates,
  onOpenMarketplace,
}: SkillManagerProps) {
  const [search, setSearch] = useState("");
  const [cliFilter, setCliFilter] = useState("all");
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [selectedSet, setSelectedSet] = useState<Set<string>>(new Set());
  const [toolbarExpanded, setToolbarExpanded] = useState(false);
  const [batchUninstallConfirm, setBatchUninstallConfirm] = useState<BatchToggleItem[] | null>(null);
  const lastClickedRef = useRef<{ id: string; section: string } | null>(null);

  const toggleSection = (key: string) => {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };

  const filtered = useMemo(() => {
    if (!data) return null;
    const q = search.toLowerCase();
    const filterBySearch = (name: string) => !q || name.toLowerCase().includes(q);
    const filterByCli = (cli: CliKind) => cliFilter === "all" || cli === cliFilter;

    return {
      plugins: data.plugins.filter(
        (p) => filterByCli(p.cli) && (filterBySearch(p.name) || p.skills.some((s) => filterBySearch(s.name)))
      ),
      standaloneSkills: data.standaloneSkills.filter(
        (s) => filterByCli(s.cli) && filterBySearch(s.name)
      ),
      systemSkills: data.systemSkills.filter(
        (s) => filterByCli(s.cli) && filterBySearch(s.name)
      ),
    };
  }, [data, search, cliFilter]);

  // Filter agents based on search and CLI filter
  const filteredAgents = useMemo(() => {
    if (!agentData) return [];
    const q = search.toLowerCase();
    return agentData.agents.filter((a) => {
      if (cliFilter !== "all" && a.cli !== cliFilter) return false;
      if (q && !a.name.toLowerCase().includes(q) &&
          !a.description.toLowerCase().includes(q)) return false;
      return true;
    });
  }, [agentData, search, cliFilter]);

  // Reset batch selection when data/filter changes
  useEffect(() => { setSelectedSet(new Set()); }, [data, search, cliFilter]);

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
        if (skill) items.push({ cli: skill.cli, id: skill.id, enabled: true });
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
        if (skill) items.push({ cli: skill.cli, id: skill.id, enabled: false });
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
        if (skill) items.push({ cli: skill.cli, id: skill.id, enabled: skill.enabled });
      });
      if (items.length > 0) { setBatchUninstallConfirm(items); }
    };
    return { selectAll, allSelected, anySelected, enableAll, disableAll, uninstallAll };
  }, [filtered, selectedSet, onBatchToggle, onBatchUninstall]);

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

        <div className="flex flex-col gap-1.5">
          <SearchInput value={search} onChange={setSearch} placeholder="搜索..." />
          <DropFilter
            active={cliFilter}
            options={CLI_OPTIONS}
            onChange={setCliFilter}
          />
        </div>
      </div>

      {/* Batch operations toolbar — below filters, ExpandableToolbar pattern */}
      <div className="flex items-center gap-1 px-2.5 pb-2">
        {/* Marketplace — always visible */}
        <button
          onClick={onOpenMarketplace}
          className="p-0.5 text-[var(--app-text-muted)] hover:text-[var(--app-accent)] hover:bg-[var(--app-hover)] transition-colors shrink-0"
          title="浏览 Marketplace"
        >
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <circle cx="12" cy="12" r="10"/><path d="M2 12h20"/><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"/>
          </svg>
        </button>

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
            maxWidth: toolbarExpanded ? "250px" : "0px",
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

          {/* Batch uninstall */}
          {globalBatchHandlers.anySelected && (
            <>
              <div className="h-4 border-l border-[var(--app-border)] mx-0.5 shrink-0" />
              <button
                onClick={globalBatchHandlers.uninstallAll}
                className="p-0.5 text-[var(--app-text-muted)] hover:text-[var(--app-red)] hover:bg-[var(--app-red-bg)] transition-colors shrink-0"
                title={`卸载已选的 ${selectedSet.size} 项`}
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
      <div className="flex-1 overflow-y-auto overflow-x-hidden py-0.5">
        {loading && (
          <div className="flex items-center justify-center py-10">
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
                        <CliBadge cli={s.cli} />
                      </div>
                    }
                    enabled={s.enabled}
                    selected={selectedId === s.id}
                    checked={selectedSet.has(s.id)}
                    showCheck={showBatch}
                    onClick={(e) => handleClick(s.id, "standalone", e, () => onSelect({ type: "standalone", data: s }))}
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
                {!collapsed.has("agents") && filteredAgents.map((agent) => {
                  const isBuiltin = agent.source === "builtin";
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
                    />
                  );
                })}
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
              <div className="flex flex-col items-center justify-center py-12 gap-2">
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
        <div className="shrink-0 border-t border-[var(--app-border)] px-3 py-1.5 bg-[var(--app-statusbar)]">
          <span className="text-2xs text-[var(--app-text-muted)] font-mono">
            {data.plugins.length} plugins · {data.standaloneSkills.length} skills
          </span>
        </div>
      )}

      {/* Batch uninstall confirmation */}
      <ConfirmDialog
        open={batchUninstallConfirm !== null}
        title="批量卸载"
        message={`确定要卸载已选的 ${batchUninstallConfirm?.length || 0} 个 Plugin / Skill 吗？此操作不可撤销。`}
        confirmLabel="卸载"
        onConfirm={() => {
          if (batchUninstallConfirm) {
            onBatchUninstall(batchUninstallConfirm);
            setBatchUninstallConfirm(null);
            setSelectedSet(new Set());
          }
        }}
        onCancel={() => setBatchUninstallConfirm(null)}
      />
    </div>
  );
}
