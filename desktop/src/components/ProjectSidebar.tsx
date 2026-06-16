import { relativeTime, relativeTimeShort } from "../lib/time-utils";
import React, { useState, useCallback, useRef, useEffect, useMemo } from "react";
import { createPortal } from "react-dom";
import { SearchInput } from "./common/SearchInput";
import { ContextMenu } from "./ContextMenu";
import { Folder, Trash2, Play, FolderOpen, Plus, Star, Pin, Pencil, ArrowUpDown, ExternalLink, Terminal } from "lucide-react";
import { Button } from "./common/Button";
import { Dialog } from "./common/Dialog";
import type { ProjectInfo, ProfileSummary, ProjectStats } from "../lib/types";

interface ProjectSidebarProps {
  projects: ProjectInfo[];
  selectedProject: ProjectInfo | null;
  onSelect: (project: ProjectInfo | null) => void;
  onAddProject: () => void;
  onDeleteProject: (name: string) => void;
  onRunProfile: (projectPath: string, projectName: string, profileName: string, cliType: string) => void;
  onSetDescription: (name: string, description: string) => void;
  onTogglePin: (name: string, pinned: boolean) => void;
  onOpenInEditor: (path: string, editor: string) => void;
  profiles: ProfileSummary[];
  statsMap: Record<string, ProjectStats>;
}

