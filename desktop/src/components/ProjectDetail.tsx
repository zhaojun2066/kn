import React, { useState, useCallback, useEffect, useRef } from "react";
import { createPortal } from "react-dom";
import { FileTree, type FileTreeNode } from "./FileTree";
import { FileContentBlock } from "./common/FileContentBlock";
import { SessionList } from "./SessionList";
import { FolderOpen, Terminal, ExternalLink, ChevronDown, Play } from "lucide-react";
import type { ProjectInfo, SessionInfo, ProfileSummary } from "../lib/types";
import { invoke } from "@tauri-apps/api/core";

interface ProjectDetailProps {
  project: ProjectInfo;
  profiles: ProfileSummary[];
  sessions: SessionInfo[];
  sessionsLoading: boolean;
  onResumeSession: (session: SessionInfo) => void;
  onRunProfile: (profileName: string, cliType: string) => void;
  onScanSessions: (projectPath: string) => void;
}

type DetailTab = "files" | "sessions";

export function ProjectDetail({
  project,
  profiles,
  sessions,
  sessionsLoading,
  onResumeSession,
  onRunProfile,
  onScanSessions,
}: ProjectDetailProps) {
  const [tab, setTab] = useState<DetailTab>("files");
  const [selectedFile, setSelectedFile] = useState<FileTreeNode | null>(null);
  const [fileContent, setFileContent] = useState<string>("");
  const [showProfilePicker, setShowProfilePicker] = useState(false);
  const [pickerPosition, setPickerPosition] = useState({ x: 0, y: 0 });
  const [treeWidth, setTreeWidth] = useState(240);

  // Track which project path sessions were last scanned for, so we only
  // scan when the user actually clicks the "sessions" tab.
  const sessionsScannedForRef = useRef<string | null>(null);

  // Scan sessions only when user switches to the sessions tab and data is stale
  const handleTabChange = useCallback((newTab: DetailTab) => {
    setTab(newTab);
    if (newTab === "sessions" && sessionsScannedForRef.current !== project.path) {
      sessionsScannedForRef.current = project.path;
      onScanSessions(project.path);
    }
  }, [project.path, onScanSessions]);

  // Debounce timer for session scanning when project changes rapidly (e.g. arrow key nav)
  const scanTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // When project changes while on the sessions tab, debounce the scan to avoid
  // stuttering during rapid arrow-key navigation. File tree updates instantly.
  useEffect(() => {
    sessionsScannedForRef.current = null;
    if (scanTimerRef.current) clearTimeout(scanTimerRef.current);
    if (tab === "sessions") {
      scanTimerRef.current = setTimeout(() => {
        sessionsScannedForRef.current = project.path;
        onScanSessions(project.path);
      }, 300);
    }
    return () => {
      if (scanTimerRef.current) clearTimeout(scanTimerRef.current);
    };
  }, [project.path, tab]);

  // Load file content when selected file changes
  useEffect(() => {
    if (!selectedFile || selectedFile.is_dir) {
      setFileContent("");
      return;
    }
    invoke<string>("read_file", { path: selectedFile.path })
      .then(setFileContent)
      .catch(() => setFileContent(""));
  }, [selectedFile]);

  const handleRunClick = useCallback((e: React.MouseEvent) => {
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    // Prevent dropdown from overflowing the right edge of the viewport
    const dropdownMinWidth = 200;
    const x = Math.min(rect.left, window.innerWidth - dropdownMinWidth - 12);
    setPickerPosition({ x, y: rect.bottom + 4 });
    setShowProfilePicker((v) => !v);
  }, []);

  const handleSelectProfile = useCallback((profile: ProfileSummary) => {
    const cliType = profile.cli_type || "claude";
    onRunProfile(profile.name, cliType);
    setShowProfilePicker(false);
  }, [onRunProfile]);

  const handleOpenInTerminal = useCallback(() => {
    invoke("open_in_terminal", { path: project.path }).catch((e) =>
      console.error("open_in_terminal failed:", e)
    );
  }, [project.path]);

  const handleOpenInFinder = useCallback(() => {
    invoke("open_file", { path: project.path }).catch((e) =>
      console.error("open_file failed:", e)
    );
  }, [project.path]);

  const handleCopyPath = useCallback(() => {
    navigator.clipboard.writeText(project.path).catch(() => {});
  }, [project.path]);

  // File tree resize — horizontal drag
  const handleTreeResize = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    const startX = e.clientX;
    const startWidth = treeWidth;
    const onMouseMove = (ev: MouseEvent) => {
      const delta = ev.clientX - startX;
      setTreeWidth(Math.max(160, Math.min(600, startWidth + delta)));
    };
    const onMouseUp = () => {
      document.removeEventListener("mousemove", onMouseMove);
      document.removeEventListener("mouseup", onMouseUp);
    };
    document.addEventListener("mousemove", onMouseMove);
    document.addEventListener("mouseup", onMouseUp);
  }, [treeWidth]);

  return (
    <div className="flex-1 flex flex-col bg-[var(--app-bg)] overflow-hidden">
      {/* Header */}
      <div className="flex items-center gap-2 px-3 py-2 border-b border-[var(--app-border)] shrink-0">
        <FolderOpen size={16} className="text-[var(--app-amber)] shrink-0" />
        <span className="text-sm font-mono font-medium text-[var(--app-text)] truncate">
          {project.name}
        </span>
        <span
          onClick={handleCopyPath}
          className="text-3xs font-mono text-[var(--app-text-muted)] truncate cursor-pointer hover:text-[var(--app-text-dim)] ml-1"
          title={project.path}
        >
          {project.path}
        </span>

        <div className="flex-1" />

        {/* Run button — split button style */}
        <div className="flex items-stretch">
          <button
            onClick={handleRunClick}
            className="flex items-center gap-1 px-2 py-1 text-2xs font-mono
              bg-[var(--app-accent)] text-white hover:opacity-90
              border-none outline-none cursor-pointer
              transition-opacity duration-100"
          >
            <Play size={11} className="shrink-0" />
            <span>运行</span>
          </button>
          <button
            onClick={handleRunClick}
            className="flex items-center px-1 py-1 text-2xs font-mono
              bg-[var(--app-accent)] text-white hover:opacity-90
              border-l border-white/20 border-none outline-none cursor-pointer
              transition-opacity duration-100"
          >
            <ChevronDown size={11} />
          </button>
        </div>

        <button
          onClick={handleOpenInTerminal}
          className="p-1 text-[var(--app-text-muted)] hover:text-[var(--app-text)] transition-colors"
          title="在终端中打开"
        >
          <Terminal size={14} />
        </button>
        <button
          onClick={handleOpenInFinder}
          className="p-1 text-[var(--app-text-muted)] hover:text-[var(--app-text)] transition-colors"
          title="在 Finder 中打开"
        >
          <ExternalLink size={14} />
        </button>
      </div>

      {/* Tabs */}
      <div className="flex items-center border-b border-[var(--app-border)] shrink-0 px-2">
        {(["files", "sessions"] as DetailTab[]).map((t) => (
          <button
            key={t}
            onClick={() => handleTabChange(t)}
            className={`px-3 py-1.5 text-2xs font-mono transition-colors
              ${tab === t
                ? "text-[var(--app-accent)] border-b border-[var(--app-accent)]"
                : "text-[var(--app-text-muted)] hover:text-[var(--app-text)]"
              }`}
          >
            {t === "files" ? "文件" : "会话"}
          </button>
        ))}
      </div>

      {/* Tab content */}
      {tab === "files" ? (
        <div className="flex-1 flex overflow-hidden min-h-0">
          <div
            className="shrink-0 border-r border-[var(--app-border)] overflow-y-auto"
            style={{ width: treeWidth }}
          >
            <FileTree
              key={project.path}
              rootPath={project.path}
              onSelect={setSelectedFile}
              activePath={selectedFile?.path}
            />
          </div>
          {/* Resize handle */}
          <div
            className="w-[5px] shrink-0 cursor-col-resize hover:bg-[var(--app-accent)]/20 transition-colors duration-fast group/resize flex items-center justify-center"
            onMouseDown={handleTreeResize}
          >
            <div className="w-px h-full bg-[var(--app-border)] group-hover/resize:bg-[var(--app-accent)]/50" />
          </div>
          <div className="flex-1 overflow-y-auto">
            {selectedFile && !selectedFile.is_dir ? (
              <FileContentBlock
                content={fileContent}
                filePath={selectedFile.path}
              />
            ) : (
              <div className="flex items-center justify-center h-full text-xs text-[var(--app-text-muted)] font-mono">
                选择文件以预览
              </div>
            )}
          </div>
        </div>
      ) : (
        <div className="flex-1 overflow-y-auto">
          <SessionList
            sessions={sessions}
            loading={sessionsLoading}
            onResume={onResumeSession}
          />
        </div>
      )}

      {/* Profile picker dropdown — portal to body to avoid overflow clipping */}
      {showProfilePicker && createPortal(
        <>
          <div
            className="fixed z-50 bg-[var(--app-panel)] border border-[var(--app-border)] shadow-lg min-w-[200px] max-h-[300px] overflow-y-auto"
            style={{ left: pickerPosition.x, top: pickerPosition.y }}
          >
            <div className="px-2 py-1 text-3xs text-[var(--app-text-muted)] font-mono uppercase">
              选择 Profile
            </div>
            <div className="border-t border-[var(--app-border)]" />
            {project.defaultProfile && (
              <>
                <button
                  onClick={() => {
                    const p = profiles.find((pr) => pr.name === project.defaultProfile);
                    if (p) handleSelectProfile(p);
                  }}
                  className="w-full text-left px-3 py-1.5 text-2xs font-mono text-[var(--app-amber)] hover:bg-[var(--app-hover)] transition-colors flex items-center gap-2"
                >
                  <span>⭐</span>
                  <span>{project.defaultProfile}</span>
                  <span className="text-3xs text-[var(--app-text-muted)]">默认</span>
                </button>
                <div className="border-t border-[var(--app-border)]" />
              </>
            )}
            {profiles.map((p) => (
              <button
                key={p.name}
                onClick={() => handleSelectProfile(p)}
                className="w-full text-left px-3 py-1.5 text-2xs font-mono text-[var(--app-text)] hover:bg-[var(--app-hover)] transition-colors flex items-center gap-2"
              >
                <span className={p.is_default ? "text-[var(--app-accent)]" : ""}>
                  {p.name}
                </span>
                {p.is_default && (
                  <span className="text-3xs text-[var(--app-text-muted)]">全局默认</span>
                )}
              </button>
            ))}
          </div>
          {/* Click-outside backdrop */}
          <div
            className="fixed inset-0 z-40"
            onClick={() => setShowProfilePicker(false)}
          />
        </>,
        document.body
      )}
    </div>
  );
}
