import React, { useState, useRef, useEffect } from "react";
import type { ProjectInfo } from "../lib/types";

interface ProjectSelectorProps {
  projects: ProjectInfo[];
  active: ProjectInfo | null;
  onSelect: (project: ProjectInfo | null) => void;
  onAddProject: () => void;
}

export const ProjectSelector = React.memo(function ProjectSelector({
  projects,
  active,
  onSelect,
  onAddProject,
}: ProjectSelectorProps) {
  const [open, setOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  // Close dropdown on outside click
  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open]);

  return (
    <div className="relative" ref={containerRef}>
      <div className="flex items-center gap-1.5 py-0.5">
        <span className="text-3xs text-[var(--app-text-muted)] uppercase tracking-[0.1em] font-mono shrink-0">
          项目
        </span>
        <button
          onClick={() => setOpen(!open)}
          className="flex-1 flex items-center gap-1.5 px-2 py-0.5 text-2xs font-mono
            bg-[var(--app-input)] border border-[var(--app-border)]
            text-[var(--app-amber)] hover:text-[var(--app-amber-glow)]
            hover:border-[var(--app-amber)] hover:bg-[var(--app-hover)]
            transition-all duration-100 cursor-pointer outline-none"
        >
          <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" className="shrink-0 opacity-70">
            <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
          </svg>
          <span className="flex-1 text-left truncate">
            {active ? active.name : <span className="text-[var(--app-accent)]">全部项目</span>}
          </span>
          <svg width="8" height="8" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"
            className={`shrink-0 transition-transform duration-150 ${open ? "rotate-180" : ""}`}>
            <polyline points="6 9 12 15 18 9"/>
          </svg>
        </button>
      </div>

      {open && (
        <div
          className="absolute top-full left-0 right-0 z-50 mt-0.5 bg-[var(--app-panel)]
            border border-[var(--app-border)] shadow-lg"
          style={{ animation: "slideUp 100ms ease" }}
        >
          {/* "全部项目" — clear project filter */}
          <button
            onClick={() => { onSelect(null); setOpen(false); }}
            className={`w-full flex items-center gap-1.5 px-2 py-1.5 text-2xs font-mono text-left
              transition-colors duration-60 cursor-pointer border-none outline-none
              ${active === null
                ? "text-[var(--app-accent)] bg-[var(--app-hover)]"
                : "text-[var(--app-text-dim)] hover:text-[var(--app-text)] hover:bg-[var(--app-hover)]"
              }`}
          >
            <span className="shrink-0">🌐</span>
            <span className="flex-1 truncate">全部项目</span>
            {active === null && <span className="text-[var(--app-accent)] shrink-0">✓</span>}
          </button>
          {projects.length > 0 && (
            <div className="border-t border-[var(--app-border)] my-0.5" />
          )}
          {projects.map((p) => (
            <button
              key={p.name}
              onClick={() => { onSelect(p); setOpen(false); }}
              className={`w-full flex items-center gap-1.5 px-2 py-1.5 text-2xs font-mono text-left
                transition-colors duration-60 cursor-pointer border-none outline-none
                ${active?.name === p.name
                  ? "text-[var(--app-amber)] bg-[var(--app-hover)]"
                  : "text-[var(--app-text-dim)] hover:text-[var(--app-text)] hover:bg-[var(--app-hover)]"
                }`}
            >
              <span className="shrink-0">📁</span>
              <span className="flex-1 truncate">{p.name}</span>
              {active?.name === p.name && (
                <span className="text-[var(--app-amber)] shrink-0">✓</span>
              )}
              <span className="text-3xs text-[var(--app-text-muted)] truncate max-w-[120px]">{p.path}</span>
            </button>
          ))}
          {projects.length > 0 && (
            <div className="border-t border-[var(--app-border)] my-0.5" />
          )}
          <button
            onClick={() => { setOpen(false); onAddProject(); }}
            className="w-full flex items-center gap-1.5 px-2 py-1.5 text-2xs font-mono text-left
              text-[var(--app-accent-dim)] hover:text-[var(--app-accent)] hover:bg-[var(--app-hover)]
              transition-colors duration-60 cursor-pointer border-none outline-none"
          >
            <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
            </svg>
            <span>注册新项目...</span>
          </button>
        </div>
      )}
    </div>
  );
});
