import React, { useState, useEffect, useMemo, useCallback, useRef } from "react";
import { SearchInput } from "./common/SearchInput";
import { CLIIcon } from "./common/CLIIcon";
import { ContextMenu } from "./ContextMenu";
import { Circle, Hash, ArrowUpDown, Copy, Pencil, Trash2, Star, Tag, ChevronDown, Download, CheckSquare, Square } from "lucide-react";
import type { ProfileSummary } from "../lib/types";

interface SidebarProps {
  profiles: ProfileSummary[];
  selectedName: string | null;
  searchQuery: string;
  onSelect: (name: string) => void;
  onSearch: (query: string) => void;
  onCopy: (name: string) => void;
  onRename: (name: string) => void;
  onDelete: (name: string) => void;
  onSetDefault: (name: string) => void;
  usageCounts?: Record<string, number>;
  onBatchDelete?: (names: string[]) => void;
  onBatchExport?: (names: string[]) => void;
}

/* ── Sidebar ────────────────────────────────────────────── */
export function Sidebar({ profiles, selectedName, searchQuery, onSelect, onSearch, onCopy, onRename, onDelete, onSetDefault, usageCounts, onBatchDelete, onBatchExport }: SidebarProps) {
  // Tag filter
  const [activeTag, setActiveTag] = useState<string | null>(null);
  const allTags = useMemo(() => {
    const set = new Set<string>();
    profiles.forEach((p) => p.tags?.forEach((t) => set.add(t)));
    return Array.from(set).sort();
  }, [profiles]);

  // Sort
  type SortKey = "name" | "type" | "count";
  const [sortKey, setSortKey] = useState<SortKey>("name");
  const sortLabels: Record<SortKey, string> = { name: "名称", type: "类型", count: "数量" };

  const sortedProfiles = useMemo(() => {
    let list = [...profiles];
    if (activeTag) list = list.filter((p) => p.tags?.includes(activeTag));
    if (sortKey === "name") list.sort((a, b) => a.name.localeCompare(b.name));
    else if (sortKey === "type") list.sort((a, b) => (a.cli_type || "").localeCompare(b.cli_type || "") || a.name.localeCompare(b.name));
    else if (sortKey === "count") list.sort((a, b) => b.env_count - a.env_count || a.name.localeCompare(b.name));
    return list;
  }, [profiles, sortKey, activeTag]);

  // Multi-select
  const [selectedSet, setSelectedSet] = useState<Set<string>>(new Set());
  const lastClickedRef = useRef<number>(-1);

  // Clear multi-select when profiles change (e.g. after delete)
  useEffect(() => { setSelectedSet(new Set()); }, [profiles.length]);

  const toggleSelect = useCallback((name: string, index: number, shiftKey: boolean, metaKey: boolean) => {
    if (shiftKey && lastClickedRef.current >= 0) {
      // Range select
      const start = Math.min(lastClickedRef.current, index);
      const end = Math.max(lastClickedRef.current, index);
      const rangeNames = sortedProfiles.slice(start, end + 1).map((p) => p.name);
      setSelectedSet((prev) => {
        const next = new Set(prev);
        rangeNames.forEach((n) => next.add(n));
        return next;
      });
    } else if (metaKey) {
      // Toggle single
      setSelectedSet((prev) => {
        const next = new Set(prev);
        if (next.has(name)) next.delete(name); else next.add(name);
        return next;
      });
      lastClickedRef.current = index;
    } else {
      // Normal click — single select, clear batch
      setSelectedSet(new Set());
      lastClickedRef.current = index;
      onSelect(name);
    }
  }, [sortedProfiles, onSelect]);

  const handleClick = useCallback((name: string, index: number, e: React.MouseEvent) => {
    const isCheckbox = (e.target as HTMLElement).closest("[data-checkbox]");
    if (isCheckbox) {
      // Checkbox click always toggles (no modifier needed)
      e.preventDefault();
      toggleSelect(name, index, e.shiftKey, true);
    } else if (e.metaKey || e.ctrlKey || e.shiftKey) {
      e.preventDefault();
      toggleSelect(name, index, e.shiftKey, e.metaKey || e.ctrlKey);
    } else {
      setSelectedSet(new Set());
      onSelect(name);
    }
  }, [toggleSelect, onSelect]);

  const clearSelection = () => setSelectedSet(new Set());

  // Context menu
  const [ctxMenu, setCtxMenu] = useState<{ x: number; y: number; name: string } | null>(null);
  const onContextMenu = useCallback((e: React.MouseEvent, name: string) => {
    e.preventDefault();
    setCtxMenu({ x: e.clientX, y: e.clientY, name });
  }, []);

  // Track pending selection
  const [pendingName, setPendingName] = useState<string | null>(null);
  useEffect(() => { setPendingName(null); }, [selectedName]);

  const batchCount = selectedSet.size;
  const showBatch = batchCount > 0;

  return (
    <div className="w-[230px] shrink-0 flex flex-col bg-app-sidebar border-r border-app-border select-none">
      <div className="px-2 pt-2 pb-1.5">
        <SearchInput value={searchQuery} onChange={onSearch} placeholder="搜索 profile..." />
      </div>
      <div className="mx-2 border-b border-app-border-light" />

      {/* Tag filter dropdown */}
      {allTags.length > 0 && <TagFilter tags={allTags} active={activeTag} onChange={setActiveTag} />}

      <div className="flex items-center justify-between px-3 pt-2.5 pb-1.5">
        <span className="text-2xs text-app-text-muted uppercase tracking-[0.2em]">Profiles</span>
        <div className="flex items-center gap-1">
          <button
            onClick={() => {
              const keys: SortKey[] = ["name", "type", "count"];
              const idx = keys.indexOf(sortKey);
              setSortKey(keys[(idx + 1) % keys.length]);
            }}
            className="flex items-center gap-0.5 text-2xs text-app-text-muted hover:text-app-text transition-colors font-mono"
            title={`排序: ${sortLabels[sortKey]}`}
          >
            <ArrowUpDown size={9} />
            {sortLabels[sortKey]}
          </button>
          <span className="text-2xs text-app-text-muted tabular-nums">[{profiles.length}]</span>
        </div>
      </div>
      <div className="flex-1 overflow-y-auto overflow-x-hidden py-0.5">
        {profiles.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full gap-3 px-4 text-center">
            <Hash size={22} className="text-app-text-muted opacity-25" />
            <div className="text-xs text-app-text-dim">暂无 profile</div>
            <div className="text-2xs text-app-text-muted leading-relaxed">
              按 <kbd className="text-app-amber">⌘N</kbd> 新建
            </div>
          </div>
        ) : (
          sortedProfiles.map((p, idx) => {
            const isSelected = p.name === selectedName;
            const isPending = p.name === pendingName;
            const isChecked = selectedSet.has(p.name);
            return (
              <div
                key={p.name}
                onClick={(e) => handleClick(p.name, idx, e)}
                onContextMenu={(e) => onContextMenu(e, p.name)}
                className={`group flex items-center gap-2 mx-1 my-px px-2.5 py-1.5 cursor-pointer
                  transition-all duration-fast
                  ${isSelected || isPending
                    ? "bg-app-selected text-app-text border-l-[3px] border-l-app-accent shadow-[inset_0_0_8px_var(--app-glow)]"
                    : isChecked
                      ? "bg-app-hover text-app-text border-l-[3px] border-l-app-amber"
                      : "text-app-text border-l-[3px] border-l-transparent hover:bg-app-hover active:bg-app-active"
                  }`}
              >
                {/* Checkbox — visible on hover or when checked */}
                <span
                  data-checkbox
                  className={`shrink-0 cursor-pointer transition-opacity duration-fast
                    ${isChecked || showBatch ? "opacity-100" : "opacity-0 group-hover:opacity-100"}`}
                >
                  {isChecked
                    ? <CheckSquare size={13} className="text-app-amber" />
                    : <Square size={13} className="text-app-text-muted" />
                  }
                </span>
                {/* Default indicator */}
                <Circle
                  size={7}
                  className={`shrink-0 transition-colors duration-fast ${
                    p.is_default
                      ? "fill-app-accent text-app-accent shadow-[0_0_6px_var(--app-glow)]"
                      : "fill-transparent text-transparent group-hover:text-app-text-muted"
                  }`}
                />
                {/* CLI type icon */}
                {p.cli_type && <CLIIcon type={p.cli_type} size={16} />}
                {/* Name */}
                <span className={`truncate text-sm font-mono ${isSelected ? "font-medium" : "font-normal"}`}>
                  {p.name}
                </span>
                {/* Tags */}
                {p.tags && p.tags.length > 0 && (
                  <span className="hidden group-hover:flex items-center gap-0.5 shrink-0">
                    {p.tags.slice(0, 2).map((t) => (
                      <span key={t} className="text-2xs px-1 py-px bg-[var(--app-input)] text-app-text-muted font-mono leading-none">{t}</span>
                    ))}
                    {p.tags.length > 2 && <span className="text-2xs text-app-text-muted">+{p.tags.length - 2}</span>}
                  </span>
                )}
                {/* Env count */}
                <span className={`text-2xs px-1.5 py-0.5 font-mono tabular-nums transition-colors duration-fast
                  ${isSelected
                    ? "bg-app-green-bg text-app-accent"
                    : "bg-[var(--app-input)] text-app-text-muted group-hover:bg-[var(--app-hover)] group-hover:text-app-text-dim"
                  }`}>
                  {p.env_count}
                </span>
                {/* Usage count */}
                {usageCounts?.[p.name] ? (
                  <span className={`text-2xs px-1.5 py-0.5 font-mono tabular-nums transition-colors duration-fast
                    ${isSelected
                      ? "bg-app-amber-bg text-app-amber"
                      : "bg-[var(--app-input)] text-app-text-muted group-hover:bg-[var(--app-hover)] group-hover:text-app-amber"
                    }`}
                    title={`已使用 ${usageCounts[p.name]} 次`}
                  >
                    {usageCounts[p.name]}
                  </span>
                ) : null}
              </div>
            );
          })
        )}
      </div>

      {/* Batch action bar */}
      {showBatch && onBatchDelete && onBatchExport && (
        <div className="shrink-0 border-t border-app-border bg-app-panel px-2 py-2 space-y-1.5">
          <div className="flex items-center justify-between">
            <span className="text-2xs text-app-text-dim font-mono">已选 {batchCount} 项</span>
            <button onClick={clearSelection} className="text-2xs text-app-text-muted hover:text-app-text font-mono">取消</button>
          </div>
          <div className="flex gap-1">
            <button
              onClick={() => { onBatchExport(Array.from(selectedSet)); clearSelection(); }}
              className="flex-1 flex items-center justify-center gap-1 px-2 py-1 text-2xs font-mono
                text-app-text-dim hover:text-app-text border border-app-border hover:border-app-accent
                bg-[var(--app-input)] hover:bg-[var(--app-hover)] transition-colors"
            >
              <Download size={10} />导出
            </button>
            <button
              onClick={() => onBatchDelete(Array.from(selectedSet))}
              className="flex-1 flex items-center justify-center gap-1 px-2 py-1 text-2xs font-mono
                text-app-red hover:bg-app-red-bg border border-app-border hover:border-app-red
                bg-[var(--app-input)] transition-colors"
            >
              <Trash2 size={10} />删除
            </button>
          </div>
        </div>
      )}

      {/* Context menu */}
      {ctxMenu && (
        <ContextMenu
          x={ctxMenu.x} y={ctxMenu.y}
          onClose={() => setCtxMenu(null)}
          items={[
            { label: "复制", icon: <Copy size={13} />, onClick: () => onCopy(ctxMenu.name) },
            { label: "重命名", icon: <Pencil size={13} />, onClick: () => onRename(ctxMenu.name) },
            { label: "设为默认", icon: <Star size={13} />, onClick: () => onSetDefault(ctxMenu.name), disabled: ctxMenu.name === selectedName },
            { label: "删除", icon: <Trash2 size={13} />, onClick: () => onDelete(ctxMenu.name), danger: true },
          ]}
        />
      )}
    </div>
  );
}

