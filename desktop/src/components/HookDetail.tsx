import { useState, useEffect, useCallback } from "react";
import { Terminal, Lock, FolderOpen, Play, Ban, Trash2, Info, Pencil, Check, X, FileText, Loader, Zap, ArrowRight, Clock, ChevronDown, ChevronRight, RotateCw, RefreshCw } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "./common/Button";
import { ConfirmDialog } from "./ConfirmDialog";
import { CLI_HEX_COLORS, CLI_LABELS } from "../lib/cli-constants";
import type { CliKind } from "../lib/types";
import { getHookExecutionLogs } from "../lib/tauri-api";
import type { HookExecutionLog } from "../lib/types";
const CLI_COLORS: Record<string, string> = CLI_HEX_COLORS;

// run-with-log.sh wrapper utilities
// Usage: run-with-log.sh <hook_id> <command...>
// hook_id is a label (first arg), everything else is the real command.
const LOG_WRAPPER = "~/.kn/hooks/run-with-log.sh";
function isLogWrapped(cmd: string): boolean { return cmd.includes("run-with-log.sh"); }
function unwrapLog(cmd: string): string {
  // Strip "run-with-log.sh <hook_id> " prefix, keep everything after as the original command.
  // Handle optional -- separator that might have been added by older versions.
  const rest = cmd.replace(/^\S*run-with-log\.sh\s+\S+\s+(--\s+)?/, "");
  return rest.trim() || cmd;
}
function wrapLog(cmd: string, hookId: string): string {
  const label = hookId.replace(/[/:]/g, "__");
  return `${LOG_WRAPPER} ${label} ${cmd}`;
}

/* ──────────────────── Types ──────────────────── */

export interface HookEntry {
  id: string;
  cli: string;
  eventType: string;
  matcher?: string;
  command: string;
  hookType: string;
  enabled: boolean;
  source: string;
  path: string;
  groupIdx: number;
  hookIdx: number;
  timeout?: number;
  statusMessage?: string;
  name?: string;
  description?: string;
  projectName?: string;
  inherited?: boolean;
}

/* ──────────────────── Props ──────────────────── */

interface HookDetailProps {
  hook: HookEntry | null;
  onRefresh?: () => void;
  /** Context where the component is rendered — tailors the empty-state text. */
  scope?: "user" | "project";
}


const EVENT_LABELS: Record<string, string> = {
  UserPromptSubmit: "用户提交提示词",
  PreToolUse: "工具调用前",
  PermissionRequest: "权限请求",
  PostToolUse: "工具调用后",
  PostToolUseFailure: "工具调用失败",
  PostToolBatch: "批量工具完成",
  Stop: "会话回合结束",
  StopFailure: "回合异常结束",
  Notification: "系统通知",
  SessionStart: "会话开始",
  SessionEnd: "会话结束",
  PreCompact: "上下文压缩前",
  PostCompact: "上下文压缩后",
  SubagentStart: "子 Agent 启动",
  SubagentStop: "子 Agent 结束",
};

function MetaRow({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex items-baseline gap-2 py-0.5">
      <span className="text-2xs text-[var(--app-text-muted)] font-mono uppercase tracking-[0.1em] w-16 shrink-0">
        {label}
      </span>
      <span className="text-xs text-[var(--app-text-dim)] font-mono">{children}</span>
    </div>
  );
}

function SectionHeader({ icon, label }: { icon: React.ReactNode; label: string }) {
  return (
    <div className="flex items-center gap-2 mb-3 pt-1">
      {icon}
      <span className="text-2xs text-[var(--app-text-muted)] font-mono uppercase tracking-[0.2em]">
        {label}
      </span>
      <div className="flex-1 border-b border-[var(--app-border-light)]" />
    </div>
  );
}

/* ─────────────────── Empty State ──────────────────── */

