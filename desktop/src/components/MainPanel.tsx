import React, { useState, useCallback, useRef, useEffect } from "react";
import { EnvVarTable } from "./EnvVarTable";
import { Badge } from "./common/Badge";
import { Star, Copy, Check, Terminal, Play, Pencil, FlaskConical, Tag, X, Clock, FolderOpen, Trash2 } from "lucide-react";
import type { ProfileDetail } from "../lib/types";
import { parseAiCmd } from "../hooks/useTerminal";
import type { SessionRecord } from "../hooks/useTerminal";
import { shortenPath } from "../lib/path-utils";
import { formatShortcut } from "../utils/shortcut";
import { ConfirmDialog } from "./ConfirmDialog";
import { OnboardingWizard } from "./OnboardingWizard";

interface MainPanelProps {
  profile: ProfileDetail | null;
  hasProfiles: boolean;
  showWelcome: boolean;
  allTags: string[];
  history: SessionRecord[];
  onSetEnv: (key: string, value: string) => Promise<void>;
  onDeleteEnv: (key: string) => Promise<void>;
  onPasteCommand: (command: string) => void;
  onRenameProfile: (name: string) => void;
  onResumeSession: (record: SessionRecord) => void;
  onNewSessionFromHistory: (record: SessionRecord) => void;
  onDeleteHistory: (id: string) => void;
  onClearProfileHistory: (profileName: string) => void;
  onInit: () => void;
  onSetTags: (name: string, tags: string) => Promise<void>;
  onAdd: () => void;
}

/* ── Helpers ─────────────────────────────────────────────── */
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

/* ── Tool name mapping ──────────────────────────────────── */
const TOOL_DISPLAY_NAMES: Record<string, string> = {
  claude: "Claude Code",
  codex: "Codex CLI",
  gemini: "Gemini CLI",
  qoderclicn: "Qoder CLI (国内版)",
};

/* ── Detect tool from env vars ──────────────────────────── */
function detectTools(env: Record<string, string>): string | null {
  // Explicit stored type takes priority
  if (env._KN_CLI_TYPE && env._KN_CLI_TYPE !== "both") return env._KN_CLI_TYPE;
  // Heuristic fallback
  const keys = Object.keys(env).map((k) => k.toUpperCase());
  if (keys.some((k) => k === "GEMINI_API_KEY" || k.startsWith("GOOGLE_CLOUD_"))) return "gemini";
  // Qoder uses OPENAI_API_KEY + OPENAI_BASE_URL; distinguish by dashscope endpoint
  if (env.OPENAI_BASE_URL?.includes("dashscope")) return "qoderclicn";
  if (keys.some((k) => k.startsWith("ANTHROPIC_"))) return "claude";
  if (keys.some((k) => k.startsWith("OPENAI_") || k.startsWith("OPENROUTER_"))) return "codex";
  return null;
}

function buildCommands(name: string, env: Record<string, string>): { label: string; cmd: string }[] {
  const toolId = detectTools(env);
  const cmds: { label: string; cmd: string }[] = [];
  if (toolId) {
    const displayName = TOOL_DISPLAY_NAMES[toolId] ?? toolId;
    cmds.push({ label: displayName, cmd: `ai ${toolId} ${name}` });
  }
  return cmds;
}

/* ── Copy helper ────────────────────────────────────────── */
function useCopy() {
  const [copied, setCopied] = useState<string | null>(null);
  const copy = useCallback(async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(text);
      setTimeout(() => setCopied(null), 1800);
    } catch {
      const ta = document.createElement("textarea");
      ta.value = text;
      ta.style.position = "fixed";
      ta.style.opacity = "0";
      document.body.appendChild(ta);
      ta.select();
      document.execCommand("copy");
      document.body.removeChild(ta);
      setCopied(text);
      setTimeout(() => setCopied(null), 1800);
    }
  }, []);
  return { copied, copy };
}

