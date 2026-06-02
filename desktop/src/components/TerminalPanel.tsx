import React, { useRef, useEffect, useCallback, useState } from "react";
import { X, Plus, FolderOpen, Clock, Trash2, Play, Minus, ExternalLink, Maximize2, Minimize2 } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { open as tauriOpen } from "@tauri-apps/plugin-dialog";
import { XTerm, XTermHandle } from "./XTerm";
import { ConfirmDialog } from "./ConfirmDialog";
import type { Terminal } from "@xterm/xterm";
import type { SessionRecord } from "../hooks/useTerminal";
import { shortenPath } from "../lib/path-utils";

interface TabInfo {
  id: string;
  name: string;
  workDir: string;
  ptyRunning: boolean;
}

interface TerminalPanelProps {
  width?: number;
  maximized?: boolean;
  onToggleMaximize?: () => void;
  tabs: TabInfo[];
  activeTabId: string;
  history: SessionRecord[];
  onAttachTerminal: (tabId: string, term: Terminal) => void;
  onClose: () => void;
  onSwitchTab: (tabId: string) => void;
  onCloseTab: (tabId: string) => void;
  onNewTab: () => void;
  onSetWorkDir: (tabId: string, dir: string) => void;
  onTerminalReady?: (tabId: string) => void;
  onTerminalResize?: (tabId: string, cols: number, rows: number) => void;
  fontSize: number;
  onSetFontSize: (size: number) => void;
  terminalVersion: number;
  onResumeSession: (record: SessionRecord) => void;
  onNewSessionFromHistory: (record: SessionRecord) => void;
  onDeleteHistory: (id: string) => void;
  onClearHistory: () => void;
}

/* ── Per-tab XTerm wrapper ──────────────────────────────── */
function TabTerminal({ tabId, onAttach, onReady, onResize, fontSize }: {
  tabId: string;
  onAttach: (term: Terminal) => void;
  onReady?: () => void;
  onResize?: (cols: number, rows: number) => void;
  fontSize?: number;
}) {
  const xtermRef = useRef<XTermHandle>(null);
  const attached = useRef(false);

  const handleTerminal = useCallback((term: Terminal) => {
    if (!attached.current) {
      attached.current = true;
      onAttach(term);
    }
  }, [onAttach]);

  useEffect(() => { attached.current = false; }, [tabId]);

  return <XTerm ref={xtermRef} onTerminal={handleTerminal} onReady={onReady} onResize={onResize} fontSize={fontSize} />;
}

