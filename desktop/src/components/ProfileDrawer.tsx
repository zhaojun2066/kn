import React, { useState, useEffect, useCallback, useRef } from "react";
import { Folder, X } from "lucide-react";
import { open as tauriOpen } from "@tauri-apps/plugin-dialog";
import type { ProfileDetail, ProfileSummary, ProjectInfo, EnvCheckResult } from "../lib/types";
import type { SessionRecord } from "../hooks/useTerminal";
import { useResizeHandle } from "../hooks/useResizeHandle";
import { projectLikeListRowChrome, projectLikeListRowState } from "./common/listRowStyles";
import { Sidebar } from "./Sidebar";
import { MainPanel } from "./MainPanel";
import { Dialog } from "./common/Dialog";

interface ProfileDrawerProps {
  open: boolean;
  // ── Sidebar props ──
  profiles: ProfileSummary[];
  selectedName: string | null;
  searchQuery: string;
  onClose: () => void;
  onSelect: (name: string) => void;
  onSearch: (query: string) => void;
  onCopy: (name: string) => void;
  onRename: (name: string) => void;
  onDelete: (name: string) => void;
  onSetDefault: (name: string) => void;
  usageCounts?: Record<string, number>;
  isDefault?: boolean;
  hasSelection?: boolean;
  backupExists?: boolean;
  onAdd: () => void;
  onCopyProfile: () => void;
  onInit: () => void;
  onImport: () => void;
  onExport: () => void;
  onBatchDelete?: (names: string[]) => void;
  onBatchExport?: (names: string[]) => void;
  onRefresh: () => void;
  onBackup: () => void;
  onRestore: () => void;
  // ── MainPanel (detail) props ──
  selectedProfile: ProfileDetail | null;
  allTags: string[];
  history: SessionRecord[];
  envCheck?: EnvCheckResult;
  onSetEnv: (key: string, value: string) => Promise<void>;
  onDeleteEnv: (key: string) => Promise<void>;
  onPasteCommand: (command: string) => void;
  onSplitCommand?: (command: string) => void;
  onRenameProfile: (name: string) => void;
  onResumeSession: (record: SessionRecord) => void;
  onNewSessionFromHistory: (record: SessionRecord) => void;
  onDeleteHistory: (id: string) => void;
  onClearProfileHistory: (profileName: string) => void;
  onSetTags: (name: string, tags: string) => Promise<void>;
  // ── Project picker props ──
  projects?: ProjectInfo[];
  onRunProfileInProject?: (profileName: string, cliType: string, projectPath: string, projectName: string) => void;
}

