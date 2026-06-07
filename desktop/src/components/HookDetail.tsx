import { useState, useEffect } from "react";
import { Terminal, Lock, FolderOpen, Play, Ban, Trash2, Info, Pencil, Check, X, FileText, Loader } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { ConfirmDialog } from "./ConfirmDialog";
import { CLI_HEX_COLORS } from "../lib/cli-constants";
const CLI_COLORS: Record<string, string> = CLI_HEX_COLORS;

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
}

/* ──────────────────── Props ──────────────────── */

interface HookDetailProps {
  hook: HookEntry | null;
  onRefresh?: () => void;
}


const CLI_LABELS: Record<string, string> = {
  claude: "Claude",
  codex: "Codex",
  qoder: "Qoder",
};

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

function EmptyState() {
  return (
    <div className="flex flex-col items-center justify-center h-full gap-4 text-center px-8">
      <div
        className="w-16 h-16 flex items-center justify-center border border-dashed"
        style={{ borderColor: "var(--app-border)", background: "var(--app-bg)" }}
      >
        <Terminal size={24} className="text-[var(--app-text-muted)] opacity-25" />
      </div>
      <div>
        <h3 className="text-sm font-mono text-[var(--app-text-dim)] mb-1">Hook Manager</h3>
        <p className="text-xs text-[var(--app-text-muted)] leading-relaxed max-w-xs">
          从左侧列表选择一个 Hook 查看详情。
        </p>
      </div>
    </div>
  );
}

/* ──────────────────── Main ──────────────────── */

export function HookDetail({ hook, onRefresh }: HookDetailProps) {
  if (!hook) return <EmptyState />;

  const cliColor = CLI_COLORS[hook.cli] || "#6B7280";
  const eventLabel = EVENT_LABELS[hook.eventType] || hook.eventType;
  // Profile-managed hooks (token tracking) have source="system" set by the Rust backend.
  const isReadonly = hook.source === "system";

  // Capture after null guard — TS doesn't narrow inside async closures
  const h = hook;

  const [showDeleteConfirm, setShowDeleteConfirm] = useState(false);
  const [editingMeta, setEditingMeta] = useState(false);
  const [editName, setEditName] = useState(hook.name || "");
  const [editDesc, setEditDesc] = useState(hook.description || "");
  const [savingMeta, setSavingMeta] = useState(false);

  // Script content — load if command is an absolute file path
  const isFilePath = hook.command.startsWith("/");
  const [scriptContent, setScriptContent] = useState<string | null>(null);
  const [scriptLoading, setScriptLoading] = useState(false);
  const [scriptError, setScriptError] = useState("");

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
      });
      setShowDeleteConfirm(false);
      onRefresh?.();
    } catch (e) {
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
                {CLI_LABELS[hook.cli] || hook.cli}
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
            <span style={{ color: cliColor }}>{CLI_LABELS[hook.cli] || hook.cli}</span>
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
          <div className="mt-3 flex items-center gap-1">
            <button
              onClick={handleToggle}
              className={`p-1.5 border transition-all duration-fast
                ${hook.enabled
                  ? "text-[var(--app-text-muted)] border-[var(--app-border)] hover:text-[var(--app-red)] hover:border-[var(--app-red)] hover:bg-[var(--app-red-bg)]"
                  : "text-[var(--app-text-muted)] border-[var(--app-border)] hover:text-[var(--app-accent)] hover:border-[var(--app-accent)] hover:bg-[var(--app-green-bg)]"
                }`}
              title={hook.enabled ? "禁用" : "启用"}
            >
              {hook.enabled ? <Ban size={14} /> : <Play size={14} />}
            </button>
            <button
              onClick={() => setShowDeleteConfirm(true)}
              className="p-1.5 border border-[var(--app-border)] text-[var(--app-text-muted)]
                hover:text-[var(--app-red)] hover:border-[var(--app-red)] hover:bg-[var(--app-red-bg)]
                transition-all duration-fast"
              title="删除"
            >
              <Trash2 size={14} />
            </button>
          </div>
        )}
      </div>

      {/* Command */}
      <div className="px-6 py-4 border-b border-[var(--app-border-light)]">
        <SectionHeader
          icon={<Terminal size={12} className="text-[var(--app-text-muted)]" />}
          label="Command"
        />
        <pre className="p-3 text-2xs text-[var(--app-text-dim)] font-mono leading-relaxed whitespace-pre-wrap bg-[var(--app-bg)] border border-[var(--app-border-light)]">
          {hook.command}
        </pre>

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

      {/* Empty space */}
      <div className="flex-1" />

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
