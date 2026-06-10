import React, { useState, useCallback } from "react";
import { SearchInput } from "./common/SearchInput";
import { ContextMenu } from "./ContextMenu";
import { Folder, Trash2, Pencil, FolderOpen } from "lucide-react";
import type { ProjectInfo } from "../lib/types";

interface ProjectSidebarProps {
  projects: ProjectInfo[];
  selectedProject: ProjectInfo | null;
  onSelect: (project: ProjectInfo | null) => void;
  onAddProject: () => void;
  onDeleteProject: (name: string) => void;
  onRenameProject: (name: string) => void;
  onChangePath: (name: string) => void;
}

export function ProjectSidebar({
  projects,
  selectedProject,
  onSelect,
  onAddProject,
  onDeleteProject,
  onRenameProject,
  onChangePath,
}: ProjectSidebarProps) {
  const [search, setSearch] = useState("");
  const [ctxMenu, setCtxMenu] = useState<{ x: number; y: number; name: string } | null>(null);

  const onContextMenu = useCallback((e: React.MouseEvent, name: string) => {
    e.preventDefault();
    setCtxMenu({ x: e.clientX, y: e.clientY, name });
  }, []);

  const filtered = search
    ? projects.filter((p) => p.name.toLowerCase().includes(search.toLowerCase()))
    : projects;

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

      <div className="flex-1 overflow-y-auto overflow-x-hidden py-0.5">
        {filtered.length === 0 && (
          <div className="flex flex-col items-center justify-center h-full gap-3 px-4 text-center">
            <Folder size={22} className="text-app-text-muted opacity-25" />
            <div className="text-xs text-app-text-dim">暂无项目</div>
            <div className="text-2xs text-app-text-muted leading-relaxed">
              注册一个项目目录开始
            </div>
          </div>
        )}
        {filtered.map((p) => {
          const isSelected = p.name === selectedProject?.name;
          return (
            <div
              key={p.name}
              onClick={() => onSelect(isSelected ? null : p)}
              onContextMenu={(e) => onContextMenu(e, p.name)}
              className={`group flex flex-col mx-1 my-px px-2.5 py-1.5 cursor-pointer
                transition-all duration-fast
                ${isSelected
                  ? "bg-app-selected text-app-text border-l-[3px] border-l-app-amber shadow-[inset_0_0_8px_var(--app-glow)]"
                  : "text-app-text border-l-[3px] border-l-transparent hover:bg-app-hover active:bg-app-active"
                }`}
            >
              <div className="flex items-center gap-2">
                {isSelected
                  ? <FolderOpen size={14} className="text-[var(--app-amber)] shrink-0" />
                  : <Folder size={14} className="text-[var(--app-text-muted)] shrink-0 group-hover:text-[var(--app-amber)]" />
                }
                <span className={`truncate text-sm font-mono ${isSelected ? "font-medium" : "font-normal"}`}>
                  {p.name}
                </span>
              </div>
              {p.defaultProfile && (
                <div className="flex items-center gap-1 mt-0.5 ml-6">
                  <span className="text-3xs text-[var(--app-text-muted)] font-mono">默认:</span>
                  <span className="text-3xs text-[var(--app-amber)] font-mono bg-[var(--app-amber-bg)] px-1 rounded">
                    {p.defaultProfile}
                  </span>
                </div>
              )}
            </div>
          );
        })}
      </div>

      <div className="p-2 border-t border-app-border">
        <button
          onClick={onAddProject}
          className="w-full flex items-center justify-center gap-1.5 px-2 py-1.5
            text-2xs font-mono text-[var(--app-accent-dim)] hover:text-[var(--app-accent)]
            hover:bg-[var(--app-hover)] border border-dashed border-[var(--app-border)]
            hover:border-[var(--app-accent)] transition-all duration-100 cursor-pointer"
        >
          <span>+</span>
          <span>注册新项目...</span>
        </button>
      </div>

      {ctxMenu && (
        <ContextMenu
          x={ctxMenu.x} y={ctxMenu.y}
          onClose={() => setCtxMenu(null)}
          items={[
            {
              label: "修改名称",
              icon: <Pencil size={13} />,
              onClick: () => { onRenameProject(ctxMenu.name); setCtxMenu(null); },
            },
            {
              label: "修改路径",
              icon: <FolderOpen size={13} />,
              onClick: () => { onChangePath(ctxMenu.name); setCtxMenu(null); },
            },
            {
              label: "删除项目",
              icon: <Trash2 size={13} />,
              onClick: () => { onDeleteProject(ctxMenu.name); setCtxMenu(null); },
              danger: true,
            },
          ]}
        />
      )}
    </div>
  );
}
