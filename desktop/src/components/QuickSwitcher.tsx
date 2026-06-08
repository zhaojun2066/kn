import React, { useState, useEffect, useRef, useMemo, useCallback } from "react";
import { CLIIcon } from "./common/CLIIcon";
import { open as tauriOpen } from "@tauri-apps/plugin-dialog";
import type { ProfileSummary, ProjectInfo } from "../lib/types";
import type { SessionRecord } from "../hooks/useTerminal";

// ── Types ────────────────────────────────────────────────────

type QuickSwitcherMode = "profile" | "history";
type ProfileStep = "profile" | "project";

interface QuickSwitcherProps {
  open: boolean;
  mode: QuickSwitcherMode;
  onClose: () => void;
  // Profile mode
  profiles: ProfileSummary[];
  projects: ProjectInfo[];
  onLaunchProfile: (name: string, command: string, workDir: string) => void;
  // History mode
  history: SessionRecord[];
  onResumeSession: (record: SessionRecord) => void;
}

// ── Fuzzy search ─────────────────────────────────────────────

interface FuzzyResult {
  matches: boolean;
  score: number;
}

function fuzzyMatch(query: string, target: string): FuzzyResult {
  if (!query) return { matches: true, score: 0 };

  const q = query.toLowerCase();
  const t = target.toLowerCase();

  // Exact substring match → highest score
  if (t.includes(q)) return { matches: true, score: 1000 };

  let qi = 0;
  let score = 0;
  let consecutive = 0;

  for (let ti = 0; ti < t.length && qi < q.length; ti++) {
    if (t[ti] === q[qi]) {
      consecutive++;
      // Word-boundary bonus
      const boundaryBonus = ti === 0 || /[\s\-_.]/.test(t[ti - 1]) ? 50 : 0;
      // Earlier matches score higher; consecutive chars compound
      score += 200 - ti + consecutive * 15 + boundaryBonus;
      qi++;
    } else {
      consecutive = 0;
    }
  }

  return { matches: qi === q.length, score };
}

// ── Profile command builder ──────────────────────────────────

function profileCommand(name: string, cliType?: string): string | null {
  if (!cliType || cliType === "both") return null;
  const tool =
    cliType === "anthropic" ? "claude" :
    cliType === "openai" ? "codex" :
    cliType;
  if (["claude", "codex", "qoderclicn"].includes(tool)) {
    return `ai ${tool} ${name}`;
  }
  return null;
}

// ── Time formatter ───────────────────────────────────────────

function relativeTime(ts: number): string {
  const sec = Math.floor((Date.now() - ts) / 1000);
  if (sec < 60) return "刚刚";
  if (sec < 3600) return `${Math.floor(sec / 60)} 分钟前`;
  if (sec < 86400) return `${Math.floor(sec / 3600)} 小时前`;
  if (sec < 604800) return `${Math.floor(sec / 86400)} 天前`;
  return new Date(ts).toLocaleDateString("zh-CN");
}

// ── Component ────────────────────────────────────────────────