function EmptyState({ scope }: { scope?: "user" | "project" }) {
  const isUser = scope === "user";
  const isProject = scope === "project";
  const scopeLabel = isUser ? "用户级" : isProject ? "项目级" : "";
  const scopeNote = isUser
    ? "管理当前用户下所有 CLI 工具的 Hook，存储在用户主目录。"
    : isProject
    ? "管理当前项目下的 Hook，存储在项目目录中，可随 Git 提交与团队共享。"
    : "";

  return (
    <div className="flex flex-col items-center justify-center h-full gap-5 text-center px-8">
      <div
        className="w-16 h-16 flex items-center justify-center border border-dashed"
        style={{ borderColor: "var(--app-border)", background: "var(--app-bg)" }}
      >
        <Zap size={28} className="text-[var(--app-text-muted)] opacity-25" />
      </div>

      <div>
        <h3 className="text-sm font-mono font-semibold text-[var(--app-text)] mb-1">
          {scopeLabel ? `${scopeLabel} Hooks` : "Hooks"}
        </h3>
        <p className="text-xs text-[var(--app-text-muted)] leading-relaxed max-w-sm">
          {scopeNote || "从左侧列表选择一个 Hook 查看详情，或点击 + 新建自定义 Hook。"}
        </p>
      </div>

      {/* What are hooks */}
      <div className="text-xs text-[var(--app-text-muted)] font-mono text-left space-y-1.5
        bg-[var(--app-cmd-bg)] border border-[var(--app-border)] p-3 w-full max-w-sm">
        <div className="text-[var(--app-text)] font-semibold">什么是 Hook？</div>
        <div className="leading-relaxed">
          Hook 是在 AI CLI 工具<b className="text-[var(--app-text)]">关键事件发生时自动执行</b>的脚本。
          比如在提交提示词前注入上下文、工具调用后记录日志、会话结束时发送通知等。
        </div>
        <div className="leading-relaxed mt-1">
          支持 <b className="text-[var(--app-text)]">Claude Code</b>、<b className="text-[var(--app-text)]">Codex</b>、<b className="text-[var(--app-text)]">Qoder</b> 三款工具。
        </div>
      </div>

      {/* Common events */}
      <div className="text-xs text-[var(--app-text-muted)] font-mono text-left
        bg-[var(--app-cmd-bg)] border border-[var(--app-border)] p-3 w-full max-w-sm">
        <div className="text-[var(--app-text)] font-semibold mb-1.5">常用触发事件</div>
        <div className="space-y-1 leading-relaxed">
          {[
            ["UserPromptSubmit", "用户提交提示词时"],
            ["PreToolUse", "工具调用前"],
            ["PostToolUse", "工具调用后"],
            ["Stop", "会话回合结束时"],
          ].map(([event, desc]) => (
            <div key={event} className="flex items-center gap-2">
              <ArrowRight size={10} className="text-[var(--app-accent)] opacity-50 shrink-0" />
              <code className="text-2xs px-1.5 py-px bg-[var(--app-input)] border border-[var(--app-border)] text-[var(--app-text-dim)] shrink-0">
                {event}
              </code>
              <span className="text-[var(--app-text-muted)]">{desc}</span>
            </div>
          ))}
        </div>
      </div>

      {/* Tip */}
      <p className="text-2xs text-[var(--app-text-muted)] max-w-xs">
        {isProject
          ? "项目级 Hook 存储在项目目录的 CLI 配置文件中，可通过 Git 与团队共享。"
          : "创建 Hook 时选择 CLI 工具、事件类型和脚本命令，保存后自动写入对应工具的配置文件。"}
      </p>
    </div>
  );
}

/** Format ISO 8601 timestamp to short display form: "MM-DD HH:mm" */
function formatLogTime(iso: string): string {
  try {
    const d = new Date(iso);
    const mm = String(d.getMonth() + 1).padStart(2, "0");
    const dd = String(d.getDate()).padStart(2, "0");
    const hh = String(d.getHours()).padStart(2, "0");
    const min = String(d.getMinutes()).padStart(2, "0");
    return `${mm}-${dd} ${hh}:${min}`;
  } catch {
    return iso.slice(0, 16);
  }
}

/* ──────────────────── Main ──────────────────── */

