import { useState, useMemo } from "react";
import { X, ChevronRight, Terminal, Check, AlertCircle, FolderOpen, Copy } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";

/* ─────────────────── Types ─────────────────── */

interface HookWizardProps {
  open: boolean;
  onClose: () => void;
  onCreated: (name: string, description: string) => void;
}

/* ──────────────────── Constants ──────────────────── */

const CLI_TOOLS = [
  { id: "claude", name: "Claude", desc: "Claude Code" },
  { id: "qoder", name: "Qoder", desc: "Qoder CLI" },
  { id: "codex", name: "Codex", desc: "OpenAI Codex" },
] as const;

type CliId = (typeof CLI_TOOLS)[number]["id"];

interface HookEventDef {
  id: string;
  label: string;
  desc: string;
  blockable: boolean;
}

/**
 * Per-CLI hook event definitions.
 *
 * Each CLI has its own set of supported events with correct blockable flags.
 * When adding a new CLI or event, only modify this map — the wizard will
 * automatically show the right events for the selected CLI.
 */
const CLI_EVENTS: Record<CliId, HookEventDef[]> = {
  claude: [
    { id: "UserPromptSubmit",  label: "用户提交提示词",   desc: "用户每次发送消息时触发。可拦截（exit 2）阻止本次请求。常用于注入上下文、屏蔽敏感词、打日志。stdin 含 session_id / prompt / cwd。", blockable: true },
    { id: "PreToolUse",        label: "工具调用前",       desc: "工具执行前触发。可拦截（exit 2 阻止工具执行）。matcher 匹配工具名（Bash / Write / Edit 等）。常用于安全审查、参数校验、拦截 rm -rf 等危险命令。stdin 含 tool_name / tool_input。", blockable: true },
    { id: "PermissionRequest", label: "权限请求",         desc: "工具需要用户确认权限时触发。可拦截或自动批准。matcher 匹配工具名。常用于自动批准安全操作、拒绝高风险请求、实现自定义权限策略。", blockable: true },
    { id: "PostToolUse",       label: "工具调用后",       desc: "工具执行成功后触发（exit 2 无效，仅观察）。matcher 匹配工具名。常用于自动格式化代码、运行 lint、记录日志。stdin 含 tool_name / tool_input / tool_response。", blockable: false },
    { id: "PostToolUseFailure",label: "工具调用失败",      desc: "工具执行失败后触发（仅观察）。matcher 匹配工具名。常用于错误通知、自动重试、上报异常。stdin 含 tool_name / tool_input / error。", blockable: false },
    { id: "PostToolBatch",     label: "批量工具完成后",    desc: "并行工具批次全部完成时触发（仅观察）。所有同批次 PostToolUse 之后才执行一次。用于批量汇总、整体质量检查。", blockable: false },
    { id: "Stop",              label: "会话回合结束",      desc: "Agent 完成一轮应答时触发。可拦截（exit 2 让 Agent 继续执行，不结束回合）。用于检查任务完成度、自动追加遗漏步骤。stdin 含 stop_reason / transcript。", blockable: true },
    { id: "StopFailure",       label: "回合异常结束",      desc: "API 限流、鉴权失败等异常导致回合结束时触发（仅观察）。用于异常告警、自动切换 API key。", blockable: false },
    { id: "Notification",      label: "系统通知",         desc: "Agent 发出系统通知时触发（仅观察）。用于自定义通知转发（Slack / 钉钉 / 邮件）。", blockable: false },
    { id: "SessionStart",      label: "会话开始",         desc: "会话启动或恢复时触发（仅观察）。matcher 匹配启动来源：startup（新会话）/ resume（恢复）/ clear（清空）/ compact（压缩后）。用于初始化环境、加载配置。", blockable: false },
    { id: "SessionEnd",        label: "会话结束",         desc: "会话终止、进程退出前触发（仅观察）。用于持久化统计数据、清理临时文件、上报用量。", blockable: false },
    { id: "PreCompact",        label: "上下文压缩前",      desc: "会话历史被压缩前触发（仅观察）。matcher 匹配触发方式：manual（手动）/ auto（自动）。用于标记重要对话段落、保存关键上下文。", blockable: false },
    { id: "PostCompact",       label: "上下文压缩后",      desc: "上下文压缩完成后触发（仅观察）。matcher 匹配触发方式。用于验证压缩质量、记录压缩前后 token 数变化。", blockable: false },
    { id: "SubagentStart",     label: "子 Agent 启动",    desc: "子 Agent（如 code-review Agent）创建时触发（仅观察）。matcher 自由填写子 Agent 类型名。用于子任务环境初始化、分配资源。", blockable: false },
    { id: "SubagentStop",      label: "子 Agent 结束",    desc: "子 Agent 完成/销毁时触发。可拦截（exit 2）。matcher 自由填写。用于验证子任务输出、收集子 Agent 产物。", blockable: true },
  ],
  qoder: [
    { id: "UserPromptSubmit",  label: "用户提交提示词",   desc: "用户每次发送消息时触发。可拦截（exit 2）阻止本次请求。常用于注入上下文、屏蔽敏感词。stdin 含 session_id / prompt / cwd。", blockable: true },
    { id: "PreToolUse",        label: "工具调用前",       desc: "工具执行前触发。可拦截（exit 2 阻止工具执行）。matcher 匹配工具名（Bash / Write / Edit 等）。常用于安全审查、拦截危险命令。stdin 含 tool_name / tool_input。", blockable: true },
    { id: "PostToolUse",       label: "工具调用后",       desc: "工具执行成功后触发（仅观察）。matcher 匹配工具名。常用于自动格式化、运行 lint、记录日志。stdin 含 tool_name / tool_input / tool_response。", blockable: false },
    { id: "PostToolUseFailure",label: "工具调用失败",      desc: "工具执行失败后触发（仅观察）。matcher 匹配工具名。常用于错误通知、自动重试。stdin 含 tool_name / tool_input / error。", blockable: false },
    { id: "Stop",              label: "会话回合结束",      desc: "Agent 完成一轮应答时触发。可拦截（exit 2 让 Agent 继续执行）。用于检查任务完成度、自动追加遗漏步骤。", blockable: true },
    { id: "Notification",      label: "系统通知",         desc: "Agent 发出系统通知时触发（仅观察）。用于自定义通知转发。", blockable: false },
    { id: "SessionStart",      label: "会话开始",         desc: "会话启动/恢复/压缩后触发（仅观察）。matcher：startup（新会话）/ resume（恢复）/ compact（压缩后）。用于初始化环境、加载配置。", blockable: false },
    { id: "SubagentStop",      label: "子代理结束",       desc: "子代理准备结束时触发。可拦截（exit 2）。用于验证子任务输出、收集产物。", blockable: true },
    { id: "PreCompact",        label: "上下文压缩前",      desc: "会话历史被压缩前触发（仅观察）。用于标记重要对话、保存关键上下文。", blockable: false },
    { id: "SessionEnd",        label: "会话结束",         desc: "会话终止、进程退出前触发（仅观察）。用于持久化统计、清理临时文件。", blockable: false },
  ],
  codex: [
    { id: "SessionStart",      label: "会话开始",         desc: "会话初始化时触发（仅观察）。matcher：startup（新开）/ resume（恢复）/ clear（清空）/ compact（压缩后）。常用于环境初始化、加载会话笔记。", blockable: false },
    { id: "UserPromptSubmit",  label: "用户提交提示词",   desc: "用户提交消息后、模型处理前触发（仅观察，matcher 不生效）。用于记录对话、注入初始上下文。stdin 含 session_id / prompt。", blockable: false },
    { id: "PreToolUse",        label: "工具调用前",       desc: "工具执行前触发。可拦截（exit 2 阻止工具执行）。matcher 匹配工具名（Bash / apply_patch / MCP 工具等）。常用于安全审查、参数校验。stdin 含 tool_name / tool_input。", blockable: true },
    { id: "PermissionRequest", label: "权限请求",         desc: "工具需要用户审批时触发。可拦截或自动批准。matcher 匹配工具名。用于实现自定义审批策略。", blockable: true },
    { id: "PostToolUse",       label: "工具调用后",       desc: "工具完成后触发（仅观察）。matcher 匹配工具名。用于审查输出、自动格式化、记录日志。stdin 含 tool_name / tool_input / tool_response。", blockable: false },
    { id: "Stop",              label: "会话回合结束",      desc: "Agent 完成回合或被中断时触发（仅观察，matcher 不生效）。用于清理、通知、分析。支持 timeout 字段限制脚本执行时间。", blockable: false },
    { id: "PreCompact",        label: "上下文压缩前",      desc: "会话历史压缩前触发（仅观察）。matcher：manual（手动触发）/ auto（自动压缩）。用于保存关键上下文、标记重要段落。", blockable: false },
    { id: "PostCompact",       label: "上下文压缩后",      desc: "上下文压缩完成后触发（仅观察）。matcher：manual / auto。用于验证压缩质量、记录 token 变化。", blockable: false },
    { id: "SubagentStart",     label: "子 Agent 启动",    desc: "子 Agent（如 Reviewer）创建时触发（仅观察）。matcher 自由填写子 Agent 类型名。用于子任务环境准备。", blockable: false },
    { id: "SubagentStop",      label: "子 Agent 结束",    desc: "子 Agent 完成/销毁时触发（仅观察）。matcher 自由填写。用于收集子 Agent 产物、验收输出。", blockable: false },
  ],
};