export const QuickSwitcher = React.memo(function QuickSwitcher({
  open,
  mode,
  onClose,
  profiles,
  projects,
  onLaunchProfile,
  history,
  onResumeSession,
}: QuickSwitcherProps) {
  const [searchQuery, setSearchQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);

  // Profile mode: two-step flow
  const [profileStep, setProfileStep] = useState<ProfileStep>("profile");
  const [selectedProfile, setSelectedProfile] = useState<{
    name: string;
    command: string;
  } | null>(null);

  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  // ── Reset on open / mode change ──────────────────────────
  useEffect(() => {
    if (open) {
      setSearchQuery("");
      setActiveIndex(0);
      setProfileStep("profile");
      setSelectedProfile(null);
      // Auto-focus search input
      requestAnimationFrame(() => {
        inputRef.current?.focus();
      });
    }
  }, [open, mode]);

  // ── Filtered & sorted results ────────────────────────────

  // Profile step: fuzzy-match profiles
  const filteredProfiles = useMemo(() => {
    return profiles
      .map((p) => {
        const cmd = profileCommand(p.name, p.cli_type);
        const result = fuzzyMatch(searchQuery, p.name + " " + (p.desc || ""));
        return { ...p, cmd, ...result };
      })
      .filter((p) => p.matches)
      .sort((a, b) => b.score - a.score);
  }, [profiles, searchQuery]);

  // Project step: fuzzy-match projects (always show "browse" option at end)
  const filteredProjects = useMemo(() => {
    if (searchQuery) {
      return projects
        .map((p) => {
          const result = fuzzyMatch(searchQuery, p.name + " " + p.path);
          return { ...p, ...result };
        })
        .filter((p) => p.matches)
        .sort((a, b) => b.score - a.score);
    }
    return projects.map((p) => ({ ...p, matches: true, score: 0 }));
  }, [projects, searchQuery]);

  // History mode: fuzzy-match session records
  const filteredHistory = useMemo(() => {
    return history
      .map((r) => {
        const query = searchQuery;
        const text = r.command + " " + (r.label || "") + " " + (r.workDir || "");
        const result = fuzzyMatch(query, text);
        return { ...r, ...result };
      })
      .filter((r) => r.matches)
      .sort((a, b) => b.score - a.score);
  }, [history, searchQuery]);

  // ── Clamp activeIndex ─────────────────────────────────────

  const itemCount =
    mode === "history"
      ? filteredHistory.length
      : profileStep === "profile"
        ? filteredProfiles.length
        : filteredProjects.length + 1; // +1 for "browse" option

  useEffect(() => {
    if (activeIndex >= itemCount) {
      setActiveIndex(Math.max(0, itemCount - 1));
    }
  }, [activeIndex, itemCount]);

  // ── Scroll selected into view ─────────────────────────────

  useEffect(() => {
    if (!listRef.current) return;
    const selected = listRef.current.querySelector(
      `[data-index="${activeIndex}"]`
    ) as HTMLElement | null;
    if (selected) {
      selected.scrollIntoView({ block: "nearest" });
    }
  }, [activeIndex]);

  // ── Keyboard handler ──────────────────────────────────────

  const handleKeyDown = useCallback(
    async (e: React.KeyboardEvent) => {
      // Cmd+P / Cmd+Shift+P toggle modes (handled globally, but also catch here)
      if ((e.metaKey || e.ctrlKey) && !e.shiftKey && e.key === "p") {
        e.preventDefault();
        return; // let global handler deal with mode switch
      }

      if (e.key === "ArrowDown") {
        e.preventDefault();
        setActiveIndex((prev) => (prev + 1) % itemCount);
        return;
      }
      if (e.key === "ArrowUp") {
        e.preventDefault();
        setActiveIndex((prev) => (prev - 1 + itemCount) % itemCount);
        return;
      }

      if (e.key === "Escape") {
        e.preventDefault();
        if (mode === "profile" && profileStep === "project") {
          // Go back to profile step
          setProfileStep("profile");
          setSelectedProfile(null);
          setActiveIndex(0);
          setSearchQuery("");
        } else {
          onClose();
        }
        return;
      }

      if (e.key === "Enter") {
        e.preventDefault();

        if (mode === "history") {
          // ── History: resume selected session ────────────
          const record = filteredHistory[activeIndex];
          if (record) {
            onResumeSession(record);
            onClose();
          }
          return;
        }

        // ── Profile mode ──────────────────────────────────
        if (profileStep === "profile") {
          // Step 1: select profile → go to project selection
          const prof = filteredProfiles[activeIndex];
          if (!prof || !prof.cmd) return; // grayed out, no cli_type
          setSelectedProfile({ name: prof.name, command: prof.cmd });
          setProfileStep("project");
          setSearchQuery("");
          setActiveIndex(0);
        } else {
          // Step 2: select project → launch
          if (!selectedProfile) return;

          if (activeIndex < filteredProjects.length) {
            // Selected a registered project
            const proj = filteredProjects[activeIndex];
            onLaunchProfile(selectedProfile.name, selectedProfile.command, proj.path);
            onClose();
          } else {
            // "浏览文件夹..." option — open native directory picker
            try {
              const dir = await tauriOpen({
                directory: true,
                multiple: false,
                title: "选择项目工作目录",
              });
              if (dir && typeof dir === "string") {
                onLaunchProfile(selectedProfile.name, selectedProfile.command, dir);
                onClose();
              }
            } catch {
              // user cancelled
            }
          }
        }
      }
    },
    [
      mode,
      profileStep,
      filteredProfiles,
      filteredHistory,
      filteredProjects,
      activeIndex,
      itemCount,
      selectedProfile,
      onLaunchProfile,
      onResumeSession,
      onClose,
    ]
  );

  // ── Don't render when closed ──────────────────────────────

  if (!open) return null;

  // ── Determine title, placeholder, footer ─────────────────

  const isHistory = mode === "history";
  const isProfileStep = mode === "profile" && profileStep === "profile";
  const isProjectStep = mode === "profile" && profileStep === "project";

  const placeholder = isHistory
    ? "搜索会话历史..."
    : isProfileStep
      ? "搜索 profile..."
      : "搜索项目目录...";

  const title = isHistory
    ? "会话历史"
    : isProfileStep
      ? "快速切换 Profile"
      : "选择项目目录";

  const footerHints = isHistory
    ? "↑↓ 导航  ↵ 最近会话  Esc 取消"
    : isProfileStep
      ? "↑↓ 导航  ↵ 选择  Esc 取消"
      : "↑↓ 导航  ↵ 选择  Esc 返回";

  // ── Render result rows ────────────────────────────────────

  function renderProfileRow(p: ProfileSummary & { cmd: string | null; matches: boolean; score: number }, index: number) {
    const canLaunch = p.cmd !== null;
    return (
      <div
        key={p.name}
        data-index={index}
        className={`flex items-center gap-2.5 mx-1 my-px px-2.5 py-1.5 cursor-pointer transition-colors duration-60 border-l-[3px] ${
          index === activeIndex
            ? "bg-[var(--app-selected)] text-[var(--app-text)] border-l-[var(--app-accent)] shadow-[inset_0_0_8px_var(--app-glow)]"
            : "text-[var(--app-text)] border-l-transparent hover:bg-[var(--app-hover)]"
        } ${!canLaunch ? "opacity-50" : ""}`}
        onClick={() => {
          if (!canLaunch) return;
          setSelectedProfile({ name: p.name, command: p.cmd! });
          setProfileStep("project");
          setSearchQuery("");
          setActiveIndex(0);
        }}
      >
        <CLIIcon type={p.cli_type || "other"} size={16} />
        <span className="flex-1 text-sm font-mono truncate">{p.name}</span>
        {p.tags && p.tags.length > 0 && (
          <span className="flex items-center gap-0.5 shrink-0">
            {p.tags.map((tag) => (
              <span
                key={tag}
                className="px-1 py-px text-2xs font-mono bg-[var(--app-input)] text-[var(--app-text-muted)]"
              >
                {tag}
              </span>
            ))}
          </span>
        )}
        <span className="text-2xs text-[var(--app-text-muted)] shrink-0">{p.env_count} 变量</span>
        {!canLaunch && (
          <span className="text-2xs text-[var(--app-amber)] shrink-0">需配置 CLI</span>
        )}
      </div>
    );
  }

  function renderProjectRow(p: ProjectInfo & { matches: boolean; score: number }, index: number) {
    return (
      <div
        key={p.name}
        data-index={index}
        className={`flex items-center gap-2.5 mx-1 my-px px-2.5 py-1.5 cursor-pointer transition-colors duration-60 border-l-[3px] ${
          index === activeIndex
            ? "bg-[var(--app-selected)] text-[var(--app-text)] border-l-[var(--app-accent)] shadow-[inset_0_0_8px_var(--app-glow)]"
            : "text-[var(--app-text)] border-l-transparent hover:bg-[var(--app-hover)]"
        }`}
        onClick={() => {
          if (!selectedProfile) return;
          onLaunchProfile(selectedProfile.name, selectedProfile.command, p.path);
          onClose();
        }}
      >
        <span className="shrink-0 text-sm">📁</span>
        <span className="flex-1 text-sm font-mono truncate">{p.name}</span>
        <span className="text-2xs text-[var(--app-text-muted)] shrink-0 max-w-[200px] truncate font-mono">
          {p.path}
        </span>
      </div>
    );
  }

  function renderHistoryRow(
    r: SessionRecord & { matches: boolean; score: number },
    index: number
  ) {
    const parsedTool = r.tool;
    // Strip "· 恢复" suffix from label — avoids "xxx · 恢复 · 恢复" accumulation
    const displayLabel = (r.label || r.command).replace(/\s*·\s*恢复\s*/g, " ").trim();
    const isResume = !!r.resumeCommand;
    return (
      <div
        key={r.id}
        data-index={index}
        className={`flex items-center gap-2.5 mx-1 my-px px-2.5 py-1.5 cursor-pointer transition-colors duration-60 border-l-[3px] ${
          index === activeIndex
            ? "bg-[var(--app-selected)] text-[var(--app-text)] border-l-[var(--app-accent)] shadow-[inset_0_0_8px_var(--app-glow)]"
            : "text-[var(--app-text)] border-l-transparent hover:bg-[var(--app-hover)]"
        }`}
        onClick={() => {
          onResumeSession(r);
          onClose();
        }}
      >
        {parsedTool ? (
          <CLIIcon type={parsedTool} size={16} />
        ) : (
          <span className="shrink-0 text-sm">💻</span>
        )}
        <span className="flex-1 text-sm font-mono truncate">{displayLabel}</span>
        {isResume && (
          <span className="shrink-0 px-1 py-px text-2xs font-mono bg-[var(--app-accent)]/15 text-[var(--app-accent)]">
            恢复
          </span>
        )}
        {r.workDir && (
          <span className="text-2xs text-[var(--app-text-muted)] shrink-0 max-w-[150px] truncate font-mono">
            {r.workDir.replace(/^.*\//, "") || r.workDir}
          </span>
        )}
        <span className="text-2xs text-[var(--app-text-muted)] shrink-0">
          {relativeTime(r.timestamp)}
        </span>
      </div>
    );
  }

  // ── Render ──────────────────────────────────────────────────

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/40 backdrop-blur-sm"
      style={{ paddingTop: "15vh" }}
      onClick={(e) => {
        // Click backdrop to close
        if (e.target === e.currentTarget) onClose();
      }}
      onKeyDown={handleKeyDown}
    >
      <div
        className="w-[520px] bg-[var(--app-panel)] border border-[var(--app-border)] flex flex-col overflow-hidden"
        style={{
          animation: "scaleIn 150ms ease-out",
          boxShadow: "var(--shadow-dialog)",
        }}
      >
        {/* ── Search input ──────────────────────────────── */}
        <div className="flex items-center gap-2 px-3 h-[36px] border-b border-[var(--app-border)] shrink-0">
          <span className="text-sm shrink-0">
            {isProjectStep ? "📁" : "🔍"}
          </span>
          <input
            ref={inputRef}
            type="text"
            value={searchQuery}
            onChange={(e) => {
              setSearchQuery(e.target.value);
              setActiveIndex(0);
            }}
            placeholder={placeholder}
            className="flex-1 bg-transparent text-sm font-mono text-[var(--app-text)] placeholder:text-[var(--app-text-muted)] outline-none border-none"
            autoComplete="off"
            spellCheck={false}
          />
          {isProjectStep && selectedProfile && (
            <span className="text-2xs text-[var(--app-text-muted)] font-mono shrink-0 truncate max-w-[160px]">
              {selectedProfile.command}
            </span>
          )}
          <span className="text-2xs text-[var(--app-text-muted)] font-mono shrink-0">
            {itemCount} 项
          </span>
        </div>

        {/* ── Result list ────────────────────────────────── */}
        <div
          ref={listRef}
          className="flex-1 overflow-y-auto py-1 max-h-[320px] min-h-[60px]"
        >
          {/* ── Profile step ────────────────────────────── */}
          {isProfileStep &&
            (filteredProfiles.length > 0 ? (
              filteredProfiles.map((p, i) => renderProfileRow(p, i))
            ) : (
              <div className="flex items-center justify-center py-8 text-xs text-[var(--app-text-muted)] font-mono">
                无匹配结果
              </div>
            ))}

          {/* ── Project step ────────────────────────────── */}
          {isProjectStep &&
            (filteredProjects.length > 0 || true ? (
              <>
                {filteredProjects.map((p, i) => renderProjectRow(p, i))}
                {/* "浏览文件夹..." — always present, unfiltered */}
                <div
                  data-index={filteredProjects.length}
                  className={`flex items-center gap-2.5 mx-1 my-px px-2.5 py-1.5 cursor-pointer transition-colors duration-60 border-l-[3px] ${
                    activeIndex === filteredProjects.length
                      ? "bg-[var(--app-selected)] text-[var(--app-text)] border-l-[var(--app-accent)] shadow-[inset_0_0_8px_var(--app-glow)]"
                      : "text-[var(--app-text-dim)] border-l-transparent hover:bg-[var(--app-hover)]"
                  }`}
                  onClick={async () => {
                    if (!selectedProfile) return;
                    try {
                      const dir = await tauriOpen({
                        directory: true,
                        multiple: false,
                        title: "选择项目工作目录",
                      });
                      if (dir && typeof dir === "string") {
                        onLaunchProfile(
                          selectedProfile.name,
                          selectedProfile.command,
                          dir
                        );
                        onClose();
                      }
                    } catch {
                      // user cancelled
                    }
                  }}
                >
                  <span className="shrink-0 text-sm">📂</span>
                  <span className="flex-1 text-sm font-mono">浏览文件夹...</span>
                  <span className="text-2xs text-[var(--app-accent)] shrink-0 font-mono">
                    选择目录
                  </span>
                </div>
              </>
            ) : null)}

          {/* ── History mode ─────────────────────────────── */}
          {isHistory &&
            (filteredHistory.length > 0 ? (
              filteredHistory.map((r, i) => renderHistoryRow(r, i))
            ) : (
              <div className="flex flex-col items-center justify-center py-8 gap-1">
                <span className="text-xs text-[var(--app-text-muted)] font-mono">
                  暂无会话历史
                </span>
                <span className="text-2xs text-[var(--app-text-muted)] font-mono">
                  请先运行 profile
                </span>
              </div>
            ))}
        </div>

        {/* ── Footer ──────────────────────────────────────── */}
        <div className="px-3 py-1.5 border-t border-[var(--app-border)] shrink-0">
          <div className="flex items-center gap-3 text-2xs text-[var(--app-text-muted)] font-mono">
            <span>{footerHints}</span>
            {isProjectStep && (
              <span className="text-[var(--app-accent)]">
                {selectedProfile?.name}
              </span>
            )}
          </div>
        </div>
      </div>
    </div>
  );
});