/* ── Tag filter dropdown ────────────────────────────────── */
function TagFilter({ tags, active, onChange }: { tags: string[]; active: string | null; onChange: (t: string | null) => void }) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const h = (e: MouseEvent) => { if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false); };
    document.addEventListener("mousedown", h);
    return () => document.removeEventListener("mousedown", h);
  }, [open]);

  return (
    <div ref={ref} className="relative px-2 pt-1.5 pb-1">
      <button
        onClick={() => setOpen(!open)}
        className="flex items-center gap-1 w-full px-2 py-1 text-2xs font-mono border border-app-border bg-[var(--app-input)]
          text-app-text-dim hover:text-app-text transition-colors"
      >
        <Tag size={9} className="shrink-0" />
        <span className="flex-1 text-left truncate">{active || "全部标签"}</span>
        {active && (
          <span
            onClick={(e) => { e.stopPropagation(); onChange(null); }}
            className="text-app-text-muted hover:text-app-red shrink-0"
          >✕</span>
        )}
        <ChevronDown size={9} className={`shrink-0 transition-transform ${open ? "rotate-180" : ""}`} />
      </button>
      {open && (
        <div className="absolute top-full left-2 right-2 z-50 bg-app-panel border border-app-border shadow-dialog py-0.5 max-h-[200px] overflow-y-auto">
          <button
            onClick={() => { onChange(null); setOpen(false); }}
            className={`w-full text-left px-3 py-1 text-2xs font-mono transition-colors ${!active ? "bg-app-accent text-[var(--app-bg)]" : "text-app-text-dim hover:bg-[var(--app-hover)]"}`}
          >全部 ({tags.length})</button>
          {tags.map((t) => (
            <button
              key={t}
              onClick={() => { onChange(active === t ? null : t); setOpen(false); }}
              className={`w-full text-left px-3 py-1 text-2xs font-mono transition-colors ${active === t ? "bg-app-accent text-[var(--app-bg)]" : "text-app-text-dim hover:bg-[var(--app-hover)]"}`}
            >{t}</button>
          ))}
        </div>
      )}
    </div>
  );
}