/** Union of all event IDs across all CLIs — used for event labels and grouping. */
const ALL_EVENT_IDS = [
  "UserPromptSubmit", "PreToolUse", "PermissionRequest", "PostToolUse",
  "PostToolUseFailure", "PostToolBatch", "Stop", "StopFailure",
  "Notification", "SessionStart", "SessionEnd", "PreCompact", "PostCompact",
  "SubagentStart", "SubagentStop",
] as const;

/**
 * Matcher presets grouped by event category.
 *
 * Different events match on different things:
 * - Tool events → tool names (Bash, Write, Edit, …)
 * - SessionStart → start source (startup, resume, clear, compact)
 * - PreCompact / PostCompact → compaction trigger (manual, auto)
 * - SubagentStart / SubagentStop → free-text only (depends on subagent type)
 * - Other events → matcher is ignored, hide the section entirely
 */
const MATCHER_BY_CATEGORY: Record<string, { presets: string[]; hint: string }> = {
  tool: {
    presets: ["*", "Bash", "Write", "Edit", "Read", "Grep", "Glob"],
    hint: "留空 = 所有工具，多选以 | 分隔，支持正则",
  },
  session: {
    presets: ["*", "startup", "resume", "clear", "compact"],
    hint: "留空 = 所有来源，多选以 | 分隔",
  },
  compact: {
    presets: ["*", "manual", "auto"],
    hint: "留空 = 所有触发方式，多选以 | 分隔",
  },
  subagent: {
    presets: [],
    hint: "输入子 Agent 类型名，多选以 | 分隔",
  },
  none: {
    presets: [],
    hint: "",
  },
};

