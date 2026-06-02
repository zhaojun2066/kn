import React, { useState, useRef, useEffect } from "react";
import { X, ChevronRight, ChevronLeft, Check, Terminal, Trash2, Play, Copy } from "lucide-react";
import { Button } from "./common/Button";

interface ProfileDialogProps {
  open: boolean;
  onClose: () => void;
  onAdd: (name: string, desc: string | undefined, env: Record<string, string>) => Promise<void>;
  onRunCommand?: (cmd: string) => void;
  allTags?: string[];
}

type CLI = "anthropic" | "openai" | "both";

const CLI_OPTIONS = [
  { value: "anthropic" as const, label: "Claude Code", desc: "兼容 Anthropic 协议的三方服务（如 DeepSeek、OpenRouter 等）", icon: "C" },
  { value: "openai" as const, label: "Codex", desc: "兼容 OpenAI 协议的三方服务", icon: "O" },
  { value: "both" as const, label: "两者", desc: "同时用于 Claude Code 和 Codex", icon: "C+O" },
];

/* ── Env var templates per CLI type ──────────────────────── */
function getEnvTemplate(cli: CLI): [string, string][] {
  const anthropic: [string, string][] = [
    ["ANTHROPIC_AUTH_TOKEN", ""],
    ["ANTHROPIC_BASE_URL", ""],
    ["ANTHROPIC_MODEL", ""],
    ["ANTHROPIC_DEFAULT_HAIKU_MODEL", ""],
    ["ANTHROPIC_DEFAULT_SONNET_MODEL", ""],
    ["ANTHROPIC_DEFAULT_OPUS_MODEL", ""],
    ["DISABLE_AUTOUPDATER", ""],
  ];
  const openai: [string, string][] = [
    ["OPENAI_API_KEY", ""],
    ["OPENAI_BASE_URL", ""],
    ["OPENAI_MODEL", ""],  // 注意：Codex 可能不读取此变量，视版本而定
  ];
  if (cli === "anthropic") return anthropic;
  if (cli === "openai") return openai;
  return [...anthropic, ...openai];
}