/* ── Format relative time ───────────────────────────────── */
function formatTime(ts: number): string {
  const diff = Date.now() - ts;
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return "刚刚";
  if (mins < 60) return `${mins} 分钟前`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours} 小时前`;
  const days = Math.floor(hours / 24);
  if (days < 7) return `${days} 天前`;
  return new Date(ts).toLocaleDateString("zh-CN");
}

/* ── TerminalPanel ──────────────────────────────────────── */
export function TerminalPanel({
  width, maximized, onToggleMaximize,
  tabs, activeTabId, history,
  onAttachTerminal, onClose, onSwitchTab, onCloseTab, onNewTab,
  onSetWorkDir, onTerminalReady, onTerminalResize,
  fontSize, onSetFontSize, terminalVersion,
  onResumeSession, onNewSessionFromHistory, onDeleteHistory, onClearHistory,
}: TerminalPanelProps) {
  const activeTab = tabs.find((t) => t.id === activeTabId);
  const runningCount = tabs.filter((t) => t.ptyRunning).length;
  const [showHistory, setShowHistory] = useState(false);
  const [historySearch, setHistorySearch] = useState("");
  const [closingTabId, setClosingTabId] = useState<string | null>(null);
  const [closingPanel, setClosingPanel] = useState(false);

  const filteredHistory = historySearch.trim()
    ? history.filter((r) =>
        r.command.toLowerCase().includes(historySearch.toLowerCase()) ||
        r.workDir.toLowerCase().includes(historySearch.toLowerCase())
      )
    : history;

  const browseDir = async () => {
    try {
      const selected = await tauriOpen({ directory: true, multiple: false, title: "选择工作目录" });
      if (selected && typeof selected === "string" && activeTabId) {
        onSetWorkDir(activeTabId, selected);
      }
    } catch { /* */ }
  };

  useEffect(() => {
    const timer = setTimeout(() => window.dispatchEvent(new Event("resize")), 100);
    return () => clearTimeout(timer);
  }, [width]);

  return (
    <div
      style={width !== undefined ? { width: `${width}px` } : undefined}
      className={`flex flex-col bg-[var(--app-terminal-bg)] border-l border-app-border h-full ${maximized ? "flex-1" : "shrink-0"}`}
    >
      {/* Header */}
      <div className="flex items-center h-[28px] bg-[var(--app-terminal-header)] border-b border-app-border shrink-0 select-none">
        {/* Tab bar */}
        <div className="flex-1 flex items-center overflow-x-auto min-w-0">
          {tabs.map((tab) => {
            const isActive = tab.id === activeTabId;
            return (
              <div key={tab.id} onClick={() => onSwitchTab(tab.id)}
                className={`group flex items-center gap-1.5 px-2.5 h-[28px] text-2xs font-mono cursor-pointer
                  border-r border-app-border shrink-0 transition-colors select-none
                  ${isActive ? "bg-[var(--app-terminal-bg)] text-app-text border-b-[2px] border-b-app-accent -mb-px"
                    : "text-app-text-muted hover:text-app-text hover:bg-[var(--app-hover)]"}`}
              >
                <span className={`w-1.5 h-1.5 rounded-full shrink-0 ${tab.ptyRunning ? "bg-app-accent shadow-[0_0_4px_var(--app-glow)]" : "bg-app-text-muted opacity-40"}`} />
                <span className="max-w-[100px] truncate">{tab.name}</span>
                {tabs.length > 1 && (
                  <button onClick={(e) => { e.stopPropagation(); setClosingTabId(tab.id); }}
                    className={`ml-0.5 p-0.5 hover:bg-[var(--app-hover)] transition-colors shrink-0
                      ${isActive ? "" : "opacity-0 group-hover:opacity-100"}`} title="关闭">
                    <X size={10} />
                  </button>
                )}
              </div>
            );
          })}
          <button onClick={onNewTab} className="px-2.5 h-[28px] text-app-text-muted hover:text-app-text hover:bg-[var(--app-hover)] transition-colors shrink-0" title="新建终端">
            <Plus size={13} />
          </button>
        </div>

        {/* History button */}
        <div className="relative shrink-0">
          <button onClick={() => setShowHistory(!showHistory)}
            className={`px-2 h-[28px] text-app-text-muted hover:text-app-text transition-colors flex items-center gap-1 ${showHistory ? "text-app-accent" : ""}`}
            title="历史会话"
          >
            <Clock size={12} />
            {history.length > 0 && (
              <span className="text-2xs tabular-nums">{history.length}</span>
            )}
          </button>

          {/* History dropdown */}
          {showHistory && (
            <>
              <div className="fixed inset-0 z-40" onClick={() => setShowHistory(false)} />
              <div className="absolute right-0 top-[28px] z-50 w-[380px] max-h-[400px] overflow-y-auto
                bg-[var(--app-panel)] border border-app-border shadow-dialog">
                <div className="px-3 py-2 border-b border-app-border bg-[var(--app-subtle)] space-y-1.5">
                  <div className="flex items-center justify-between">
                    <span className="text-2xs text-app-text-muted uppercase tracking-wider">历史会话</span>
                    <div className="flex items-center gap-2">
                      <span className="text-2xs text-app-text-muted">({filteredHistory.length}/{history.length})</span>
                      {history.length > 0 && (
                        <button
                          onClick={() => { if (window.confirm("确定要清空所有历史会话记录吗？")) onClearHistory(); }}
                          className="text-2xs text-app-text-dim hover:text-app-red transition-colors"
                          title="清空全部历史"
                        >
                          清空
                        </button>
                      )}
                    </div>
                  </div>
                  <input
                    type="text"
                    value={historySearch}
                    onChange={(e) => setHistorySearch(e.target.value)}
                    placeholder="搜索..."
                    className="w-full h-[22px] text-2xs font-mono bg-[var(--app-input)] border border-app-border px-2 py-0 focus:border-app-accent"
                    spellCheck={false}
                  />
                </div>
                {filteredHistory.length === 0 ? (
                  <div className="px-4 py-6 text-center text-xs text-app-text-muted font-mono">
                    暂无历史记录
                  </div>
                ) : (
                  filteredHistory.map((r) => (
                    <div key={r.id}
                      className="px-3 py-2 border-b border-app-border-light hover:bg-[var(--app-hover)] group/h transition-colors"
                    >
                      {/* Command + directory */}
                      <div className="min-w-0 mb-1.5">
                        {r.workDir && (
                          <div className="text-2xs text-app-text-muted font-mono flex items-center gap-1 mb-0.5">
                            <FolderOpen size={9} />
                            {shortenPath(r.workDir)}
                          </div>
                        )}
                        <code className="text-xs text-app-text font-mono block truncate">
                          <span className="text-app-accent opacity-70">$ </span>{r.command}
                        </code>
                        <div className="text-2xs text-app-text-muted mt-0.5">{formatTime(r.timestamp)}</div>
                      </div>

                      {/* Actions */}
                      <div className="flex items-center gap-1">
                        <button
                          onClick={() => { onNewSessionFromHistory(r); setShowHistory(false); }}
                          className="flex items-center gap-1 px-2 py-0.5 text-2xs text-app-text-dim
                            hover:text-app-green border border-app-border hover:border-[var(--app-accent)]
                            bg-[var(--app-input)] hover:bg-[var(--app-hover)] transition-colors font-mono"
                        >
                          <Play size={10} />新会话
                        </button>
                        {r.resumeLastCommand && (
                          <button
                            onClick={() => { onResumeSession({ ...r, resumeCommand: r.resumeLastCommand }); setShowHistory(false); }}
                            className="flex items-center gap-1 px-2 py-0.5 text-2xs text-app-green
                              hover:text-[var(--app-accent)] border border-app-border
                              hover:border-app-accent bg-[var(--app-input)]
                              hover:bg-[var(--app-hover)] transition-colors font-mono"
                            title={`恢复最近: ${r.resumeLastCommand}`}
                          >
                            <Clock size={10} />恢复最近
                          </button>
                        )}
                        {r.resumeCommand && (
                          <button
                            onClick={() => { onResumeSession(r); setShowHistory(false); }}
                            className="flex items-center gap-1 px-2 py-0.5 text-2xs text-app-amber
                              hover:text-[var(--app-amber-glow)] border border-app-border
                              hover:border-app-amber bg-[var(--app-input)]
                              hover:bg-[var(--app-hover)] transition-colors font-mono"
                            title={`恢复: ${r.resumeCommand}`}
                          >
                            <Clock size={10} />恢复会话
                          </button>
                        )}
                        <div className="flex-1" />
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            if (window.confirm(`确定要删除此会话记录吗？\n\n${r.command}`)) {
                              onDeleteHistory(r.id);
                            }
                          }}
                          className="p-1 text-app-text-dim hover:text-app-red hover:bg-app-red-bg transition-colors shrink-0"
                          title="删除"
                        >
                          <Trash2 size={11} />
                        </button>
                      </div>
                    </div>
                  ))
                )}
              </div>
            </>
          )}
        </div>

        {/* Work dir + close */}
        <div className="flex items-center gap-1 px-2 shrink-0">
          <input type="text" value={activeTab?.workDir || ""}
            onChange={(e) => activeTabId && onSetWorkDir(activeTabId, e.target.value)}
            placeholder="工作目录"
            className="w-[160px] h-[20px] bg-[var(--app-input)] border border-app-border text-2xs font-mono text-app-text-dim px-1.5 py-0 focus:border-app-accent"
            spellCheck={false} />
          <button onClick={browseDir} className="p-0.5 text-app-text-muted hover:text-app-accent transition-colors" title="选择目录">
            <FolderOpen size={11} />
          </button>
          {/* Font size */}
          <button onClick={() => onSetFontSize(fontSize - 1)}
            className="px-1 h-[20px] text-2xs text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)] transition-colors font-mono"
            title="缩小字体">A⁻</button>
          <span className="text-2xs text-app-text-muted tabular-nums w-5 text-center">{fontSize}</span>
          <button onClick={() => onSetFontSize(fontSize + 1)}
            className="px-1 h-[20px] text-2xs text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)] transition-colors font-mono"
            title="放大字体">A⁺</button>
          {/* Pop-out */}
          <button
            onClick={() => invoke("new_window").catch(() => {})}
            className="p-0.5 text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)] transition-colors"
            title="弹出为独立窗口"
          >
            <ExternalLink size={12} />
          </button>
          {/* Maximize / Restore */}
          {onToggleMaximize && (
            <button
              onClick={onToggleMaximize}
              className="p-0.5 text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)] transition-colors"
              title={maximized ? "还原" : "最大化"}
            >
              {maximized ? <Minimize2 size={12} /> : <Maximize2 size={12} />}
            </button>
          )}
          <button
            onClick={() => {
              if (runningCount > 0) {
                setClosingPanel(true);
              } else {
                onClose();
              }
            }}
            className="p-0.5 text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)] transition-colors ml-1"
            title="关闭面板"
          >
            <X size={13} />
          </button>
        </div>
      </div>

      {/* Terminal area — all tabs mounted, inactive hidden via visibility (not display:none) */}
      <div className="flex-1 relative bg-[var(--app-terminal-bg)]">
        {tabs.map((tab) => {
          const isActive = tab.id === activeTabId;
          return (
            <div
              key={`${tab.id}-v${terminalVersion}`}
              className="absolute inset-0"
              style={{
                visibility: isActive ? "visible" : "hidden",
                pointerEvents: isActive ? "auto" : "none",
              }}
            >
              <TabTerminal tabId={tab.id} onAttach={(term) => onAttachTerminal(tab.id, term)}
                onReady={onTerminalReady ? () => onTerminalReady(tab.id) : undefined}
                onResize={onTerminalResize ? (cols, rows) => onTerminalResize(tab.id, cols, rows) : undefined}
                fontSize={fontSize} />
            </div>
          );
        })}
      </div>

      <ConfirmDialog
        open={!!closingTabId}
        title="关闭终端"
        message="确定要关闭此终端标签吗？未保存的输出将丢失。"
        confirmLabel="关闭"
        onConfirm={() => {
          if (closingTabId) { onCloseTab(closingTabId); setClosingTabId(null); }
        }}
        onCancel={() => setClosingTabId(null)}
      />

      <ConfirmDialog
        open={closingPanel}
        title="关闭终端面板"
        message={`有 ${runningCount} 个终端会话正在运行，关闭面板将终止所有终端。确定要关闭吗？`}
        confirmLabel="关闭"
        onConfirm={() => {
          setClosingPanel(false);
          onClose();
        }}
        onCancel={() => setClosingPanel(false)}
      />
    </div>
  );
}