export function ProfileDrawer({
  open,
  // Sidebar
  profiles,
  selectedName,
  searchQuery,
  onClose,
  onSelect,
  onSearch,
  onCopy,
  onRename,
  onDelete,
  onSetDefault,
  usageCounts,
  isDefault = false,
  hasSelection = false,
  backupExists = false,
  onAdd,
  onCopyProfile,
  onInit,
  onImport,
  onExport,
  onBatchDelete,
  onBatchExport,
  onRefresh,
  onBackup,
  onRestore,
  // MainPanel
  selectedProfile,
  allTags,
  history,
  envCheck,
  onSetEnv,
  onDeleteEnv,
  onPasteCommand,
  onSplitCommand,
  onRenameProfile,
  onResumeSession,
  onNewSessionFromHistory,
  onDeleteHistory,
  onClearProfileHistory,
  onSetTags,
  // Project picker
  projects = [],
  onRunProfileInProject,
}: ProfileDrawerProps) {
  /* ── Resizable width ── */
  const [maxWidth, setMaxWidth] = useState(() => Math.max(400, window.innerWidth - 12));
  const [defaultDrawerWidth, setDefaultDrawerWidth] = useState(() => Math.round(window.innerWidth * 0.6));
  useEffect(() => {
    const updateDrawerBounds = () => {
      setMaxWidth(Math.max(400, window.innerWidth - 12));
      setDefaultDrawerWidth(Math.round(window.innerWidth * 0.6));
    };
    window.addEventListener("resize", updateDrawerBounds);
    return () => window.removeEventListener("resize", updateDrawerBounds);
  }, []);
  const { size: drawerWidth, handleProps } = useResizeHandle({
    direction: "horizontal",
    minSize: 400,
    maxSize: maxWidth,
    defaultSize: defaultDrawerWidth,
    storageKey: "kn-profile-drawer-width-v3",
  });

  // ── Project picker state ──
  const [showProjectPicker, setShowProjectPicker] = useState(false);
  const [projectPickerIdx, setProjectPickerIdx] = useState(0);
  const [activeProfileForPicker, setActiveProfileForPicker] = useState<{ name: string; cliType: string } | null>(null);
  // Reset picker when drawer opens/closes
  useEffect(() => { if (!open) { setShowProjectPicker(false); setActiveProfileForPicker(null); } }, [open]);

  const handleProfileEnter = useCallback((name: string, cliType: string) => {
    setActiveProfileForPicker({ name, cliType });
    setProjectPickerIdx(0);
    setShowProjectPicker(true);
  }, []);

  // Keep latest picker state in refs so the keydown handler always reads fresh values
  const projectPickerIdxRef = useRef(projectPickerIdx);
  projectPickerIdxRef.current = projectPickerIdx;
  const activeProfileRef = useRef(activeProfileForPicker);
  activeProfileRef.current = activeProfileForPicker;
  const projectsRef = useRef(projects);
  projectsRef.current = projects;

  // Select a project from the list and close everything
  const handleSelectProject = useCallback((index: number) => {
    if (!activeProfileForPicker) return;
    const p = projects[index];
    if (!p) return;
    onRunProfileInProject?.(activeProfileForPicker.name, activeProfileForPicker.cliType, p.path, p.name);
    setShowProjectPicker(false);
    setActiveProfileForPicker(null);
    onClose();
  }, [activeProfileForPicker, projects, onRunProfileInProject, onClose]);

  // Browse folder fallback
  const handleBrowseFolder = useCallback(async () => {
    if (!activeProfileForPicker) return;
    const selected = await tauriOpen({ directory: true, multiple: false, title: "选择项目工作目录" });
    if (selected && typeof selected === "string") {
      onRunProfileInProject?.(activeProfileForPicker.name, activeProfileForPicker.cliType, selected, selected.split("/").pop() || selected);
    }
    setShowProjectPicker(false);
    setActiveProfileForPicker(null);
    onClose();
  }, [activeProfileForPicker, onRunProfileInProject, onClose]);

  // Keyboard navigation within the project picker dialog
  const handlePickerKeyDown = useCallback((e: React.KeyboardEvent) => {
    const itemCount = projectsRef.current.length + 1;
    if (e.key === "ArrowDown") {
      e.preventDefault(); e.stopPropagation();
      setProjectPickerIdx((i) => (i >= itemCount - 1 ? 0 : i + 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault(); e.stopPropagation();
      setProjectPickerIdx((i) => (i <= 0 ? itemCount - 1 : i - 1));
    } else if (e.key === "Enter") {
      e.preventDefault(); e.stopPropagation();
      const idx = projectPickerIdxRef.current;
      const profile = activeProfileRef.current;
      const projs = projectsRef.current;
      if (!profile) return;
      if (idx < projs.length) {
        const p = projs[idx];
        if (!p) return;
        onRunProfileInProject?.(profile.name, profile.cliType, p.path, p.name);
        setShowProjectPicker(false);
        setActiveProfileForPicker(null);
        onClose();
      } else {
        // Browse folder fallback via Enter
        tauriOpen({ directory: true, multiple: false, title: "选择项目工作目录" }).then((selected) => {
          if (selected && typeof selected === "string") {
            onRunProfileInProject?.(profile.name, profile.cliType, selected, selected.split("/").pop() || selected);
          }
        }).catch(() => {});
        setShowProjectPicker(false);
        setActiveProfileForPicker(null);
        onClose();
      }
    } else if (e.key === "Escape") {
      e.preventDefault(); e.stopPropagation();
      setShowProjectPicker(false);
      setActiveProfileForPicker(null);
    }
  }, [onRunProfileInProject, onClose]);

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-[80] flex justify-end">
      <button
        aria-label="关闭运行配置管理遮罩"
        className="absolute inset-0 z-0 bg-black/45 animate-[fadeIn_160ms_ease-out]"
        onClick={onClose}
      />
      {/* Resize handle — outside section so internal content can't block it */}
      <div
        {...handleProps}
        className={`absolute top-0 bottom-0 w-3 z-[90] pointer-events-auto hover:bg-app-accent/15 transition-colors ${handleProps.className}`}
        style={{ right: `${drawerWidth}px` }}
      >
        <div className="absolute right-0 top-0 bottom-0 w-px bg-app-border" />
      </div>
      <section
        data-testid="profile-drawer-panel"
        className="relative z-20 h-full bg-app-bg border-l border-app-border shadow-dialog flex flex-col shrink-0"
        style={{ width: `${drawerWidth}px` }}
      >
        {/* ── Header ── */}
        <div className="h-[44px] shrink-0 flex items-center gap-3 px-4 border-b border-app-border bg-app-toolbar">
          <div className="text-sm font-mono text-app-text font-semibold">运行配置管理</div>
          <div className="text-2xs font-mono text-app-text-muted">全局运行配置</div>
          <button
            aria-label="关闭运行配置管理"
            onClick={onClose}
            className="ml-auto text-app-text-muted hover:text-app-text p-1 hover:bg-[var(--app-hover)] transition-colors"
          >
            <X size={14} />
          </button>
        </div>

        {/* ── Body: Sidebar (list) + MainPanel (detail) ── */}
        <div className="flex-1 min-h-0 flex">
          {/* Left: Profile list */}
          <Sidebar
            className="w-[280px] shrink-0 flex flex-col bg-app-sidebar border-r border-app-border select-none"
            profiles={profiles}
            selectedName={selectedName}
            searchQuery={searchQuery}
            onSelect={onSelect}
            onSearch={onSearch}
            onCopy={onCopy}
            onRename={onRename}
            onDelete={onDelete}
            onSetDefault={onSetDefault}
            usageCounts={usageCounts}
            isDefault={isDefault}
            hasSelection={hasSelection}
            backupExists={backupExists}
            onAdd={onAdd}
            onCopyProfile={onCopyProfile}
            onInit={onInit}
            onImport={onImport}
            onExport={onExport}
            onBatchDelete={onBatchDelete}
            onBatchExport={onBatchExport}
            onRefresh={onRefresh}
            onBackup={onBackup}
            onRestore={onRestore}
            onProfileEnter={handleProfileEnter}
          />

          {/* ── Project picker modal (centered Dialog) ── */}
          {showProjectPicker && activeProfileForPicker && (
            <Dialog
              open={showProjectPicker}
              onClose={() => {
                setShowProjectPicker(false);
                setActiveProfileForPicker(null);
              }}
              width="340px"
            >
              <div
                className="flex flex-col max-h-[360px] outline-none"
                onKeyDown={handlePickerKeyDown}
              >
                {/* Header */}
                <div className="flex items-center justify-between gap-4 px-4 py-3 border-b border-[var(--app-border-light)] bg-[var(--app-subtle)]/70">
                  <span className="text-sm text-[var(--app-text)] font-semibold">
                    选择运行项目
                  </span>
                  <span className="text-3xs text-[var(--app-text-muted)] font-mono">
                    ↑↓ 选择 · Enter 运行
                  </span>
                </div>

                {/* Project list */}
                <div className="overflow-y-auto flex-1 py-1">
                  {projects.length === 0 ? (
                    <div className="px-4 py-6 text-center text-xs text-[var(--app-text-muted)] font-mono">
                      暂无项目，请先添加项目
                    </div>
                  ) : (
                    projects.map((p, i) => (
                      <button
                        key={p.name}
                        data-project-index={i}
                        onMouseEnter={() => setProjectPickerIdx(i)}
                        onClick={() => handleSelectProject(i)}
                        className={`w-[calc(100%-12px)] text-left px-3 py-2.5 text-xs flex items-center gap-2.5
                          ${projectLikeListRowChrome} ${projectLikeListRowState({ selected: i === projectPickerIdx })}`}
                      >
                        <Folder size={14} className={i === projectPickerIdx ? "text-app-accent shrink-0" : "text-app-text-muted shrink-0"} />
                        <span className={`truncate flex-1 ${i === projectPickerIdx ? "font-semibold" : "font-medium"}`}>{p.name}</span>
                        <span className="text-3xs text-[var(--app-text-muted)] truncate max-w-[140px]">
                          {p.path.split("/").pop()}
                        </span>
                      </button>
                    ))
                  )}
                </div>

                {/* Browse folder fallback */}
                <button
                  data-project-index={projects.length}
                  onMouseEnter={() => setProjectPickerIdx(projects.length)}
                  onClick={handleBrowseFolder}
                  className={`mx-1.5 my-1.5 w-[calc(100%-12px)] text-left px-3 py-2.5 text-xs flex items-center gap-2.5
                    border border-[var(--app-border-light)] rounded-lg transition-all duration-fast
                    ${projectPickerIdx === projects.length
                      ? "bg-app-selected text-app-text border-app-accent shadow-sm"
                      : "text-app-text-dim hover:bg-app-hover hover:text-app-text"
                    }`}
                >
                  <Folder size={14} className={projectPickerIdx === projects.length ? "text-app-accent shrink-0" : "text-app-text-muted shrink-0"} />
                  <span className="font-medium">浏览文件夹...</span>
                </button>
              </div>
            </Dialog>
          )}

          {/* Right: Profile detail */}
          <div className="flex-1 min-w-0 overflow-hidden flex flex-col">
            <MainPanel
              profile={selectedProfile}
              hasProfiles={profiles.length > 0}
              allTags={allTags}
              history={history}
              envCheck={envCheck}
              onSetEnv={onSetEnv}
              onDeleteEnv={onDeleteEnv}
              onPasteCommand={onPasteCommand}
              onSplitCommand={onSplitCommand}
              onRunProfile={handleProfileEnter}
              onRenameProfile={onRenameProfile}
              onResumeSession={onResumeSession}
              onNewSessionFromHistory={onNewSessionFromHistory}
              onDeleteHistory={onDeleteHistory}
              onClearProfileHistory={onClearProfileHistory}
              onInit={onInit}
              onSetTags={onSetTags}
              onAdd={onAdd}
            />
          </div>
        </div>
      </section>
    </div>
  );
}