/** Events that use tool names as matcher. */
const TOOL_MATCHER_EVENTS = new Set([
  "PreToolUse", "PostToolUse", "PermissionRequest",
  "PostToolUseFailure", "PostToolBatch",
]);

/** Events that use session source as matcher. */
const SESSION_MATCHER_EVENTS = new Set(["SessionStart"]);

/** Events that use compact trigger as matcher. */
const COMPACT_MATCHER_EVENTS = new Set(["PreCompact", "PostCompact"]);

/** Events that use subagent type as matcher (free-text, no presets). */
const SUBAGENT_MATCHER_EVENTS = new Set(["SubagentStart", "SubagentStop"]);

function getMatcherCategory(eventType: string): string {
  if (TOOL_MATCHER_EVENTS.has(eventType)) return "tool";
  if (SESSION_MATCHER_EVENTS.has(eventType)) return "session";
  if (COMPACT_MATCHER_EVENTS.has(eventType)) return "compact";
  if (SUBAGENT_MATCHER_EVENTS.has(eventType)) return "subagent";
  return "none";
}

/**
 * Available hook types per CLI.
 *
 * Claude Code supports: command (shell), prompt (LLM eval), agent (sub-agent),
 *   http (POST JSON), mcp_tool (MCP invoke).
 * Qoder / Codex currently only support command in practice.
 */
const HOOK_TYPES_BY_CLI: Record<CliId, { id: string; label: string; desc: string }[]> = {
  claude: [
    { id: "command", label: "命令", desc: "执行 Shell 命令，stdin 传入 JSON" },
    { id: "prompt", label: "提示词", desc: "单轮 LLM 评估，可分析上下文" },
    { id: "agent", label: "Agent", desc: "多轮子 Agent，有工具访问权" },
    { id: "http", label: "HTTP", desc: "POST JSON 到指定 URL" },
    { id: "mcp_tool", label: "MCP 工具", desc: "调用 MCP Server 工具" },
  ],
  qoder: [
    { id: "command", label: "命令", desc: "执行 Shell 命令，stdin 传入 JSON" },
  ],
  codex: [
    { id: "command", label: "命令", desc: "执行 Shell 命令，stdin 传入 JSON" },
  ],
};