export function ProjectSidebar({
  projects,
  selectedProject,
  onSelect,
  onAddProject,
  onDeleteProject,
  onRunProfile,
  onSetDescription,
  onTogglePin,
  onOpenInEditor,
  profiles,
  statsMap,
}: ProjectSidebarProps) {
  const [search, setSearch] = useState("");
  const [ctxMenu, setCtxMenu] = useState<{ x: number; y: number; name: string } | null>(null);
  const [focusedIndex, setFocusedIndex] = useState(-1);
  const [showProfilePicker, setShowProfilePicker] = useState(false);
  const [pickerFocusedIndex, setPickerFocusedIndex] = useState(0);
  const [editingDesc, setEditingDesc] = useState<string | null>(null);
  const [descInput, setDescInput] = useState("");
  const [tooltip, setTooltip] = useState<{ stats: ProjectStats; x: number; y: number } | null>(null);
  const [sortKey, setSortKey] = useState<"name" | "recent" | "count">("name");
  const listRef = useRef<HTMLDivElement>(null);

  const sortLabels: Record<string, string> = { name: "名称", recent: "最近", count: "会话数" };

  const cycleSort = useCallback(() => {
    setSortKey((prev) => prev === "name" ? "recent" : prev === "recent" ? "count" : "name");
  }, []);

  // Sort: pinned first, then by sortKey
  const sorted = useMemo(() => {
    const list = search
      ? projects.filter((p) => p.name.toLowerCase().includes(search.toLowerCase()))
      : [...projects];
    list.sort((a, b) => {
      if (a.pinned !== b.pinned) return a.pinned ? -1 : 1;
      if (sortKey === "name") return a.name.localeCompare(b.name);
      if (sortKey === "recent") {
        const ta = statsMap[a.path]?.latestTimestamp ?? 0;
        const tb = statsMap[b.path]?.latestTimestamp ?? 0;
        if (tb !== ta) return tb - ta; // descending
        return a.name.localeCompare(b.name);
      }
      if (sortKey === "count") {
        const ca = statsMap[a.path]?.sessionCount ?? 0;
        const cb = statsMap[b.path]?.sessionCount ?? 0;
        if (cb !== ca) return cb - ca; // descending
        return a.name.localeCompare(b.name);
      }
      return 0;
    });
    return list;
  }, [projects, search, sortKey, statsMap]);

  // ── Keyboard navigation ──
  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setFocusedIndex((prev) => {
        const next = prev + 1 >= sorted.length ? 0 : prev + 1;
        if (sorted.length > 0) onSelect(sorted[next]);
        return next;
      });
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setFocusedIndex((prev) => {
        const next = prev - 1 < 0 ? sorted.length - 1 : prev - 1;
        if (sorted.length > 0) onSelect(sorted[next]);
        return next;
      });
    } else if (e.key === "Enter" && focusedIndex >= 0 && focusedIndex < sorted.length) {
      e.preventDefault();
      openProfilePicker(sorted[focusedIndex]);
    }
  }, [sorted, focusedIndex, onSelect]);

  // Keep focusedIndex in sync with selectedProject, so keyboard navigation
  // always starts from the currently selected item regardless of how it was
  // selected (mouse click, keyboard, or external programmatic selection).
  useEffect(() => {
    if (!selectedProject) {
      setFocusedIndex(-1);
      return;
    }
    const idx = sorted.findIndex((p) => p.name === selectedProject.name);
    setFocusedIndex(idx);
  }, [selectedProject, sorted]);

  // ── Context menu ──
  const onContextMenu = useCallback((e: React.MouseEvent, name: string) => {
    e.preventDefault();
    setCtxMenu({ x: e.clientX, y: e.clientY, name });
  }, []);

  // ── Profile picker ──
  const runTargetRef = useRef<ProjectInfo | null>(null);

  const openProfilePicker = useCallback((project: ProjectInfo) => {
    runTargetRef.current = project;
    onSelect(project);
    const defaultIdx = project.defaultProfile
      ? profiles.findIndex((profile) => profile.name === project.defaultProfile)
      : -1;
    setPickerFocusedIndex(defaultIdx >= 0 ? defaultIdx : 0);
    setShowProfilePicker(true);
  }, [onSelect, profiles]);

  const handleRunFromContext = useCallback(() => {
    if (!ctxMenu) return;
    const p = projects.find((pr) => pr.name === ctxMenu.name);
    if (!p) { setCtxMenu(null); return; }
    openProfilePicker(p);
    setCtxMenu(null);
  }, [ctxMenu, projects, openProfilePicker]);

  const handleSelectProfile = useCallback((project: ProjectInfo, profile: ProfileSummary) => {
    const cliType = profile.cli_type || "claude";
    onRunProfile(project.path, project.name, profile.name, cliType);
    setShowProfilePicker(false);
  }, [onRunProfile]);

  // ── Description editing ──
  const startEditDesc = useCallback((name: string, current: string) => {
    setEditingDesc(name);
    setDescInput(current);
  }, []);

  const saveDesc = useCallback((name: string) => {
    onSetDescription(name, descInput.trim());
    setEditingDesc(null);
    setDescInput("");
  }, [descInput, onSetDescription]);

  return (
    <div className="w-[300px] shrink-0 flex flex-col bg-app-sidebar border-r border-app-border select-none">
      <div className="px-2.5 pt-2.5 pb-2">
        <div className="flex items-center gap-1.5 mb-2.5">
          <Folder size={13} className="text-[var(--app-amber)] shrink-0" />
          <span className="text-2xs text-[var(--app-text)] font-mono tracking-[0.15em] uppercase flex-1">
            项目
          </span>
        </div>
        <SearchInput value={search} onChange={setSearch} placeholder="搜索项目..." />
      </div>
      <div className="mx-2.5 border-b border-app-border-light" />

      {/* Toolbar */}
      <div className="flex items-center gap-1 px-2 pt-1.5 pb-1">
        <Button variant="primary" size="sm" onClick={onAddProject} title="注册新项目">
          <Plus size={14} />
        </Button>

        <button
          disabled={!selectedProject}
          onClick={() => { if (selectedProject) openProfilePicker(selectedProject); }}
          className={`p-0.5 transition-colors duration-fast
            ${!selectedProject
              ? "text-app-text-muted opacity-40 cursor-not-allowed"
              : "text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)]"
            }`}
          title="运行"
        >
          <Play size={13} />
        </button>

        <button
          disabled={!selectedProject}
          onClick={() => selectedProject && onDeleteProject(selectedProject.name)}
          className={`p-0.5 transition-colors duration-fast
            ${!selectedProject
              ? "text-app-text-muted opacity-40 cursor-not-allowed"
              : "text-app-text-dim hover:text-app-red hover:bg-[var(--app-hover)]"
            }`}
          title="删除项目"
        >
          <Trash2 size={13} />
        </button>

        <div className="flex-1" />

        <button
          onClick={cycleSort}
          className="p-0.5 text-[var(--app-text-muted)]/40 hover:text-[var(--app-text-muted)] transition-colors"
          title={`排序: ${sortLabels[sortKey]}`}
        >
          <ArrowUpDown size={12} />
        </button>
      </div>

      {/* Project list */}
      <div
        ref={listRef}
        className="flex-1 overflow-y-auto overflow-x-hidden py-0.5 outline-none focus:outline-none focus-visible:outline-none focus:ring-0"
        tabIndex={0}
        onKeyDown={handleKeyDown}
      >
        {sorted.length === 0 && (
          <div className="flex flex-col items-center justify-center h-full gap-3 px-4 text-center">
            <Folder size={22} className="text-app-text-muted opacity-25" />
            <div className="text-xs text-app-text-dim">暂无项目</div>
            <div className="text-2xs text-app-text-muted leading-relaxed">
              注册一个项目目录开始
            </div>
          </div>
        )}
        {sorted.map((p, idx) => {
          const isSelected = p.name === selectedProject?.name;
          const isFocused = idx === focusedIndex;
          const stats = statsMap[p.path];
          const isEditingDesc = editingDesc === p.name;

          return (
            <div
              key={p.name}
              data-project-name={p.name}
              onClick={() => onSelect(p)}
              onContextMenu={(e) => onContextMenu(e, p.name)}
              className={`group relative mx-1.5 my-0.5 px-3 py-2.5 cursor-pointer
                ${isSelected
                  ? "bg-[var(--app-selected)] text-[var(--app-text)] border-l-[3px] border-l-[var(--app-amber)] transition-none"
                  : isFocused
                    ? "bg-[var(--app-hover)] text-[var(--app-text)] border-l-[3px] border-l-[var(--app-text-muted)] transition-all duration-150"
                    : p.pinned
                      ? "bg-[var(--app-amber)]/5 text-[var(--app-text)] border-l-[3px] border-l-[var(--app-amber)]/40 transition-all duration-150"
                      : "text-[var(--app-text)] border-l-[3px] border-l-transparent hover:bg-[var(--app-hover)] transition-all duration-150"
                }`}
            >

              {/* Main row: icon + name | CLI dots + stats */}
              <div className="flex items-center gap-2.5 min-w-0">
                {isSelected
                  ? <FolderOpen size={15} className="text-[var(--app-amber)] shrink-0" />
                  : <Folder size={15} className="text-[var(--app-text-muted)] shrink-0 group-hover:text-[var(--app-amber)] transition-colors" />
                }
                <span className={`truncate text-[13px] font-mono leading-snug flex-1 ${isSelected ? "font-semibold" : "font-normal"}`}>
                  {p.name}
                </span>

                {/* Right side: count + time, hover for CLI breakdown */}
                {stats && (stats.sessionCount > 0 || stats.latestTimestamp > 0) && (
                  <div className="flex items-center gap-2 shrink-0">
                    {stats.sessionCount > 0 && (
                      <span
                        className="flex items-center gap-1 text-[10px] font-mono text-[var(--app-text-muted)]/50 cursor-default py-1 px-1 -my-1 -mr-1"
                        onMouseEnter={(e) => {
                          const rect = e.currentTarget.getBoundingClientRect();
                          setTooltip({ stats, x: rect.left, y: rect.bottom + 6 });
                        }}
                        onMouseLeave={() => setTooltip(null)}
                      >
                        <span className="tabular-nums">{stats.sessionCount}</span>
                        <span className="text-[var(--app-text-muted)]/30">会话</span>
                      </span>
                    )}
                    {stats.latestTimestamp > 0 && (
                      <span className="text-[10px] font-mono text-[var(--app-text-muted)]/40">
                        {relativeTime(stats.latestTimestamp)}
                      </span>
                    )}
                  </div>
                )}
              </div>

              {/* Secondary: description or edit */}
              {isEditingDesc ? (
                <div className="flex items-center gap-1 mt-1" onClick={(e) => e.stopPropagation()}>
                  <input
                    autoFocus
                    value={descInput}
                    onChange={(e) => setDescInput(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") saveDesc(p.name);
                      if (e.key === "Escape") setEditingDesc(null);
                    }}
                    onBlur={() => saveDesc(p.name)}
                    className="w-full bg-[var(--app-bg)] border border-[var(--app-border)] px-2 py-0.5 text-[11px] font-mono text-[var(--app-text)] outline-none focus:border-[var(--app-accent)]"
                    placeholder="备注..."
                  />
                </div>
              ) : p.description ? (
                <div className="mt-1 text-[11px] text-[var(--app-text-dim)] font-mono leading-relaxed truncate">
                  {p.description}
                </div>
              ) : null}
            </div>
          );
        })}
      </div>

      {/* Context menu */}
      {ctxMenu && (() => {
        const p = projects.find((pr) => pr.name === ctxMenu.name);
        return (
          <ContextMenu
            x={ctxMenu.x} y={ctxMenu.y}
            onClose={() => setCtxMenu(null)}
            items={[
              {
                label: "运行",
                icon: <Play size={13} />,
                onClick: handleRunFromContext,
              },
              { separator: true as const },
              {
                label: "在 VS Code 中打开",
                icon: <ExternalLink size={13} />,
                onClick: () => {
                  if (p) { onOpenInEditor(p.path, "code"); setCtxMenu(null); }
                },
              },
              {
                label: "在 Cursor 中打开",
                icon: <ExternalLink size={13} />,
                onClick: () => {
                  if (p) { onOpenInEditor(p.path, "cursor"); setCtxMenu(null); }
                },
              },
              {
                label: "在 IntelliJ IDEA 中打开",
                icon: <ExternalLink size={13} />,
                onClick: () => {
                  if (p) { onOpenInEditor(p.path, "idea"); setCtxMenu(null); }
                },
              },
              {
                label: "在终端中打开",
                icon: <Terminal size={13} />,
                onClick: () => {
                  if (p) { onOpenInEditor(p.path, "terminal"); setCtxMenu(null); }
                },
              },
              {
                label: "在 Finder 中打开",
                icon: <FolderOpen size={13} />,
                onClick: () => {
                  if (p) { onOpenInEditor(p.path, "finder"); setCtxMenu(null); }
                },
              },
              { separator: true as const },
              {
                label: p?.pinned ? "取消置顶" : "置顶",
                icon: <Pin size={13} />,
                onClick: () => {
                  if (p) { onTogglePin(p.name, !p.pinned); setCtxMenu(null); }
                },
              },
              {
                label: "编辑备注",
                icon: <Pencil size={13} />,
                onClick: () => {
                  if (p) { startEditDesc(p.name, p.description || ""); setCtxMenu(null); }
                },
              },
              {
                label: "删除项目",
                icon: <Trash2 size={13} />,
                onClick: () => { onDeleteProject(ctxMenu.name); setCtxMenu(null); },
                danger: true,
              },
            ]}
          />
        );
      })()}

      {/* Session stats tooltip — portal to body */}
      {tooltip && createPortal(
        <div
          className="fixed z-50 bg-[var(--app-panel)] border border-[var(--app-border)] shadow-lg px-3 py-2 min-w-[130px]"
          style={{ left: tooltip.x, top: tooltip.y }}
        >
          <div className="text-[10px] font-mono text-[var(--app-text-muted)]/40 uppercase tracking-wider mb-1.5">
            会话分布
          </div>
          {[
            { id: "claude", label: "Claude", count: tooltip.stats.claudeCount, color: "bg-[var(--app-accent)]" },
            { id: "codex", label: "Codex", count: tooltip.stats.codexCount, color: "bg-[var(--app-blue)]" },
            { id: "qoder", label: "Qoder", count: tooltip.stats.qoderCount, color: "bg-[var(--app-purple)]" },
          ].map((cli) => (
            <div
              key={cli.id}
              className={`flex items-center gap-2.5 text-[11px] font-mono py-0.5 ${cli.count > 0 ? "text-[var(--app-text)]" : "text-[var(--app-text-muted)]/25"}`}
            >
              <span className={`w-2 h-2 rounded-full shrink-0 ${cli.color} ${cli.count === 0 ? "opacity-15" : ""}`} />
              <span className="flex-1">{cli.label}</span>
              <span className="text-[10px] text-[var(--app-text-muted)]/40 tabular-nums">
                {cli.count > 0 ? cli.count : "—"}
              </span>
            </div>
          ))}
        </div>,
        document.body
      )}

      {/* Profile picker modal */}
      <Dialog open={showProfilePicker} onClose={() => setShowProfilePicker(false)} width="400px">
        <div
          className="flex flex-col max-h-[360px] outline-none"
          onKeyDown={(e) => {
            if (e.key === "ArrowDown") {
              e.preventDefault();
              setPickerFocusedIndex((prev) => prev + 1 >= profiles.length ? 0 : prev + 1);
            } else if (e.key === "ArrowUp") {
              e.preventDefault();
              setPickerFocusedIndex((prev) => prev - 1 < 0 ? profiles.length - 1 : prev - 1);
            } else if (e.key === "Enter" && profiles.length > 0) {
              e.preventDefault();
              const p = profiles[pickerFocusedIndex];
              if (runTargetRef.current) handleSelectProfile(runTargetRef.current, p);
            }
          }}
        >
          <div className="flex items-center justify-between px-4 py-3 border-b border-[var(--app-border)]">
            <div className="flex items-center gap-2">
              <Play size={14} className="text-[var(--app-accent)]" />
              <span className="text-sm font-mono font-medium text-[var(--app-text)]">
                运行 {runTargetRef.current?.name ?? ""}
              </span>
            </div>
            <span className="text-3xs text-[var(--app-text-muted)] font-mono">
              ↑↓ 选择 · Enter 运行
            </span>
          </div>
          <div className="overflow-y-auto flex-1 py-1">
            {profiles.length === 0 ? (
              <div className="px-4 py-6 text-center text-xs text-[var(--app-text-muted)] font-mono">
                暂无 Profile，请先创建
              </div>
            ) : (
              profiles.map((p, idx) => {
                const isFocused = idx === pickerFocusedIndex;
                const isProjectDefault = p.name === runTargetRef.current?.defaultProfile;
                return (
                  <button
                    key={p.name}
                    onClick={() => runTargetRef.current && handleSelectProfile(runTargetRef.current, p)}
                    className={`w-full text-left px-4 py-2 text-xs font-mono transition-colors flex items-center gap-2
                      ${isFocused
                        ? "bg-[var(--app-selected)] text-[var(--app-text)]"
                        : "text-[var(--app-text-dim)] hover:bg-[var(--app-hover)] hover:text-[var(--app-text)]"
                      }`}
                  >
                    <span className={`truncate ${isProjectDefault || p.is_default ? "text-[var(--app-accent)] font-medium" : ""}`}>
                      {p.name}
                    </span>
                    <div className="flex items-center gap-1.5 ml-auto shrink-0">
                      {isProjectDefault ? (
                        <>
                          <Star size={10} className="text-[var(--app-accent)]" />
                          <span className="text-3xs text-[var(--app-text-muted)]">项目默认</span>
                        </>
                      ) : p.is_default && (
                        <>
                          <Star size={10} className="text-[var(--app-text-muted)]" />
                          <span className="text-3xs text-[var(--app-text-muted)]">全局默认</span>
                        </>
                      )}
                      {p.cli_type && (
                        <span className="text-3xs text-[var(--app-text-muted)]">{p.cli_type}</span>
                      )}
                    </div>
                  </button>
                );
              })
            )}
          </div>
        </div>
      </Dialog>
    </div>
  );
}