/* ── ProfileDialog ──────────────────────────────────────── */
export function ProfileDialog({ open, onClose, onAdd, onRunCommand, allTags = [] }: ProfileDialogProps) {
  const [step, setStep] = useState<1 | 2 | 3 | 4>(1);
  const [name, setName] = useState("");
  const [desc, setDesc] = useState("");
  const [cli, setCLI] = useState<CLI>("anthropic");
  const [envVars, setEnvVars] = useState<[string, string][]>(() => getEnvTemplate("anthropic"));
  const [tags, setTags] = useState<string[]>([]);
  const [tagInput, setTagInput] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const [created, setCreated] = useState(false);
  const [copiedCmd, setCopiedCmd] = useState<string | null>(null);
  const nameRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (open) {
      setName(""); setDesc(""); setCLI("anthropic");
      setEnvVars(getEnvTemplate("anthropic"));
      setTags([]); setTagInput(""); setError(""); setCreated(false); setStep(1);
      setTimeout(() => nameRef.current?.focus(), 50);
    }
  }, [open]);

  if (!open) return null;

  const validateName = () => {
    if (!name.trim().match(/^[a-z0-9]([a-z0-9-]*[a-z0-9])?$/)) {
      setError("名称只能包含小写字母、数字和连字符（如 my-provider）");
      return false;
    }
    setError("");
    return true;
  };

  const goStep2 = () => { if (validateName()) setStep(2); };
  const goStep3 = () => {
    setEnvVars(getEnvTemplate(cli));
    setStep(3);
  };

  /* ── Build commands for completion step ───────────────── */
  const buildCommands = (profileName: string): { label: string; cmd: string }[] => {
    const cmds: { label: string; cmd: string }[] = [];
    if (cli === "anthropic" || cli === "both") cmds.push({ label: "Claude Code", cmd: `ai claude ${profileName}` });
    if (cli === "openai" || cli === "both") cmds.push({ label: "Codex", cmd: `ai codex ${profileName}` });
    cmds.push({ label: "查看环境变量", cmd: `profile env ${profileName}` });
    cmds.push({ label: "查看详情", cmd: `profile show ${profileName}` });
    return cmds;
  };

  const handleCreate = async () => {
    setSaving(true);
    setError("");
    try {
      const env: Record<string, string> = {};
      for (const [k, v] of envVars) {
        if (v.trim()) env[k] = v.trim();
      }
      // Store CLI type for sidebar icon
      const cliTypeMap: Record<CLI, string> = { anthropic: "claude", openai: "codex", both: "both" };
      env["_KN_CLI_TYPE"] = cliTypeMap[cli];
      // Store tags
      if (tags.length > 0) env["_KN_TAGS"] = tags.join(",");
      await onAdd(name.trim(), desc.trim() || undefined, env);
      setCreated(true);
      setStep(4);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  const copyCmd = async (cmd: string) => {
    try {
      await navigator.clipboard.writeText(cmd);
      setCopiedCmd(cmd);
      setTimeout(() => setCopiedCmd(null), 2000);
    } catch { /* */ }
  };

  const updateEnvValue = (idx: number, value: string) => {
    setEnvVars((prev) => prev.map((row, i) => (i === idx ? [row[0], value] : row)));
  };

  const deleteEnvRow = (idx: number) => {
    setEnvVars((prev) => prev.filter((_, i) => i !== idx));
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") onClose();
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm animate-[fadeIn_100ms_ease-out]"
      onClick={(e) => e.target === e.currentTarget && onClose()}
    >
      <div
        onKeyDown={handleKeyDown}
        className="bg-app-panel border border-app-border shadow-dialog w-[560px] animate-[scaleIn_150ms_ease-out]"
      >
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-app-border">
          <div className="flex items-center gap-2">
            <Terminal size={15} className="text-app-accent" />
            <h3 className="font-semibold text-sm font-mono">
              <span className="text-app-accent opacity-60">$ </span>
              新建 Profile
            </h3>
          </div>
          <button onClick={onClose} className="p-1 text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)] transition-colors">
            <X size={14} />
          </button>
        </div>

        {/* Stepper */}
        <div className="flex items-center gap-2 px-4 py-2 border-b border-app-border-light bg-[var(--app-subtle)]">
          <StepDot num={1} active={step === 1} done={step > 1} label="命名" />
          <ChevronRight size={11} className="text-app-text-muted" />
          <StepDot num={2} active={step === 2} done={step > 2} label="CLI" />
          <ChevronRight size={11} className="text-app-text-muted" />
          <StepDot num={3} active={step === 3} done={step > 3} label="变量" />
          <ChevronRight size={11} className="text-app-text-muted" />
          <StepDot num={4} active={step === 4} done={false} label="完成" />
        </div>

        {/* Body */}
        <div className="p-4" style={{ minHeight: step === 3 ? "300px" : step === 4 ? "auto" : "160px", maxHeight: step === 3 ? "380px" : "420px", overflowY: "auto" }}>
          {/* ── Step 1: Name ─────────────────────────────── */}
          {step === 1 && (
            <>
              <div>
                <label className="block text-xs text-app-text-dim mb-1.5 font-mono">
                  <span className="text-app-text-muted"># </span>
                  Profile 名称 <span className="text-app-red">*</span>
                </label>
                <input
                  ref={nameRef}
                  value={name}
                  onChange={(e) => { setName(e.target.value); setError(""); }}
                  className="w-full h-[30px] text-sm font-mono"
                  placeholder="my-provider"
                  spellCheck={false}
                />
                <p className="text-2xs text-app-text-muted mt-1 font-mono">
                  仅限小写字母、数字和连字符
                </p>
              </div>
              <div>
                <label className="block text-xs text-app-text-dim mb-1.5 font-mono">
                  <span className="text-app-text-muted"># </span>
                  描述
                </label>
                <input
                  value={desc}
                  onChange={(e) => setDesc(e.target.value)}
                  className="w-full h-[30px] text-sm font-mono"
                  placeholder="例：DeepSeek 中转"
                />
              </div>
              <div>
                <label className="block text-xs text-app-text-dim mb-1.5 font-mono">
                  <span className="text-app-text-muted"># </span>
                  标签 <span className="text-app-text-muted">(最多3个)</span>
                </label>
                <div className="flex items-center gap-1 flex-wrap mb-1">
                  {tags.map((t) => (
                    <span key={t} className="flex items-center gap-0.5 text-xs px-1.5 py-0.5 font-mono bg-[var(--app-selected)] text-app-accent border border-app-accent/30">
                      {t}
                      <button onClick={() => setTags(tags.filter((x) => x !== t))} className="text-app-text-dim hover:text-app-red"><X size={10} /></button>
                    </span>
                  ))}
                  {tags.length < 3 ? (
                    <input
                      value={tagInput}
                      onChange={(e) => setTagInput(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") {
                          e.preventDefault();
                          const t = tagInput.trim();
                          if (t && !tags.includes(t)) { setTags([...tags, t]); setTagInput(""); }
                        }
                      }}
                      className="h-[24px] w-[110px] text-xs font-mono"
                      placeholder="输入后回车"
                      spellCheck={false}
                    />
                  ) : (
                    <span className="text-2xs text-app-text-muted font-mono">最多 3 个</span>
                  )}
                </div>
                {allTags.length > 0 && (
                  <div className="flex items-center gap-1 flex-wrap">
                    <span className="text-2xs text-app-text-muted font-mono shrink-0">已有:</span>
                    {allTags.map((t) => (
                      <button
                        key={t}
                        onClick={() => { if (!tags.includes(t) && tags.length < 3) setTags([...tags, t]); }}
                        disabled={tags.includes(t)}
                        className={`text-2xs px-1.5 py-0.5 font-mono border transition-colors ${
                          tags.includes(t) ? "opacity-30 cursor-not-allowed border-app-border" : "border-app-border text-app-text-dim hover:text-app-accent hover:border-app-accent bg-[var(--app-input)]"
                        }`}
                      >{t}</button>
                    ))}
                  </div>
                )}
              </div>
            </>
          )}

          {/* ── Step 2: CLI type ──────────────────────────── */}
          {step === 2 && (
            <div>
              <label className="block text-xs text-app-text-dim mb-2 font-mono">
                <span className="text-app-text-muted"># </span>
                选择 CLI 类型
              </label>
              <p className="text-2xs text-app-text-muted mb-3">
                三方服务只要兼容对应协议即可，不限于 Anthropic/OpenAI 官方
              </p>
              <div className="space-y-1.5">
                {CLI_OPTIONS.map((opt) => (
                  <label
                    key={opt.value}
                    className={`flex items-center gap-3 px-3 py-2.5 border cursor-pointer transition-all duration-fast
                      ${cli === opt.value
                        ? "border-app-accent bg-[var(--app-selected)] shadow-[0_0_10px_var(--app-glow)]"
                        : "border-app-border hover:border-[var(--app-border)] hover:bg-[var(--app-hover)]"
                      }`}
                  >
                    <input type="radio" name="cliType" checked={cli === opt.value} onChange={() => setCLI(opt.value)} className="hidden" />
                    <div className={`w-9 h-9 flex items-center justify-center text-sm font-bold shrink-0 font-mono
                      ${cli === opt.value ? "bg-app-accent text-[var(--app-bg)]" : "bg-[var(--app-input)] text-app-text-dim border border-app-border"}`}>
                      {opt.icon}
                    </div>
                    <div>
                      <div className="text-sm text-app-text font-mono">{opt.label}</div>
                      <div className="text-xs text-app-text-muted">{opt.desc}</div>
                    </div>
                    {cli === opt.value && <Check size={16} className="text-app-accent ml-auto" />}
                  </label>
                ))}
              </div>
            </div>
          )}

          {/* ── Step 3: Env vars ──────────────────────────── */}
          {step === 3 && (
            <div>
              <label className="block text-xs text-app-text-dim mb-2 font-mono">
                <span className="text-app-text-muted"># </span>
                环境变量
              </label>
              <p className="text-2xs text-app-text-muted mb-3">
                已预填常用变量模板。填入需要的值，不需要的点 × 删除
              </p>
              {envVars.length === 0 ? (
                <div className="text-xs text-app-text-muted font-mono py-2 text-center space-y-2">
                  <div>所有变量已删除</div>
                  <button
                    onClick={() => setEnvVars([["", ""]])}
                    className="text-2xs text-app-accent hover:underline font-mono"
                  >
                    + 添加变量
                  </button>
                </div>
              ) : (
                <div className="space-y-1 max-h-[240px] overflow-y-auto">
                  {envVars.map(([key, val], idx) => (
                    <div key={`${key}-${idx}`} className="flex items-center gap-2">
                      <span className="w-[200px] shrink-0 font-mono text-xs text-app-accent truncate select-all">
                        {key}
                      </span>
                      <span className="text-app-text-muted text-xs shrink-0">=</span>
                      <input
                        value={val}
                        onChange={(e) => updateEnvValue(idx, e.target.value)}
                        className="flex-1 h-[26px] text-xs font-mono"
                        placeholder={
                          key === "DISABLE_AUTOUPDATER" ? "1 或 0" :
                          key.endsWith("TOKEN") || key.endsWith("KEY") ? "sk-xxx" : ""
                        }
                        spellCheck={false}
                      />
                      <button
                        onClick={() => deleteEnvRow(idx)}
                        className="p-0.5 text-app-text-dim hover:text-app-red hover:bg-app-red-bg transition-colors shrink-0"
                        title="删除此变量"
                      >
                        <X size={12} />
                      </button>
                    </div>
                  ))}
                </div>
              )}
              <button
                onClick={() => setEnvVars((prev) => [...prev, ["", ""]])}
                className="text-2xs text-app-text-muted hover:text-app-accent font-mono mt-2 transition-colors"
              >
                + 添加变量
              </button>
            </div>
          )}
        </div>

        {/* Step 4: Done */}
        {step === 4 && (
          <div className="text-center px-4">
            <div className="w-10 h-10 rounded-full bg-app-green-bg border border-app-border flex items-center justify-center mx-auto mb-2">
              <Check size={20} className="text-app-green" />
            </div>
            <div className="text-sm text-app-text font-mono font-semibold mb-0.5">Profile 创建成功</div>
            <div className="text-xs text-app-text-dim font-mono mb-3">
              <span className="text-app-accent">$ </span>{name}
            </div>

            <div className="text-left border border-app-border bg-[var(--app-cmd-bg)]">
              <div className="px-3 py-1 border-b border-app-border bg-[var(--app-cmd-header)]">
                <span className="text-2xs text-app-text-muted uppercase tracking-wider">使用命令</span>
              </div>
              <div className="px-2 py-1.5 space-y-0.5">
                {buildCommands(name).map(({ label, cmd }) => (
                  <div key={cmd} className="flex items-center justify-between group/item px-2 py-1 hover:bg-[var(--app-hover)] transition-colors">
                    <div className="min-w-0 flex items-center gap-2">
                      <span className="text-2xs text-app-text-muted shrink-0">{label}</span>
                      <code className="text-xs text-app-text font-mono select-all truncate">
                        <span className="text-app-accent opacity-70 mr-1">$</span>{cmd}
                      </code>
                    </div>
                    <div className="flex items-center gap-0.5 opacity-0 group-hover/item:opacity-100 shrink-0 ml-2">
                      {onRunCommand && (
                        <button onClick={() => { onRunCommand(cmd); onClose(); }}
                          className="flex items-center gap-1 px-2 py-0.5 text-2xs text-app-text-dim hover:text-app-green border border-transparent hover:border-app-border bg-transparent hover:bg-[var(--app-hover)]">
                          <Play size={10} />运行
                        </button>
                      )}
                      <button onClick={() => copyCmd(cmd)}
                        className="flex items-center gap-1 px-2 py-0.5 text-2xs text-app-text-dim hover:text-app-accent border border-transparent hover:border-app-border bg-transparent hover:bg-[var(--app-hover)]">
                        {copiedCmd === cmd ? <><Check size={10} className="text-app-green" />已复制</> : <><Copy size={10} />复制</>}
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          </div>
        )}

        {/* Error */}
        {error && (
          <div className="mx-4 mb-2 px-3 py-2 bg-app-red-bg border border-[var(--app-red-bg)] text-xs text-app-red flex items-center gap-2 font-mono">
            <span>!</span>
            <span>{error}</span>
          </div>
        )}

        {/* Footer */}
        <div className="flex justify-between items-center px-4 py-3 border-t border-app-border bg-[var(--app-subtle)]">
          <div className="flex gap-1.5">
            <span className={`w-2 h-2 transition-colors ${step >= 1 ? "bg-app-accent shadow-[0_0_6px_var(--app-glow)]" : "bg-[var(--app-border)]"}`} />
            <span className={`w-2 h-2 transition-colors ${step >= 2 ? "bg-app-accent shadow-[0_0_6px_var(--app-glow)]" : "bg-[var(--app-border)]"}`} />
            <span className={`w-2 h-2 transition-colors ${step >= 3 ? "bg-app-accent shadow-[0_0_6px_var(--app-glow)]" : "bg-[var(--app-border)]"}`} />
            <span className={`w-2 h-2 transition-colors ${step >= 4 ? "bg-app-accent shadow-[0_0_6px_var(--app-glow)]" : "bg-[var(--app-border)]"}`} />
          </div>
          <div className="flex gap-2">
            <Button variant="ghost" size="sm" onClick={onClose}>取消</Button>
            {step > 1 && step < 4 && (
              <Button variant="ghost" size="sm" onClick={() => setStep((step - 1) as 1 | 2 | 3 | 4)}>
                <ChevronLeft size={13} />上一步
              </Button>
            )}
            {step === 1 && (
              <Button variant="primary" size="sm" onClick={goStep2}>下一步<ChevronRight size={13} /></Button>
            )}
            {step === 2 && (
              <Button variant="primary" size="sm" onClick={goStep3}>下一步<ChevronRight size={13} /></Button>
            )}
            {step === 3 && (
              <Button variant="primary" size="sm" onClick={handleCreate} disabled={saving}>
                {saving ? "创建中..." : "创建 Profile"}
              </Button>
            )}
            {step === 4 && (
              <Button variant="primary" size="sm" onClick={onClose}>完成</Button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

function StepDot({ num, active, done, label }: { num: number; active: boolean; done: boolean; label: string }) {
  return (
    <div className={`flex items-center gap-1.5 font-mono ${active ? "text-app-text" : "text-app-text-muted"}`}>
      <span className={`w-5 h-5 flex items-center justify-center text-2xs font-bold
        ${active
          ? "bg-app-accent text-[var(--app-bg)] shadow-[0_0_8px_var(--app-glow)]"
          : done
            ? "bg-app-green text-[var(--app-bg)]"
            : "bg-[var(--app-hover)] text-app-text-dim border border-app-border"
        }`}
      >
        {done ? <Check size={10} /> : num}
      </span>
      <span className="text-2xs">{label}</span>
    </div>
  );
}