/* ──────────────────── Step Indicator ──────────────────── */

function StepDots({ current, total }: { current: number; total: number }) {
  return (
    <div className="flex items-center gap-1 px-4 py-2">
      {Array.from({ length: total }, (_, i) => (
        <div key={i} className="flex items-center gap-1">
          <div
            className={`w-5 h-5 rounded-full flex items-center justify-center text-2xs font-bold font-mono
              ${i < current
                ? "bg-[var(--app-green)] text-white"
                : i === current
                  ? "bg-[var(--app-accent)] text-white"
                  : "bg-[var(--app-bg)] text-[var(--app-text-muted)] border border-[var(--app-border)]"
              }`}
          >
            {i < current ? <Check size={10} /> : i + 1}
          </div>
          {i < total - 1 && <ChevronRight size={12} className="text-[var(--app-text-muted)]" />}
        </div>
      ))}
    </div>
  );
}

/* ──────────────────── Main ──────────────────── */

export function HookWizard({ open, onClose, onCreated }: HookWizardProps) {
  const [step, setStep] = useState(0);
  const [cli, setCli] = useState("");
  const [eventType, setEventType] = useState("");
  const [matcher, setMatcher] = useState("");
  const [commandMode, setCommandMode] = useState<"text" | "file">("text");
  const [command, setCommand] = useState("");
  const [scriptPath, setScriptPath] = useState("");
  const [hookType, setHookType] = useState("command");
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);
  const [copied, setCopied] = useState(false);
  const [hookName, setHookName] = useState("");
  const [hookDescription, setHookDescription] = useState("");

  // Available hook types for the selected CLI, filtered by event type
  const currentHookTypes = useMemo(() => {
    if (!cli) return [] as { id: string; label: string; desc: string }[];
    let types = HOOK_TYPES_BY_CLI[cli as CliId] || [];
    // SessionStart only supports command + mcp_tool
    if (eventType === "SessionStart") {
      types = types.filter((t) => t.id === "command" || t.id === "mcp_tool");
    }
    // If current type is no longer valid, fall back to command
    if (!types.some((t) => t.id === hookType)) {
      setHookType("command");
    }
    return types;
  }, [cli, eventType, hookType]);

  // Events for the currently selected CLI — changes automatically when CLI changes
  const currentEvents = useMemo(() => {
    if (!cli) return [] as HookEventDef[];
    return CLI_EVENTS[cli as CliId] || [];
  }, [cli]);

  // Matcher category and presets for the selected event type
  const matcherCategory = useMemo(() => getMatcherCategory(eventType), [eventType]);
  const matcherPresets = MATCHER_BY_CATEGORY[matcherCategory].presets;
  const matcherHint = MATCHER_BY_CATEGORY[matcherCategory].hint;
  const showMatcher = matcherCategory !== "none";

  if (!open) return null;

  const totalSteps = 4;

  const handleCliChange = (newCli: string) => {
    if (newCli !== cli) {
      setCli(newCli);
      setEventType("");  // reset event when CLI changes — events differ per CLI
      setHookType("command"); // reset to default
    }
  };

  const handleEventTypeChange = (newType: string) => {
    setEventType(newType);
    setMatcher(""); // reset matcher when event changes — valid values differ per event
  };

  const handleNext = () => {
    setError("");
    if (step === 0 && !cli) { setError("请选择 CLI"); return; }
    if (step === 1 && !eventType) { setError("请选择事件类型"); return; }
    if (step === 2) {
      const cmd = commandMode === "file" ? scriptPath : command;
      if (!cmd.trim()) { setError(commandMode === "file" ? "请选择脚本文件" : "请输入命令"); return; }
    }
    setStep((s) => Math.min(s + 1, totalSteps - 1));
  };

  const handleBack = () => {
    setError("");
    setStep((s) => Math.max(s - 1, 0));
  };

  const handleCreate = async () => {
    setError("");
    setSaving(true);
    const finalCommand = commandMode === "file" ? scriptPath.trim() : command.trim();
    try {
      await invoke("create_hook", {
        cli,
        eventType,
        matcher: matcher || "",
        command: finalCommand,
        hookType,
      });
      onCreated(hookName.trim(), hookDescription.trim());
      handleClose();
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : String(e));
    } finally {
      setSaving(false);
    }
  };

  const handleClose = () => {
    setStep(0);
    setCli("");
    setEventType("");
    setMatcher("");
    setHookType("command");
    setCommandMode("text");
    setCommand("");
    setScriptPath("");
    setError("");
    setCopied(false);
    setHookName("");
    setHookDescription("");
    onClose();
  };

  const handlePickFile = async () => {
    try {
      const selected = await openDialog({
        multiple: false,
        filters: [{ name: "Scripts", extensions: ["sh", "py", "js", "ts", "bash", "zsh"] }],
      });
      if (selected) {
        setScriptPath(typeof selected === "string" ? selected : "");
      }
    } catch (e) {
      console.error("File picker error:", e);
      setError(`文件选择失败: ${String(e)}`);
    }
  };

  const finalCommand = commandMode === "file" ? scriptPath : command;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm animate-[fadeIn_100ms_ease-out]"
      onClick={(e) => e.target === e.currentTarget && handleClose()}
    >
      <div className="bg-[var(--app-panel)] border border-[var(--app-border)] shadow-dialog w-[840px] animate-[scaleIn_150ms_ease-out] flex flex-col max-h-[80vh]">
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-[var(--app-border)]">
          <div className="flex items-center gap-2">
            <Terminal size={14} className="text-[var(--app-accent)]" />
            <span className="text-sm font-mono text-[var(--app-text)]">创建 Hook</span>
          </div>
          <button onClick={handleClose} className="p-1 text-[var(--app-text-muted)] hover:text-[var(--app-text)] transition-colors">
            <X size={14} />
          </button>
        </div>

        {/* Step dots */}
        <StepDots current={step} total={totalSteps} />

        {/* Body */}
        <div className="flex-1 overflow-y-auto p-4">
          {/* Step 0: Select CLI */}
          {step === 0 && (
            <div>
              <label className="block text-xs text-[var(--app-text-dim)] mb-3 font-mono">
                <span className="text-[var(--app-text-muted)]">$ </span>选择 CLI 工具
              </label>
              <div className="space-y-2">
                {CLI_TOOLS.map((tool) => (
                  <label
                    key={tool.id}
                    className={`flex items-center gap-3 p-3 border rounded cursor-pointer transition-colors
                      ${cli === tool.id
                        ? "border-[var(--app-accent)] bg-[var(--app-accent)]/5"
                        : "border-[var(--app-border-light)] hover:bg-[var(--app-hover)]"
                      }`}
                  >
                    <input type="radio" name="cli" value={tool.id} checked={cli === tool.id} onChange={() => handleCliChange(tool.id)} className="hidden" />
                    <div className={`w-8 h-8 rounded flex items-center justify-center font-mono text-xs font-bold
                      ${cli === tool.id ? "bg-[var(--app-accent)] text-white" : "bg-[var(--app-bg)] text-[var(--app-text-dim)]"}`}>
                      {tool.name[0]}
                    </div>
                    <div className="flex-1">
                      <div className="text-sm font-mono text-[var(--app-text)]">{tool.name}</div>
                      <div className="text-2xs text-[var(--app-text-muted)] font-mono">{tool.desc}</div>
                    </div>
                    {cli === tool.id && <Check size={14} className="text-[var(--app-accent)]" />}
                  </label>
                ))}
              </div>
            </div>
          )}

          {/* Step 1: Select Event Type */}
          {step === 1 && (
            <div>
              <label className="block text-xs text-[var(--app-text-dim)] mb-3 font-mono">
                <span className="text-[var(--app-text-muted)]">$ </span>选择事件类型
                <span className="text-[var(--app-text-muted)] ml-2">({cli.toUpperCase()})</span>
              </label>
              <div className="space-y-2">
                {currentEvents.map((et) => (
                  <label
                    key={et.id}
                    className={`flex items-start gap-3 p-3 border rounded cursor-pointer transition-colors
                      ${eventType === et.id
                        ? "border-[var(--app-accent)] bg-[var(--app-accent)]/5"
                        : "border-[var(--app-border-light)] hover:bg-[var(--app-hover)]"
                      }`}
                  >
                    <input type="radio" name="eventType" value={et.id} checked={eventType === et.id} onChange={() => handleEventTypeChange(et.id)} className="hidden" />
                    <div className="flex-1">
                      <div className="flex items-center gap-2">
                        <span className="text-sm font-mono text-[var(--app-text)]">{et.label}</span>
                        <span className="text-xs font-mono text-[var(--app-accent)]">{et.id}</span>
                        {et.blockable && (
                          <span className="text-2xs text-[var(--app-amber)] font-mono px-1 py-px border border-[var(--app-amber)] rounded">可拦截</span>
                        )}
                      </div>
                      <div className="text-2xs text-[var(--app-text-muted)] font-mono mt-0.5">{et.desc}</div>
                    </div>
                    {eventType === et.id && <Check size={14} className="text-[var(--app-accent)] shrink-0 mt-0.5" />}
                  </label>
                ))}
              </div>
            </div>
          )}

          {/* Step 2: Configure */}
          {step === 2 && (
            <div>
              <label className="block text-xs text-[var(--app-text-dim)] mb-3 font-mono">
                <span className="text-[var(--app-text-muted)]">$ </span>配置参数
              </label>

              {/* Name & Description */}
              <div className="mb-4">
                <label className="block text-xs text-[var(--app-text-dim)] mb-1.5 font-mono">
                  <span className="text-[var(--app-text-muted)]">名称 </span>
                  给 Hook 起个名字 <span className="text-[var(--app-text-muted)]">(可选)</span>
                </label>
                <input
                  type="text"
                  value={hookName}
                  onChange={(e) => setHookName(e.target.value)}
                  placeholder="例如：阻止 rm -rf"
                  className="w-full h-[28px] text-xs font-mono px-2 bg-[var(--app-input)] border border-[var(--app-border)] rounded focus:border-[var(--app-accent)] focus:outline-none"
                />
                <label className="block text-xs text-[var(--app-text-dim)] mt-2 mb-1.5 font-mono">
                  <span className="text-[var(--app-text-muted)]">描述 </span>
                  <span className="text-[var(--app-text-muted)]">(可选)</span>
                </label>
                <textarea
                  value={hookDescription}
                  onChange={(e) => setHookDescription(e.target.value)}
                  placeholder="这个 Hook 的功能说明…"
                  rows={2}
                  className="w-full text-xs font-mono px-2 py-1.5 bg-[var(--app-input)] border border-[var(--app-border)] rounded focus:border-[var(--app-accent)] focus:outline-none resize-none"
                />
              </div>

              {/* Matcher — only shown for events that support it */}
              {showMatcher && (
              <div className="mb-4">
                <label className="block text-xs text-[var(--app-text-dim)] mb-1.5 font-mono">
                  <span className="text-[var(--app-text-muted)]">matcher: </span>匹配条件
                </label>
                  {matcherPresets.length > 0 && (
                    <div className="flex flex-wrap gap-1 mb-2">
                      {matcherPresets.map((m) => {
                        const active = m === "*"
                          ? matcher === ""
                          : matcher.split("|").includes(m);
                        return (
                          <button
                            key={m}
                            onClick={() => {
                              if (m === "*") { setMatcher(""); return; }
                              const parts = matcher ? matcher.split("|").filter(Boolean) : [];
                              const idx = parts.indexOf(m);
                              if (idx >= 0) parts.splice(idx, 1);
                              else parts.push(m);
                              setMatcher(parts.join("|"));
                            }}
                            className={`text-2xs font-mono px-2 py-1 border rounded transition-colors
                              ${active
                                ? "border-[var(--app-accent)] bg-[var(--app-accent)]/10 text-[var(--app-accent)]"
                                : "border-[var(--app-border-light)] text-[var(--app-text-dim)] hover:bg-[var(--app-hover)]"
                              }`}
                          >
                            {m}
                          </button>
                        );
                      })}
                    </div>
                  )}
                  <input
                    type="text"
                    value={matcher}
                    onChange={(e) => setMatcher(e.target.value)}
                    placeholder={matcherHint || "留空 = 匹配所有"}
                    className="w-full h-[28px] text-xs font-mono px-2 bg-[var(--app-input)] border border-[var(--app-border)] rounded focus:border-[var(--app-accent)] focus:outline-none"
                  />
              </div>
              )}

              {/* Hook type selector */}
              {currentHookTypes.length > 1 && (
                <div className="mb-4">
                  <label className="block text-xs text-[var(--app-text-dim)] mb-1.5 font-mono">
                    <span className="text-[var(--app-text-muted)]">type: </span>Hook 类型
                  </label>
                  <div className="flex flex-wrap gap-1">
                    {currentHookTypes.map((t) => (
                      <button
                        key={t.id}
                        onClick={() => setHookType(t.id)}
                        className={`text-2xs font-mono px-2 py-1 border rounded transition-colors
                          ${hookType === t.id
                            ? "border-[var(--app-accent)] bg-[var(--app-accent)]/10 text-[var(--app-accent)]"
                            : "border-[var(--app-border-light)] text-[var(--app-text-dim)] hover:bg-[var(--app-hover)]"
                          }`}
                        title={t.desc}
                      >
                        {t.label} <span className="opacity-50">{t.id}</span>
                      </button>
                    ))}
                  </div>
                </div>
              )}

              {/* Command mode toggle */}
              <label className="block text-xs text-[var(--app-text-dim)] mb-1.5 font-mono">
                <span className="text-[var(--app-text-muted)]">command: </span>执行命令 <span className="text-[var(--app-red)]">*</span>
              </label>
              <div className="flex gap-2 mb-3">
                <button
                  onClick={() => setCommandMode("text")}
                  className={`flex-1 p-2 border rounded text-xs font-mono transition-colors
                    ${commandMode === "text" ? "border-[var(--app-accent)] bg-[var(--app-accent)]/5 text-[var(--app-accent)]" : "border-[var(--app-border-light)] text-[var(--app-text-dim)] hover:bg-[var(--app-hover)]"}`}
                >
                  直接输入
                </button>
                <button
                  onClick={() => setCommandMode("file")}
                  className={`flex-1 p-2 border rounded text-xs font-mono transition-colors
                    ${commandMode === "file" ? "border-[var(--app-accent)] bg-[var(--app-accent)]/5 text-[var(--app-accent)]" : "border-[var(--app-border-light)] text-[var(--app-text-dim)] hover:bg-[var(--app-hover)]"}`}
                >
                  选择脚本文件
                </button>
              </div>

              {commandMode === "text" ? (
                <textarea
                  value={command}
                  onChange={(e) => setCommand(e.target.value)}
                  placeholder="输入 shell 命令..."
                  rows={4}
                  className="w-full text-xs font-mono px-2 py-1.5 bg-[var(--app-input)] border border-[var(--app-border)] rounded focus:border-[var(--app-accent)] focus:outline-none resize-none"
                />
              ) : (
                <div className="flex items-center gap-2">
                  <input
                    type="text"
                    value={scriptPath}
                    onChange={(e) => setScriptPath(e.target.value)}
                    placeholder="脚本文件路径..."
                    className="flex-1 h-[28px] text-xs font-mono px-2 bg-[var(--app-input)] border border-[var(--app-border)] rounded focus:border-[var(--app-accent)] focus:outline-none"
                  />
                  <button
                    onClick={handlePickFile}
                    className="p-1.5 border border-[var(--app-border)] rounded text-[var(--app-text-muted)] hover:text-[var(--app-text)] hover:bg-[var(--app-hover)] transition-colors"
                    title="选择文件"
                  >
                    <FolderOpen size={14} />
                  </button>
                </div>
              )}

              {/* Preview */}
              {finalCommand && (() => {
                const previewText = cli === "codex"
                  ? `[[hooks.${eventType}]]\nmatcher = "${matcher || ""}"\n\n[[hooks.${eventType}.hooks]]\ntype = "${hookType}"\ncommand = "${finalCommand}"`
                  : `"${eventType}": [{\n  "matcher": "${matcher}",\n  "hooks": [{\n    "type": "${hookType}",\n    "command": "${finalCommand}"\n  }]\n}]`;
                return (
                <div className="mt-4 p-3 bg-[var(--app-bg)] border border-[var(--app-border-light)] rounded">
                  <div className="flex items-center justify-between mb-1">
                    <span className="text-2xs text-[var(--app-text-muted)] font-mono">即将写入的配置：</span>
                    <button
                      onClick={() => {
                        navigator.clipboard.writeText(previewText);
                        setCopied(true);
                        setTimeout(() => setCopied(false), 2000);
                      }}
                      className="flex items-center gap-1 text-2xs font-mono text-[var(--app-text-muted)] hover:text-[var(--app-accent)] transition-colors"
                    >
                      {copied ? <Check size={10} /> : <Copy size={10} />}
                      <span>{copied ? "已复制" : "复制"}</span>
                    </button>
                  </div>
                  <pre className="text-2xs text-[var(--app-text-dim)] font-mono whitespace-pre-wrap">
                    {previewText}
                  </pre>
                </div>
                );
              })()}
            </div>
          )}

          {/* Step 3: Confirm */}
          {step === 3 && (
            <div>
              <label className="block text-xs text-[var(--app-text-dim)] mb-3 font-mono">
                <span className="text-[var(--app-text-muted)]">$ </span>确认创建
              </label>
              <div className="p-4 border border-[var(--app-border-light)] rounded bg-[var(--app-bg)]">
                <div className="space-y-2 text-xs font-mono">
                  {hookName && (
                    <div className="flex gap-2">
                      <span className="text-[var(--app-text-muted)] w-20">名称:</span>
                      <span className="text-[var(--app-accent)]">{hookName}</span>
                    </div>
                  )}
                  {hookDescription && (
                    <div className="flex gap-2">
                      <span className="text-[var(--app-text-muted)] w-20">描述:</span>
                      <span className="text-[var(--app-text-dim)]">{hookDescription}</span>
                    </div>
                  )}
                  <div className="flex gap-2">
                    <span className="text-[var(--app-text-muted)] w-20">CLI:</span>
                    <span className="text-[var(--app-text)]">{cli}</span>
                  </div>
                  <div className="flex gap-2">
                    <span className="text-[var(--app-text-muted)] w-20">事件:</span>
                    <span className="text-[var(--app-text)]">{eventType}</span>
                  </div>
                  <div className="flex gap-2">
                    <span className="text-[var(--app-text-muted)] w-20">Matcher:</span>
                    <span className="text-[var(--app-text)]">{matcher || "*"}</span>
                  </div>
                  <div className="flex gap-2">
                    <span className="text-[var(--app-text-muted)] w-20">Type:</span>
                    <span className="text-[var(--app-text)]">{hookType}</span>
                  </div>
                  <div className="flex gap-2">
                    <span className="text-[var(--app-text-muted)] w-20">Command:</span>
                    <span className="text-[var(--app-text)] break-all">{finalCommand}</span>
                  </div>
                </div>
              </div>
              <div className="mt-3 p-3 border border-[var(--app-amber)] bg-[var(--app-amber-bg)] rounded">
                <div className="flex items-start gap-2">
                  <AlertCircle size={12} className="text-[var(--app-amber)] shrink-0 mt-0.5" />
                  <div className="text-2xs text-[var(--app-text-dim)] font-mono">
                    {cli === "codex"
                      ? "将写入 ~/.codex/config.toml，并确保 features.hooks = true"
                      : `将写入 ${cli === "claude" ? "~/.claude/settings.json" : "~/.qoder-cn/settings.json"}`
                    }
                  </div>
                </div>
              </div>
            </div>
          )}

          {/* Error */}
          {error && (
            <div className="mx-4 mb-2 px-3 py-2 bg-[var(--app-red-bg)] border border-[var(--app-red-bg)] text-xs text-[var(--app-red)] flex items-center gap-2 font-mono rounded">
              <AlertCircle size={12} className="shrink-0" />
              {error}
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="flex items-center justify-between px-4 py-3 border-t border-[var(--app-border)] bg-[var(--app-subtle)]">
          <div className="flex-1" />
          <div className="flex items-center gap-2">
            <button onClick={handleClose} className="px-3 py-1.5 text-xs font-mono text-[var(--app-text-muted)] hover:text-[var(--app-text)] transition-colors">
              取消
            </button>
            {step > 0 && step < totalSteps - 1 && (
              <button onClick={handleBack} className="px-3 py-1.5 text-xs font-mono text-[var(--app-text-dim)] border border-[var(--app-border)] rounded hover:bg-[var(--app-hover)] transition-colors">
                上一步
              </button>
            )}
            {step < totalSteps - 1 ? (
              <button onClick={handleNext} className="px-4 py-1.5 text-xs font-mono text-[var(--app-bg)] bg-[var(--app-accent)] rounded hover:opacity-90 transition-opacity">
                下一步
              </button>
            ) : (
              <button onClick={handleCreate} disabled={saving} className="px-4 py-1.5 text-xs font-mono text-[var(--app-bg)] bg-[var(--app-accent)] rounded hover:opacity-90 transition-opacity disabled:opacity-50">
                {saving ? "创建中..." : "创建"}
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