/* ── EmptyState ──────────────────────────────────────── */
function EmptyState({ hasProfiles, onInit }: { hasProfiles: boolean; onInit: () => void }) {
  if (!hasProfiles) {
    return (
      <div className="flex-1 flex items-center justify-center bg-app-bg">
        <div className="flex flex-col items-center gap-5 text-center max-w-sm px-4">
          <div className="w-16 h-16 rounded-full bg-[var(--app-selected)] flex items-center justify-center">
            <Terminal size={32} className="text-app-accent" />
          </div>
          <div>
            <div className="text-lg text-app-text font-mono font-semibold mb-1">AI Profile Manager</div>
            <div className="text-sm text-app-text-dim leading-relaxed">
              管理多个 AI CLI 工具的 API 配置，一键切换服务商
            </div>
          </div>
          <div className="text-xs text-app-text-dim font-mono text-left space-y-1.5 bg-[var(--app-cmd-bg)] border border-app-border p-3 w-full">
            <div className="text-app-text-muted">快速开始：</div>
            <div>1. 按 <kbd className="text-app-amber">{formatShortcut("mod+N")}</kbd> 创建第一个 profile</div>
            <div>2. 填入 API 密钥和地址</div>
            <div>3. 点击运行，选择项目目录</div>
          </div>
          {/* Scan button — prominent CTA with glow animation + tip badge */}
          <div className="relative">
            <span className="absolute -top-1.5 -right-1.5 z-10 px-2 py-0.5 text-[10px] font-mono font-bold
              bg-app-accent text-[var(--app-bg)] onboarding-tip-badge">
              推荐
            </span>
            <button onClick={onInit}
              className="text-sm text-app-text font-mono font-semibold transition-colors border-2 border-app-accent
                bg-[var(--app-selected)] px-4 py-2.5 hover:bg-[var(--app-active)]
                onboarding-scan-btn w-full">
              <span className="text-app-accent opacity-70 mr-1">$</span>
              扫描系统配置 (Claude / Codex)
            </button>
          </div>
          <div className="text-2xs text-app-text-muted mt-2">
            点击 Toolbar 右侧 <kbd>?</kbd> 可随时重新打开此引导
          </div>

          {/* Shortcuts */}
          <div className="text-left border border-app-border bg-[var(--app-cmd-bg)] w-full mt-2">
            <div className="px-3 py-1 border-b border-app-border bg-[var(--app-cmd-header)]">
              <span className="text-2xs text-app-text-muted uppercase tracking-wider">快捷键</span>
            </div>
            <div className="px-3 py-1.5 space-y-0.5 text-2xs font-mono">
              {[
                [formatShortcut("mod+N"), "新建 Profile"], ["Esc", "关闭弹窗 / 取消选中"], [formatShortcut("mod+F"), "搜索"],
                ["Ctrl+`", "开关终端"], [formatShortcut("mod+K"), "快捷键帮助"], ["↑↓", "终端历史命令"],
              ].map(([key, desc]) => (
                <div key={key} className="flex justify-between">
                  <span className="text-app-text-muted">{desc}</span>
                  <kbd className="text-app-text-dim">{key}</kbd>
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>
    );
  }
  return (
    <div className="flex-1 flex items-center justify-center bg-app-bg">
      <div className="flex flex-col items-center gap-4 text-center">
        <Terminal size={36} className="text-app-text-muted opacity-30" />
        <div>
          <div className="text-base text-app-text-dim">
            <span className="text-app-text-muted">$ </span>
            <span className="animate-cursor-blink">_</span>
          </div>
          <div className="text-sm text-app-text-muted mt-2">
            从侧边栏选择 profile，或按 <kbd className="text-app-amber">{formatShortcut("mod+N")}</kbd> 新建
          </div>
        </div>
      </div>
    </div>
  );
}

/* ── CommandBlock ────────────────────────────────────── */
function CommandBlock({
  commands,
  profileName,
  onPaste,
}: {
  commands: { label: string; cmd: string }[];
  profileName: string;
  onPaste: (cmd: string) => void;
}) {
  const { copied, copy } = useCopy();

  // Extract tool ID from first command for breakdown display
  const toolId = commands.length > 0 ? commands[0].cmd.split(/\s+/)[1] ?? null : null;
  const toolDisplayName = toolId ? (TOOL_DISPLAY_NAMES[toolId] ?? toolId) : null;

  return (
    <div className="mt-3 bg-[var(--app-cmd-bg)] select-none border-y border-app-border">
      <div className="flex items-center justify-between px-3 py-2 border-b border-app-border bg-[var(--app-cmd-header)]">
        <div className="flex items-center gap-2 text-xs">
          <Terminal size={13} className="text-app-accent opacity-60" />
          <span className="tracking-wider uppercase text-2xs text-app-text-muted">使用命令</span>
        </div>
        <span className="text-2xs text-app-text-muted">— {profileName}</span>
      </div>

      <div className="px-3 py-2 space-y-1">
        {commands.map(({ label, cmd }) => {
          const isCopied = copied === cmd;
          return (
            <div key={cmd}
              className="flex items-center justify-between group/item px-2 py-1.5
                hover:bg-[var(--app-hover)] transition-colors duration-fast"
            >
              <div className="min-w-0 flex items-center gap-2">
                <span className="text-2xs text-app-text-muted shrink-0">{label}</span>
                <code className="text-sm text-app-text font-mono select-all truncate">
                  <span className="text-app-accent opacity-70 mr-1">$</span>
                  {cmd}
                </code>
              </div>
              <div className="flex items-center gap-0.5 shrink-0 ml-2">
                <button
                  onClick={() => onPaste(cmd)}
                  className="flex items-center gap-1 px-2 py-0.5 text-2xs
                    text-app-text-dim hover:text-app-green
                    border border-transparent hover:border-app-border
                    bg-transparent hover:bg-[var(--app-hover)]"
                  title="在终端中运行"
                >
                  <Play size={11} />
                  <span>运行</span>
                </button>
                <button
                  onClick={() => copy(cmd)}
                  className="flex items-center gap-1 px-2 py-0.5 text-2xs
                    text-app-text-dim hover:text-app-accent
                    border border-transparent hover:border-app-border
                    bg-transparent hover:bg-[var(--app-hover)]"
                  title="复制到剪贴板"
                >
                  {isCopied ? (
                    <><Check size={11} className="text-app-green" /><span className="text-app-green">已复制</span></>
                  ) : (
                    <><Copy size={11} /><span>复制</span></>
                  )}
                </button>
              </div>
            </div>
          );
        })}
      </div>

      {/* Command breakdown — inline annotation style */}
      {toolId && (
        <div className="border-t border-app-border bg-[var(--app-subtle)]">
          <div className="flex items-center gap-2 px-3 py-1.5 border-b border-app-border bg-[var(--app-cmd-header)]">
            <span className="text-2xs text-app-text-muted font-mono uppercase tracking-wider">命令拆解</span>
          </div>
          <div className="px-4 py-3">
            {/* The command with color-coded parts */}
            <code className="text-sm text-app-text font-mono block mb-2.5">
              <span className="text-app-accent opacity-70">$ </span>
              <span className="text-app-accent">ai</span>
              {" "}
              <span className="text-app-green">{toolId}</span>
              {" "}
              <span className="text-app-amber">{profileName}</span>
            </code>
            {/* Tree annotations */}
            <div className="space-y-1 font-mono text-2xs text-app-text-dim leading-relaxed">
              <div className="flex items-center gap-1.5">
                <span className="text-app-text-muted shrink-0 select-none">├─</span>
                <span className="inline-flex items-center px-1.5 py-px text-[10px] font-mono
                  bg-[var(--app-selected)] text-app-accent border border-app-accent/20 shrink-0">
                  shell 函数
                </span>
                <span>注入 profile 环境变量后启动 AI 工具</span>
              </div>
              <div className="flex items-center gap-1.5">
                <span className="text-app-text-muted shrink-0 select-none">├─</span>
                <span className="inline-flex items-center px-1.5 py-px text-[10px] font-mono
                  bg-[var(--app-selected)] text-app-green border border-app-green/20 shrink-0">
                  AI 工具
                </span>
                <span>{toolDisplayName}</span>
              </div>
              <div className="flex items-center gap-1.5">
                <span className="text-app-text-muted shrink-0 select-none">╰─</span>
                <span className="inline-flex items-center px-1.5 py-px text-[10px] font-mono
                  bg-[var(--app-selected)] text-app-amber border border-app-amber/20 shrink-0">
                  Profile 名
                </span>
                <span>加载此 profile 的环境变量（API Key、Base URL 等）</span>
              </div>
            </div>
          </div>
        </div>
      )}

      <div className="px-3 py-1.5 border-t border-app-border bg-[var(--app-cmd-header)]">
        <span className="text-2xs text-app-text-muted">
          复制在终端中使用，或点击运行在内置终端中执行
        </span>
      </div>
    </div>
  );
}

/* ── MainPanel ──────────────────────────────────────────── */
/* ── Tags row (display + chip-based editing) ───────────── */
function TagsRow({ profile, allTags, onSetTags }: { profile: ProfileDetail; allTags: string[]; onSetTags: (name: string, tags: string) => Promise<void> }) {
  const [editing, setEditing] = useState(false);
  const [input, setInput] = useState("");
  const [newTags, setNewTags] = useState<string[]>([]);
  const inputRef = useRef<HTMLInputElement>(null);

  const savedTags = profile.tags || [];
  const otherTags = allTags.filter((t) => !savedTags.includes(t));

  const startEdit = () => {
    setNewTags([...savedTags]);
    setInput("");
    setEditing(true);
    setTimeout(() => inputRef.current?.focus(), 50);
  };

  const addTag = () => {
    const t = input.trim();
    if (t && !newTags.includes(t) && newTags.length < 3) {
      setNewTags([...newTags, t]);
      setInput("");
      // Keep focus on input for continuous typing
      setTimeout(() => inputRef.current?.focus(), 10);
    }
  };

  const removeTag = (t: string) => setNewTags(newTags.filter((x) => x !== t));
  const reAddTag = (t: string) => {
    if (!newTags.includes(t) && newTags.length < 3) setNewTags([...newTags, t]);
  };

  const save = async () => {
    // Flush pending input before saving
    const t = input.trim();
    const final = t && !newTags.includes(t) && newTags.length < 3
      ? [...newTags, t]
      : newTags;
    await onSetTags(profile.name, final.join(","));
    setNewTags([]);
    setInput("");
    setEditing(false);
  };

  const cancel = () => {
    setNewTags([]);
    setInput("");
    setEditing(false);
  };

  return (
    <div className="ml-3 mt-1">
      <div className="flex items-center gap-1.5 flex-wrap">
        <Tag size={11} className="text-app-text-muted shrink-0" />
        {editing ? (
          <>
            {/* Input + tag chips */}
            <div className="flex items-center gap-1 flex-wrap">
              {newTags.map((t) => (
                <span key={t} className="flex items-center gap-0.5 text-2xs px-1.5 py-0.5 font-mono bg-[var(--app-selected)] text-app-accent border border-app-accent/30">
                  {t}
                  <button onClick={() => removeTag(t)} className="text-app-text-dim hover:text-app-red"><X size={9} /></button>
                </span>
              ))}
              {newTags.length < 3 ? (
                <input
                  ref={inputRef}
                  value={input}
                  onChange={(e) => setInput(e.target.value)}
                  onKeyDown={(e) => { if (e.key === "Enter") { e.preventDefault(); addTag(); } }}
                  className="h-[22px] w-[100px] text-xs font-mono bg-[var(--app-input)] border border-app-accent px-1.5"
                  placeholder="输入后回车确认"
                />
              ) : (
                <span className="text-2xs text-app-text-muted font-mono">最多 3 个标签</span>
              )}
            </div>
            <button onClick={save} className="p-0.5 text-app-accent hover:bg-[var(--app-hover)]" title="保存"><Check size={11} /></button>
            <button onClick={cancel} className="p-0.5 text-app-text-dim hover:bg-[var(--app-hover)]" title="取消"><X size={11} /></button>
          </>
        ) : (
          <>
            {savedTags.length > 0 ? savedTags.map((t) => (
              <span key={t} className="text-2xs px-1.5 py-0.5 font-mono bg-[var(--app-input)] text-app-text-dim border border-app-border">{t}</span>
            )) : (
              <span className="text-2xs text-app-text-muted font-mono">无标签</span>
            )}
            <button onClick={startEdit} className="p-0.5 text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)]" title="编辑标签">
              <Pencil size={10} />
            </button>
          </>
        )}
      </div>

      {/* All saved tags as suggestions */}
      {editing && allTags.length > 0 && (
        <div className="flex items-center gap-1 mt-1.5 ml-5 flex-wrap">
          <span className="text-2xs text-app-text-muted font-mono shrink-0">已有标签:</span>
          {allTags.map((t) => (
            <button
              key={t}
              onClick={() => reAddTag(t)}
              disabled={newTags.includes(t)}
              className={`text-2xs px-1.5 py-0.5 font-mono border transition-colors ${
                newTags.includes(t)
                  ? "opacity-30 cursor-not-allowed border-app-border text-app-text-muted"
                  : "border-app-border text-app-text-dim hover:text-app-accent hover:border-app-accent bg-[var(--app-input)]"
              }`}
            >{t}</button>
          ))}
        </div>
      )}
    </div>
  );
}

/* ── MainPanel ──────────────────────────────────────────── */
export function MainPanel({ profile, hasProfiles, showWelcome, allTags, history, onSetEnv, onDeleteEnv, onPasteCommand, onRenameProfile, onResumeSession, onNewSessionFromHistory, onDeleteHistory, onClearProfileHistory, onInit, onSetTags, onAdd }: MainPanelProps) {
  if (showWelcome) return (
    <OnboardingWizard
      hasProfiles={hasProfiles}
      onScan={onInit}
      onCreate={onAdd}
      onDismiss={() => {
        // Toggle welcome off — the caller handles this via onToggleWelcome
        // We dispatch a custom event since the dismiss callback comes from App
        window.dispatchEvent(new CustomEvent("kn-dismiss-welcome"));
      }}
    />
  );
  if (!profile) return <EmptyState hasProfiles={hasProfiles} onInit={onInit} />;

  const envCount = Object.keys(profile.env).length;
  const commands = buildCommands(profile.name, profile.env);

  // Confirm dialog states for history deletion
  const [clearConfirm, setClearConfirm] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<SessionRecord | null>(null);

  // Filter history for this profile
  const profileHistory = history.filter((r) => {
    const parsed = parseAiCmd(r.command);
    return parsed?.profile === profile.name;
  });

  return (
    <div className="flex-1 flex flex-col bg-app-bg overflow-hidden">
      <div className="px-4 py-3 bg-app-panel border-b border-app-border shrink-0">
        <div className="flex items-center gap-2 mb-0.5">
          <h2 className="text-xl font-semibold text-app-text tracking-tight">
            <span className="text-app-accent opacity-50">$ </span>
            {profile.name}
          </h2>
          <button
            onClick={() => onRenameProfile(profile.name)}
            className="p-1 text-app-text-dim hover:text-app-accent hover:bg-[var(--app-hover)] transition-colors ml-1"
            title="重命名"
          >
            <Pencil size={13} />
          </button>
          {profile.is_default && (
            <Badge variant="primary">
              <Star size={10} className="fill-current" />默认
            </Badge>
          )}
        </div>
        {profile.desc && (
          <p className="text-sm text-app-text-dim mt-0.5 ml-3">{profile.desc}</p>
        )}
        <div className="flex items-center gap-2 mt-2 ml-3">
          <span className="text-xs text-app-text-muted">{envCount} 个环境变量</span>
        </div>
        {/* Tags */}
        <TagsRow profile={profile} allTags={allTags} onSetTags={onSetTags} />
      </div>

      <CommandBlock commands={commands} profileName={profile.name}
        onPaste={onPasteCommand} />

      {/* History + Env table — share remaining space */}
      <div className="flex-1 flex flex-col min-h-0 mt-3">
        {profileHistory.length > 0 && (
          <div className="flex flex-col flex-1 min-h-0 border-y border-app-border bg-[var(--app-cmd-bg)]">
            <div className="flex items-center gap-2 px-3 py-1.5 border-b border-app-border bg-[var(--app-cmd-header)] shrink-0">
              <Clock size={11} className="text-app-text-muted" />
              <span className="text-2xs text-app-text-muted font-mono uppercase tracking-wider">会话历史</span>
              <span className="text-2xs text-app-text-dim font-mono">({profileHistory.length})</span>
              <span className="flex-1" />
              <button
                onClick={() => setClearConfirm(true)}
                className="text-2xs text-app-text-dim hover:text-app-red transition-colors font-mono"
                title="清除此 profile 的全部历史"
              >
                清除
              </button>
            </div>
            <div className="flex-1 overflow-y-auto">
              {profileHistory.slice(0, 50).map((r) => (
                <div key={r.id} className="px-3 py-2 border-b border-app-border-light hover:bg-[var(--app-hover)] transition-colors">
                  <div className="flex items-center justify-between min-w-0">
                    <div className="min-w-0 flex-1">
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
                    <div className="flex items-center gap-1 shrink-0 ml-3">
                      {r.resumeLastCommand && (
                        <button
                          onClick={() => onResumeSession({ ...r, resumeCommand: r.resumeLastCommand })}
                          className="flex items-center gap-1 px-1.5 py-0.5 text-2xs text-app-green
                            hover:bg-[var(--app-hover)] border border-transparent hover:border-app-border transition-colors font-mono"
                          title="恢复最近会话"
                        >
                          <Clock size={10} />最近
                        </button>
                      )}
                      {r.resumeCommand && (
                        <button
                          onClick={() => onResumeSession(r)}
                          className="flex items-center gap-1 px-1.5 py-0.5 text-2xs text-app-amber
                            hover:bg-[var(--app-hover)] border border-transparent hover:border-app-border transition-colors font-mono"
                            title="恢复此会话"
                          >
                            <Play size={10} />恢复
                          </button>
                        )}
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          setDeleteTarget(r);
                        }}
                        className="p-0.5 text-app-text-dim hover:text-app-red hover:bg-app-red-bg transition-colors"
                        title="删除此记录"
                      >
                        <Trash2 size={11} />
                      </button>
                    </div>
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}
        <div className="flex-1 min-h-0 overflow-y-auto">
          <EnvVarTable
            env={profile.env}
            onSet={async (key, value) => onSetEnv(key, value)}
            onDelete={async (key) => onDeleteEnv(key)}
          />
        </div>
      </div>

      {/* Clear profile history confirm */}
      <ConfirmDialog
        open={clearConfirm}
        title="清除会话历史"
        message={`确定要清除 "${profile.name}" 的全部 ${profileHistory.length} 条会话历史吗？此操作不可撤销。`}
        confirmLabel="清除"
        onConfirm={() => {
          onClearProfileHistory(profile.name);
          setClearConfirm(false);
        }}
        onCancel={() => setClearConfirm(false)}
      />

      {/* Delete single record confirm */}
      <ConfirmDialog
        open={!!deleteTarget}
        title="删除会话记录"
        message={`确定要删除此会话记录吗？\n\n${deleteTarget?.command || ""}`}
        confirmLabel="删除"
        onConfirm={() => {
          if (deleteTarget) {
            onDeleteHistory(deleteTarget.id);
            setDeleteTarget(null);
          }
        }}
        onCancel={() => setDeleteTarget(null)}
      />
    </div>
  );
}