export function HookDetail({ hook, onRefresh, scope }: HookDetailProps) {
  if (!hook) return <EmptyState scope={scope} />;

  const cliColor = CLI_COLORS[hook.cli] || "#6B7280";
  const eventLabel = EVENT_LABELS[hook.eventType] || hook.eventType;
  // Profile-managed hooks (token tracking) have source="system" set by the Rust backend.
  const isReadonly = hook.source === "system";

  // Capture after null guard — TS doesn't narrow inside async closures
  const h = hook;

  const [showDeleteConfirm, setShowDeleteConfirm] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [editingMeta, setEditingMeta] = useState(false);
  const [editName, setEditName] = useState(hook.name || "");
  const [editDesc, setEditDesc] = useState(hook.description || "");
  const [savingMeta, setSavingMeta] = useState(false);

  // Script content — load if command is an absolute file path
  const isFilePath = hook.command.startsWith("/") || /^[A-Za-z]:\\/.test(hook.command);
  const [scriptContent, setScriptContent] = useState<string | null>(null);
  const [scriptLoading, setScriptLoading] = useState(false);
  const [scriptError, setScriptError] = useState("");

  const [wrapLoading, setWrapLoading] = useState(false);
  const wrapped = isLogWrapped(hook.command);

  // Execution logs
  const [execLogs, setExecLogs] = useState<HookExecutionLog[]>([]);
  const [logsLoading, setLogsLoading] = useState(false);
  const [expandedLogIdx, setExpandedLogIdx] = useState<number | null>(null);

  const handleToggleWrap = useCallback(async () => {
    setWrapLoading(true);
    try {
      const newCmd = wrapped ? unwrapLog(h.command) : wrapLog(h.command, h.id);
      // Use set_hook_command for in-place editing (no delete + recreate, preserves everything)
      await invoke("set_hook_command", {
        cli: h.cli,
        eventType: h.eventType,
        groupIdx: h.groupIdx,
        hookIdx: h.hookIdx,
        path: h.path,
        command: newCmd,
      });
      onRefresh?.();
    } catch (e) {
      console.error("Toggle wrap failed:", e);
      setActionError(String(e).slice(0, 200));
    } finally {
      setWrapLoading(false);
    }
  }, [h, wrapped, onRefresh]);

  const loadLogs = useCallback(() => {
    setLogsLoading(true);
    // hook.id contains ":" (e.g. "claude:hook:user:Stop:0:2"), but wrapLog
    // sanitizes them to "__" before passing to run-with-log.sh, so the log
    // files store the sanitized id. Use the same sanitization for filtering.
    const sanitizedId = hook.id.replace(/[/:]/g, "__");
    getHookExecutionLogs(sanitizedId, 50)
      .then((logs) => { setExecLogs(logs); })
      .catch((e) => { console.error("Failed to load hook logs:", e); })
      .finally(() => { setLogsLoading(false); });
  }, [hook.id]);

  useEffect(() => { loadLogs(); }, [loadLogs]);

  useEffect(() => {
    if (!isFilePath) {
      setScriptContent(null);
      setScriptError("");
      return;
    }
    let cancelled = false;
    setScriptLoading(true);
    setScriptError("");
    invoke<string>("read_file", { path: hook.command })
      .then((c) => { if (!cancelled) setScriptContent(c); })
      .catch((e) => { if (!cancelled) setScriptError(String(e).slice(0, 120)); })
      .finally(() => { if (!cancelled) setScriptLoading(false); });
    return () => { cancelled = true; };
  }, [hook.command, isFilePath]);

  async function handleSaveMeta() {
    if (!editName.trim()) return;
    setSavingMeta(true);
    try {
      await invoke("set_hook_meta", {
        cli: h.cli,
        eventType: h.eventType,
        groupIdx: h.groupIdx,
        hookIdx: h.hookIdx,
        name: editName.trim(),
        description: editDesc.trim() || null,
      });
      setEditingMeta(false);
      onRefresh?.();
    } catch (e) {
      console.error("Save hook meta failed:", e);
    } finally {
      setSavingMeta(false);
    }
  }

  async function handleToggle() {
    try {
      await invoke("toggle_hook", {
        cli: h.cli,
        eventType: h.eventType,
        groupIdx: h.groupIdx,
        hookIdx: h.hookIdx,
        enabled: !h.enabled,
        path: h.path,
      });
      onRefresh?.();
    } catch (e) {
      console.error("Toggle hook failed:", e);
    }
  }

  async function handleDelete() {
    try {
      await invoke("delete_hook", {
        cli: h.cli,
        eventType: h.eventType,
        groupIdx: h.groupIdx,
        hookIdx: h.hookIdx,
        path: h.path,
      });
      setShowDeleteConfirm(false);
      onRefresh?.();
    } catch (e) {
      setShowDeleteConfirm(false);
      setActionError(String(e).slice(0, 200));
      console.error("Delete hook failed:", e);
    }
  }

  return (
    <div className="flex flex-col h-full animate-[fadeIn_150ms_ease-out]">
      {/* Hero */}
      <div className="px-6 pt-8 pb-5 border-b border-[var(--app-border-light)]">
        <div className="flex items-start gap-3">
          <div
            className="w-10 h-10 flex items-center justify-center shrink-0 border"
            style={{ background: "var(--app-input)", borderColor: "var(--app-border)" }}
          >
            <Terminal size={18} style={{ color: hook.enabled ? cliColor : "var(--app-text-muted)" }} />
          </div>
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-2">
              <h2 className="text-base font-mono text-[var(--app-text)] truncate">
                {hook.name || eventLabel}
              </h2>
              <span
                className="text-2xs font-mono px-1.5 py-px border shrink-0"
                style={{ color: cliColor, borderColor: cliColor, opacity: 0.75 }}
              >
                {CLI_LABELS[hook.cli as CliKind] || hook.cli}
              </span>
              {/* Edit name/description button */}
              {!isReadonly && (
                <button
                  onClick={() => setEditingMeta(true)}
                  className="p-0.5 text-[var(--app-text-muted)] hover:text-[var(--app-accent)] transition-colors shrink-0"
                  title="编辑名称和描述"
                >
                  <Pencil size={12} />
                </button>
              )}
            </div>
            <div className="flex items-center gap-2 mt-1">
              <p className="text-xs text-[var(--app-text-muted)] font-mono">
                {hook.name ? eventLabel : hook.eventType}
              </p>
              <span className="flex items-center gap-1 text-2xs font-mono">
                {hook.enabled ? (
                  <>
                    <span className="w-1.5 h-1.5 rounded-full bg-[var(--app-accent)]" style={{ boxShadow: "0 0 4px var(--app-glow)" }} />
                    <span className="text-[var(--app-green)]">已启用</span>
                  </>
                ) : (
                  <>
                    <Lock size={9} className="text-[var(--app-text-muted)]" />
                    <span className="text-[var(--app-text-muted)]">已禁用</span>
                  </>
                )}
              </span>
            </div>
          </div>
        </div>
      </div>

      {/* Metadata */}
      <div className="px-6 py-4 border-b border-[var(--app-border-light)]">
        <div className="space-y-1">
          <MetaRow label="CLI">
            <span style={{ color: cliColor }}>{CLI_LABELS[hook.cli as CliKind] || hook.cli}</span>
          </MetaRow>
          <MetaRow label="类型">
            <span className="text-[var(--app-accent)]">{hook.hookType}</span>
          </MetaRow>
          <MetaRow label="级别">
            <span className={hook.source === "user" ? "text-[var(--app-accent)]" : hook.source === "system" ? "text-[var(--app-amber)]" : "text-[var(--app-text-dim)]"}>
              {hook.source === "user" ? "用户级" : hook.source === "system" ? "系统级" : "项目级"}
            </span>
          </MetaRow>
          {hook.timeout !== undefined && hook.timeout > 0 && (
            <MetaRow label="超时">{hook.timeout}s</MetaRow>
          )}
        </div>

        {/* Description — display or edit */}
        {editingMeta ? (
          <div className="mt-3 p-3 border border-[var(--app-accent)]/30 bg-[var(--app-accent)]/5">
            <div className="flex items-center gap-2 mb-2">
              <span className="text-2xs text-[var(--app-accent)] font-mono uppercase tracking-[0.1em]">编辑元数据</span>
            </div>
            <label className="block text-2xs text-[var(--app-text-muted)] font-mono mb-1">名称 *</label>
            <input
              type="text"
              value={editName}
              onChange={(e) => setEditName(e.target.value)}
              placeholder="给这个 Hook 起个名字…"
              className="w-full h-[28px] text-xs font-mono px-2 mb-2 bg-[var(--app-input)] border border-[var(--app-border)] focus:border-[var(--app-accent)] focus:outline-none"
              autoFocus
            />
            <label className="block text-2xs text-[var(--app-text-muted)] font-mono mb-1">描述</label>
            <textarea
              value={editDesc}
              onChange={(e) => setEditDesc(e.target.value)}
              placeholder="这个 Hook 是做什么的？"
              rows={2}
              className="w-full text-xs font-mono px-2 py-1 bg-[var(--app-input)] border border-[var(--app-border)] focus:border-[var(--app-accent)] focus:outline-none resize-none"
            />
            <div className="flex items-center gap-1 mt-2">
              <button
                onClick={handleSaveMeta}
                disabled={!editName.trim() || savingMeta}
                className="flex items-center gap-1 px-2 py-1 text-2xs font-mono bg-[var(--app-accent)] text-[var(--app-bg)] hover:opacity-90 disabled:opacity-50 transition-opacity"
              >
                <Check size={10} />
                {savingMeta ? "保存中…" : "保存"}
              </button>
              <button
                onClick={() => {
                  setEditingMeta(false);
                  setEditName(hook.name || "");
                  setEditDesc(hook.description || "");
                }}
                className="flex items-center gap-1 px-2 py-1 text-2xs font-mono border border-[var(--app-border)] text-[var(--app-text-muted)] hover:text-[var(--app-text)] transition-colors"
              >
                <X size={10} />
                取消
              </button>
            </div>
          </div>
        ) : hook.description ? (
          <div className="mt-3 p-2.5 border border-[var(--app-border-light)] bg-[var(--app-bg)]">
            <div className="flex items-center gap-2 mb-1">
              <span className="text-2xs text-[var(--app-text-muted)] font-mono uppercase tracking-[0.1em]">描述</span>
            </div>
            <p className="text-xs text-[var(--app-text-dim)] font-mono leading-relaxed">
              {hook.description}
            </p>
          </div>
        ) : !isReadonly ? (
          <div className="mt-3">
            <button
              onClick={() => {
                setEditName(hook.name || "");
                setEditDesc("");
                setEditingMeta(true);
              }}
              className="text-2xs text-[var(--app-text-muted)] font-mono hover:text-[var(--app-accent)] transition-colors"
            >
              + 添加描述
            </button>
          </div>
        ) : null}

        {hook.matcher && (
          <div className="mt-3">
            <div className="flex items-center gap-2 mb-1.5">
              <span className="text-2xs text-[var(--app-text-muted)] font-mono uppercase tracking-[0.1em]">Matcher</span>
            </div>
            <div className="p-2.5 border border-[var(--app-border-light)] bg-[var(--app-bg)]">
              <span className="text-xs text-[var(--app-text)] font-mono">{hook.matcher}</span>
            </div>
          </div>
        )}

        <div className="mt-3 flex items-center gap-1 text-2xs text-[var(--app-text-muted)] font-mono">
          <FolderOpen size={10} className="shrink-0" />
          <span className="truncate">{hook.path}</span>
        </div>

        {/* Action buttons or readonly hint */}
        {isReadonly ? (
          <div className="mt-3 flex items-start gap-2 p-2.5 border border-[var(--app-amber)]/30 bg-[var(--app-amber-bg)]/50">
            <Info size={12} className="text-[var(--app-text-muted)] shrink-0 mt-0.5" />
            <div>
              <p className="text-2xs text-[var(--app-text-dim)] font-mono leading-relaxed">
                Profile Hook，由 profile 系统管理
              </p>
              <p className="text-2xs text-[var(--app-text-muted)] font-mono mt-0.5">
                不可在此处启用 / 禁用或删除
              </p>
            </div>
          </div>
        ) : (
          <>
            {actionError && (
              <div className="mt-3 flex items-start gap-2 p-2.5 border border-[var(--app-red)]/30 bg-[var(--app-red-bg)]/30">
                <X size={12} className="text-[var(--app-red)] shrink-0 mt-0.5" />
                <div>
                  <p className="text-2xs text-[var(--app-red)] font-mono leading-relaxed">{actionError}</p>
                  <button
                    onClick={() => setActionError(null)}
                    className="text-2xs text-[var(--app-text-muted)] hover:text-[var(--app-text)] mt-1"
                  >
                    关闭
                  </button>
                </div>
              </div>
            )}
          <div className="mt-3 flex items-center gap-1">
            <Button
              variant="icon"
              size="sm"
              onClick={handleToggle}
              className={`p-1.5 border-[var(--app-border)] ${
                hook.enabled
                  ? "hover:text-[var(--app-red)] hover:border-[var(--app-red)] hover:bg-[var(--app-red-bg)]"
                  : "hover:text-[var(--app-accent)] hover:border-[var(--app-accent)] hover:bg-[var(--app-green-bg)]"
              }`}
              title={hook.enabled ? "禁用" : "启用"}
            >
              {hook.enabled ? <Ban size={14} /> : <Play size={14} />}
            </Button>
            <Button
              variant="icon"
              size="sm"
              onClick={() => setShowDeleteConfirm(true)}
              className="p-1.5 border-[var(--app-border)] hover:text-[var(--app-red)] hover:border-[var(--app-red)] hover:bg-[var(--app-red-bg)]"
              title="删除"
            >
              <Trash2 size={14} />
            </Button>
          </div>
          </>
        )}
      </div>

      {/* Command */}
      <div className="px-6 py-4 border-b border-[var(--app-border-light)]">
        <SectionHeader
          icon={<Terminal size={12} className="text-[var(--app-text-muted)]" />}
          label="Command"
        />
        <pre className={`p-3 text-2xs font-mono leading-relaxed whitespace-pre-wrap bg-[var(--app-bg)] border border-[var(--app-border-light)] ${wrapped ? "text-[var(--app-accent)]" : "text-[var(--app-text-dim)]"}`}>
          {hook.command}
        </pre>

        {/* Wrap/unwrap toggle + log indicator */}
        {!isReadonly && (
          <div className="mt-2 flex items-center gap-2">
            <button
              onClick={handleToggleWrap}
              disabled={wrapLoading}
              className={`flex items-center gap-1.5 px-2 py-1 text-2xs font-mono border rounded transition-colors
                ${wrapped
                  ? "border-[var(--app-accent)]/30 text-[var(--app-accent)] hover:bg-[var(--app-accent)]/10"
                  : "border-[var(--app-border)] text-[var(--app-text-muted)] hover:text-[var(--app-accent)] hover:border-[var(--app-accent)]/50"
                } disabled:opacity-50`}
              title={wrapped ? "移除日志包装" : "用 run-with-log.sh 包装（用于测试和查看执行日志）"}
            >
              {wrapLoading ? (
                <Loader size={10} className="animate-spin" />
              ) : (
                <RotateCw size={10} className={wrapped ? "text-[var(--app-accent)]" : ""} />
              )}
              {wrapped ? "已启用日志记录" : "启用日志记录（用于测试）"}
            </button>
            {wrapped && (
              <span className="text-2xs text-[var(--app-text-muted)] font-mono">
                每次执行都会记录到执行历史中
              </span>
            )}
          </div>
        )}

        {/* Script content — shown when command is a file path */}
        {isFilePath && (
          <div className="mt-3">
            <SectionHeader
              icon={<FileText size={12} className="text-[var(--app-text-muted)]" />}
              label="脚本内容"
            />
            {scriptLoading ? (
              <div className="flex items-center gap-2 p-3 text-xs text-[var(--app-text-muted)] font-mono">
                <Loader size={12} className="animate-spin" />
                加载中...
              </div>
            ) : scriptError ? (
              <div className="p-3 border border-[var(--app-amber)]/30 bg-[var(--app-amber-bg)]/50 text-xs text-[var(--app-text-dim)] font-mono">
                {scriptError}
              </div>
            ) : scriptContent !== null ? (
              <pre className="p-3 text-2xs text-[var(--app-text-dim)] font-mono leading-relaxed whitespace-pre-wrap bg-[var(--app-bg)] border border-[var(--app-border-light)] max-h-80 overflow-y-auto">
                {scriptContent}
              </pre>
            ) : null}
          </div>
        )}
      </div>

      {/* Execution History */}
      <div className="flex-1 min-h-0 flex flex-col">
        <div className="px-6 py-4 flex-1 min-h-0 flex flex-col">
          <div className="flex items-center justify-between mb-3 pt-1">
            <div className="flex items-center gap-2">
              <Clock size={12} className="text-[var(--app-text-muted)]" />
              <span className="text-2xs text-[var(--app-text-muted)] font-mono uppercase tracking-[0.2em]">执行历史</span>
            </div>
            <button
              onClick={() => loadLogs()}
              disabled={logsLoading}
              className="p-1 text-[var(--app-text-muted)] hover:text-[var(--app-accent)] transition-colors disabled:opacity-30"
              title="刷新执行历史"
            >
              <RefreshCw size={11} className={logsLoading ? "animate-spin" : ""} />
            </button>
          </div>

          {logsLoading ? (
            <div className="flex items-center gap-2 py-6 justify-center text-xs text-[var(--app-text-muted)] font-mono">
              <Loader size={12} className="animate-spin" />
              加载中...
            </div>
          ) : execLogs.length === 0 ? (
            <div className="flex flex-col items-center gap-2 py-6 text-center">
              <Clock size={20} className="text-[var(--app-text-muted)] opacity-30" />
              <p className="text-xs text-[var(--app-text-muted)] font-mono">
                暂无执行记录
              </p>
              <p className="text-2xs text-[var(--app-text-muted)] font-mono max-w-xs leading-relaxed">
                将 Hook 命令配置为通过{" "}
                <code className="px-1 py-px bg-[var(--app-input)] border border-[var(--app-border)] text-[var(--app-text-dim)]">
                  run-with-log.sh
                </code>{" "}
                包装执行后，每次运行都会在此记录日志。
              </p>
            </div>
          ) : (
            <div className="border border-[var(--app-border-light)] flex-1 min-h-0 flex flex-col overflow-hidden">
              {/* Table header */}
              <div
                className="grid text-2xs text-[var(--app-text-muted)] font-mono uppercase tracking-[0.08em] px-2 py-1.5 border-b border-[var(--app-border-light)] shrink-0"
                style={{ background: "var(--app-input)", gridTemplateColumns: "1fr 36px 40px 54px 1fr" }}
              >
                <span className="truncate">时间</span>
                <span className="text-center">状态</span>
                <span className="text-center">代码</span>
                <span className="text-center">耗时</span>
                <span className="truncate">输出预览</span>
              </div>

              {/* Log rows — flex-1 fills remaining height */}
              <div className="flex-1 overflow-y-auto">
                {execLogs.map((log, idx) => {
                  const isExpanded = expandedLogIdx === idx;
                  return (
                    <div key={`${log.timestamp}-${idx}`}>
                      <button
                        onClick={() => setExpandedLogIdx(isExpanded ? null : idx)}
                        className="grid w-full text-left text-2xs font-mono px-2 py-1.5 border-b border-[var(--app-border-light)]/50 hover:bg-[var(--app-hover)] transition-colors items-center"
                        style={{ gridTemplateColumns: "1fr 36px 40px 54px 1fr" }}
                      >
                        <span className="text-[var(--app-text-dim)] truncate">
                          {formatLogTime(log.timestamp)}
                        </span>
                        <span className="text-center">
                          {log.exitCode === 0 ? (
                            <span className="text-[var(--app-green)]" title="成功">OK</span>
                          ) : log.exitCode !== undefined && log.exitCode !== null ? (
                            <span className="text-[var(--app-red)]" title={`失败 (${log.exitCode})`}>ERR</span>
                          ) : (
                            <span className="text-[var(--app-text-muted)]">--</span>
                          )}
                        </span>
                        <span className={`text-center ${log.exitCode === 0 ? "text-[var(--app-text-muted)]" : "text-[var(--app-red)]"}`}>
                          {log.exitCode !== undefined && log.exitCode !== null ? log.exitCode : "--"}
                        </span>
                        <span className="text-center text-[var(--app-text-muted)]">
                          {log.durationMs !== undefined && log.durationMs !== null
                            ? log.durationMs < 1000
                              ? `${log.durationMs}ms`
                              : `${(log.durationMs / 1000).toFixed(1)}s`
                            : "--"}
                        </span>
                        <span className="text-[var(--app-text-dim)] truncate">
                          {(log.outputPreview || log.errorPreview || "").slice(0, 60) || "--"}
                        </span>
                      </button>

                      {/* Expanded detail panel */}
                      {isExpanded && (
                        <div className="px-3 py-2 border-b border-[var(--app-border-light)]/50 bg-[var(--app-bg)]">
                          <div className="flex items-center gap-1 mb-1.5">
                            {isExpanded ? (
                              <ChevronDown size={10} className="text-[var(--app-text-muted)]" />
                            ) : (
                              <ChevronRight size={10} className="text-[var(--app-text-muted)]" />
                            )}
                            <span className="text-2xs text-[var(--app-text-muted)] font-mono uppercase tracking-[0.08em]">
                              详细信息
                            </span>
                          </div>
                          <div className="space-y-1.5">
                            <div>
                              <span className="text-2xs text-[var(--app-text-muted)] font-mono">时间: </span>
                              <span className="text-2xs text-[var(--app-text-dim)] font-mono">{log.timestamp}</span>
                            </div>
                            <div>
                              <span className="text-2xs text-[var(--app-text-muted)] font-mono">状态: </span>
                              <span className={`text-2xs font-mono ${log.exitCode === 0 ? "text-[var(--app-green)]" : "text-[var(--app-red)]"}`}>
                                {log.exitCode === 0 ? "成功" : `失败 (exit ${log.exitCode})`}
                              </span>
                            </div>
                            {log.durationMs !== undefined && log.durationMs !== null && (
                              <div>
                                <span className="text-2xs text-[var(--app-text-muted)] font-mono">耗时: </span>
                                <span className="text-2xs text-[var(--app-text-dim)] font-mono">
                                  {log.durationMs < 1000 ? `${log.durationMs}ms` : `${(log.durationMs / 1000).toFixed(2)}s`}
                                </span>
                              </div>
                            )}
                            {log.outputPreview && (
                              <div>
                                <span className="text-2xs text-[var(--app-text-muted)] font-mono">标准输出: </span>
                                <pre className="mt-0.5 p-2 text-2xs text-[var(--app-text-dim)] font-mono leading-relaxed whitespace-pre-wrap bg-[var(--app-input)] border border-[var(--app-border-light)] max-h-32 overflow-y-auto">
                                  {log.outputPreview}
                                </pre>
                              </div>
                            )}
                            {log.errorPreview && (
                              <div>
                                <span className="text-2xs text-[var(--app-text-muted)] font-mono">标准错误: </span>
                                <pre className="mt-0.5 p-2 text-2xs text-[var(--app-red)] font-mono leading-relaxed whitespace-pre-wrap bg-[var(--app-red-bg)]/20 border border-[var(--app-red)]/20 max-h-32 overflow-y-auto">
                                  {log.errorPreview}
                                </pre>
                              </div>
                            )}
                          </div>
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Delete confirmation dialog */}
      <ConfirmDialog
        open={showDeleteConfirm}
        title="删除 Hook"
        message={`确定要永久删除这个 Hook 吗？\n\n事件: ${h.eventType}\n命令: ${h.command}`}
        confirmLabel="删除"
        onConfirm={handleDelete}
        onCancel={() => setShowDeleteConfirm(false)}
      />
    </div>
  );
}
