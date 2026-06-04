import React, { useState, useRef, useEffect } from "react";
import { X, ChevronRight, ChevronLeft, Check, Terminal, Trash2, Play, Copy, Download } from "lucide-react";
import { Button } from "./common/Button";
import { CLIIcon } from "./common/CLIIcon";
import { CLI_TOOLS, CLIToolDef, ProviderPreset, getToolById, getEnvTemplate, getEnvVarDef } from "../lib/provider-presets";
import { detectKeyFormat } from "../lib/key-format-detector";

interface ProfileDialogProps {
  open: boolean;
  onClose: () => void;
  onAdd: (name: string, desc: string | undefined, env: Record<string, string>) => Promise<void>;
  onRunCommand?: (cmd: string) => void;
  onInstallTool?: (cmd: string) => void;
  allTags?: string[];
  existingNames?: string[];
  envCheck?: { items: { name: string; label: string; status: string; detail: string; install_cmd?: string }[]; all_ok: boolean } | null;
}

/* ── ProfileDialog ──────────────────────────────────────── */
export function ProfileDialog({ open, onClose, onAdd, onRunCommand, onInstallTool, allTags = [], existingNames = [], envCheck }: ProfileDialogProps) {
  const [step, setStep] = useState<1 | 2 | 3 | 4 | 5>(1);
  const [name, setName] = useState("");
  const [desc, setDesc] = useState("");
  const [toolId, setToolId] = useState<string>("claude");
  const [providerId, setProviderId] = useState<string | null>(null);
  const [envVars, setEnvVars] = useState<[string, string][]>(() => getEnvTemplate("claude", "anthropic-official"));
  const [tags, setTags] = useState<string[]>([]);
  const [tagInput, setTagInput] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const [created, setCreated] = useState(false);
  const [copiedCmd, setCopiedCmd] = useState<string | null>(null);
  const nameRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (open) {
      setName(""); setDesc(""); setToolId("claude"); setProviderId(null);
      setEnvVars(getEnvTemplate("claude", "anthropic-official"));
      setTags([]); setTagInput(""); setError(""); setCreated(false); setStep(1);
      setTimeout(() => nameRef.current?.focus(), 50);
    }
  }, [open]);

  if (!open) return null;

  /* ── Tool availability from env check ──────────────────────── */
  const toolStatusMap: Record<string, { installed: boolean; installCmd?: string }> = {};
  if (envCheck) {
    for (const item of envCheck.items) {
      if (CLI_TOOLS.some((t) => t.id === item.name)) {
        toolStatusMap[item.name] = {
          installed: item.status === "ok",
          installCmd: item.install_cmd,
        };
      }
    }
  }
  const isToolInstalled = (id: string) => !envCheck || toolStatusMap[id]?.installed !== false;
  const canProceed = isToolInstalled(toolId);

  const validateName = () => {
    const trimmed = name.trim();
    if (!trimmed.match(/^[a-z0-9]([a-z0-9-]*[a-z0-9])?$/)) {
      setError("名称只能包含小写字母、数字和连字符（如 my-provider）");
      return false;
    }
    const reserved = ["claude", "codex", "qoderclicn", "profile", "ai", "help"];
    if (reserved.includes(trimmed)) {
      setError(`"${trimmed}" 是系统保留关键字，不能用作 Profile 名称`);
      return false;
    }
    if (existingNames.includes(trimmed)) {
      setError(`Profile "${trimmed}" 已存在，换个名字吧`);
      return false;
    }
    setError("");
    return true;
  };

  /* ── Navigation ────────────────────────────────────────── */
  const goStep2 = () => {
    // Flush pending tag input before moving on
    const t = tagInput.trim();
    if (t && !tags.includes(t) && tags.length < 3) {
      setTags([...tags, t]);
      setTagInput("");
    }
    if (validateName()) setStep(2);
  };

  const goStep3 = () => {
    if (!isToolInstalled(toolId)) {
      setError(`${getToolById(toolId)?.name ?? toolId} 尚未安装，请先安装后再创建 Profile`);
      return;
    }
    setError("");
    if (!providerId) {
      const tool = getToolById(toolId);
      if (tool && tool.providers.length > 0) {
        setProviderId(tool.providers[0].id);
      }
    }
    setStep(3);
  };

  const goStep4 = () => {
    const resolvedId = providerId ?? (getToolById(toolId)?.providers[0]?.id ?? null);
    if (resolvedId) {
      setProviderId(resolvedId);
      setEnvVars(getEnvTemplate(toolId, resolvedId));
    }
    setStep(4);
  };

  /* ── Build commands for completion step ────────────────── */
  const buildCommands = (profileName: string): { label: string; cmd: string }[] => {
    const tool = getToolById(toolId);
    const toolName = tool?.name ?? "CLI";
    const cmds: { label: string; cmd: string }[] = [];
    cmds.push({ label: toolName, cmd: `ai ${toolId} ${profileName}` });
    cmds.push({ label: "查看环境变量", cmd: `profile env ${profileName}` });
    cmds.push({ label: "查看详情", cmd: `profile show ${profileName}` });
    return cmds;
  };

  const handleCreate = async () => {
    const env: Record<string, string> = {};
    for (const [k, v] of envVars) {
      if (v.trim()) env[k] = v.trim();
    }
    // Must have at least one user-defined env var with a value
    const systemKeys = ["_KN_CLI_TYPE", "_KN_TAGS"];
    const userEnvCount = Object.keys(env).filter((k) => !systemKeys.includes(k)).length;
    if (userEnvCount === 0) {
      setError("至少需要填写一个环境变量");
      return;
    }
    setSaving(true);
    setError("");
    try {
      // Store CLI type directly (no mapping needed)
      env["_KN_CLI_TYPE"] = toolId;
      // Store tags
      if (tags.length > 0) env["_KN_TAGS"] = tags.join(",");
      await onAdd(name.trim(), desc.trim() || undefined, env);
      setCreated(true);
      setStep(5);
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

  /* ── Current tool & provider data ──────────────────────── */
  const selectedTool = getToolById(toolId);
  const selectedProvider = providerId ? (selectedTool?.providers.find((p) => p.id === providerId)) : null;
  const filteredProviders = selectedTool?.providers ?? [];

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm animate-[fadeIn_100ms_ease-out]"
    >
      <div
        onKeyDown={handleKeyDown}
        className="bg-app-panel border border-app-border shadow-dialog w-[680px] animate-[scaleIn_150ms_ease-out]"
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
          <StepDot num={2} active={step === 2} done={step > 2} label="工具" />
          <ChevronRight size={11} className="text-app-text-muted" />
          <StepDot num={3} active={step === 3} done={step > 3} label="服务商" />
          <ChevronRight size={11} className="text-app-text-muted" />
          <StepDot num={4} active={step === 4} done={step > 4} label="变量" />
          <ChevronRight size={11} className="text-app-text-muted" />
          <StepDot num={5} active={step === 5} done={false} label="完成" />
        </div>

        {/* Body */}
        <div className="p-4" style={{ minHeight: step === 4 ? "300px" : step === 5 ? "auto" : "160px", maxHeight: step === 4 ? "380px" : "420px", overflowY: "auto" }}>
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

          {/* ── Step 2: CLI Tool selection ────────────────── */}
          {step === 2 && (
            <div>
              <label className="block text-xs text-app-text-dim mb-2 font-mono">
                <span className="text-app-text-muted"># </span>
                选择 CLI 工具
              </label>
              <p className="text-2xs text-app-text-muted mb-3">
                选择你使用的 AI CLI 工具，每个工具支持不同的提供商
              </p>
              <div className="space-y-1.5">
                {CLI_TOOLS.map((tool) => {
                  const installed = isToolInstalled(tool.id);
                  const info = toolStatusMap[tool.id];
                  const installCmd = info?.installCmd;
                  const canInstall = !installed && installCmd && onInstallTool;
                  const Container = installed ? "label" : "div";
                  return (
                  <Container
                    key={tool.id}
                    className={`flex items-center gap-3 px-3 py-2.5 border transition-all duration-fast
                      ${!installed
                        ? "opacity-50 border-app-border bg-[var(--app-subtle)]"
                        : toolId === tool.id
                          ? "border-app-accent bg-[var(--app-selected)] shadow-[0_0_10px_var(--app-glow)] cursor-pointer"
                          : "border-app-border hover:border-[var(--app-border)] hover:bg-[var(--app-hover)] cursor-pointer"
                      }`}
                  >
                    {installed ? (
                      <input type="radio" name="toolType" checked={toolId === tool.id} onChange={() => { setToolId(tool.id); setProviderId(null); setError(""); }} className="hidden" />
                    ) : null}
                    <div className="shrink-0">
                      <CLIIcon type={tool.id} size={28} />
                    </div>
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2">
                        <span className="text-sm text-app-text font-mono">{tool.name}</span>
                        {!installed && (
                          <span className="text-2xs font-mono text-app-red bg-app-red-bg px-1 py-px">未安装</span>
                        )}
                      </div>
                      <div className="text-xs text-app-text-muted truncate">
                        {!installed ? "尚未安装此 CLI 工具" : tool.description}
                      </div>
                    </div>
                    {toolId === tool.id && installed && <Check size={16} className="text-app-accent ml-auto shrink-0" />}
                    {canInstall && (
                      <button
                        onClick={(e) => { e.preventDefault(); e.stopPropagation(); onInstallTool!(installCmd); }}
                        className="shrink-0 flex items-center gap-1 px-2 py-1 text-2xs font-mono
                          text-app-accent border border-app-accent/40 hover:bg-app-accent hover:text-[var(--app-bg)]
                          transition-colors opacity-100"
                        title="一键安装"
                      >
                        <Download size={10} />
                        安装
                      </button>
                    )}
                  </Container>
                  );
                })}
              </div>
            </div>
          )}

          {/* ── Step 3: Provider Preset selection ──────────── */}
          {step === 3 && (
            <div>
              <label className="block text-xs text-app-text-dim mb-2 font-mono">
                <span className="text-app-text-muted"># </span>
                选择提供商预设
              </label>
              <p className="text-2xs text-app-text-muted mb-3">
                为 {selectedTool?.name} 选择合适的服务商。每个预设包含常用的环境变量模板
              </p>
              <div className="space-y-1.5 max-h-[280px] overflow-y-auto">
                {filteredProviders.map((provider) => {
                  const isCustom = provider.id === "custom";
                  const isSelected = providerId === provider.id;
                  // Preview first 2 env var key names
                  const keyPreview = provider.envVars.slice(0, 2).map((v) => v.key).join(", ");
                  return (
                    <label
                      key={provider.id}
                      className={`flex items-center gap-3 px-3 py-2.5 border cursor-pointer transition-all duration-fast
                        ${isSelected
                          ? "border-app-accent bg-[var(--app-selected)] shadow-[0_0_10px_var(--app-glow)]"
                          : "border-app-border hover:border-[var(--app-border)] hover:bg-[var(--app-hover)]"
                        }`}
                    >
                      <input type="radio" name="providerType" checked={isSelected} onChange={() => setProviderId(provider.id)} className="hidden" />
                      <div className={`w-8 h-8 flex items-center justify-center text-xs font-bold shrink-0 font-mono
                        ${isSelected
                          ? isCustom ? "bg-app-amber text-[var(--app-bg)]" : "bg-app-accent text-[var(--app-bg)]"
                          : "bg-[var(--app-input)] text-app-text-dim border border-app-border"
                        }`}
                      >
                        {isCustom ? "⚙" : provider.name.charAt(0)}
                      </div>
                      <div className="min-w-0">
                        <div className="text-sm text-app-text font-mono flex items-center gap-1.5">
                          {provider.name}
                          {isCustom && <span className="text-2xs text-app-amber bg-app-amber-bg px-1 py-px font-mono">自定义</span>}
                        </div>
                        <div className="text-xs text-app-text-muted truncate">{provider.description}</div>
                        <div className="text-2xs text-app-text-muted mt-0.5 font-mono truncate">
                          {keyPreview}
                          {provider.envVars.length > 2 && ` +${provider.envVars.length - 2} 个变量`}
                        </div>
                      </div>
                      {isSelected && <Check size={16} className="text-app-accent ml-auto shrink-0" />}
                    </label>
                  );
                })}
              </div>
            </div>
          )}

          {/* ── Step 4: Env vars ───────────────────────────── */}
          {step === 4 && (
            <div>
              <label className="block text-xs text-app-text-dim mb-2 font-mono">
                <span className="text-app-text-muted"># </span>
                环境变量
                {selectedProvider && (
                  <span className="text-app-text-muted"> — {selectedProvider.name}</span>
                )}
              </label>
              <p className="text-2xs text-app-text-muted mb-3">
                已预填 {selectedProvider?.name ?? "自定义"} 的常用变量模板。填入需要的值，不需要的点 × 删除
              </p>
              {selectedProvider?.note && (
                <div className="mb-3 px-3 py-2 border border-app-amber/30 bg-app-amber-bg/10 text-xs text-app-text-dim font-mono whitespace-pre-line leading-relaxed">
                  {selectedProvider.note}
                </div>
              )}
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
                <div className="space-y-1.5 max-h-[240px] overflow-y-auto">
                  {envVars.map(([key, val], idx) => {
                    const varDef = providerId ? getEnvVarDef(toolId, providerId, key) : undefined;
                    const keyHint = val.trim() ? detectKeyFormat(val) : null;
                    return (
                      <div key={`${key}-${idx}`} className="flex flex-col gap-0.5">
                        <div className="flex items-center gap-2">
                          <span className="w-[260px] shrink-0 font-mono text-xs text-app-accent truncate select-all flex items-center gap-1" title={key}>
                            {key}
                            {varDef?.required && <span className="text-app-red text-2xs">*</span>}
                          </span>
                          <span className="text-app-text-muted text-xs shrink-0">=</span>
                          <div className="flex-1">
                            <input
                              value={val}
                              onChange={(e) => updateEnvValue(idx, e.target.value)}
                              className="w-full h-[26px] text-xs font-mono"
                              placeholder={varDef?.placeholder ?? (key.endsWith("TOKEN") || key.endsWith("KEY") || key.endsWith("API_KEY") ? "sk-xxx" : "")}
                              spellCheck={false}
                            />
                            {keyHint && (
                              <div className="text-[10px] text-app-accent/60 font-mono mt-0.5 ml-0.5 truncate">
                                {keyHint.label}
                              </div>
                            )}
                          </div>
                          <button
                            onClick={() => deleteEnvRow(idx)}
                            className="p-0.5 text-app-text-dim hover:text-app-red hover:bg-app-red-bg transition-colors shrink-0"
                            title="删除此变量"
                          >
                            <X size={12} />
                          </button>
                        </div>
                      </div>
                    );
                  })}
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

        {/* Step 5: Done */}
        {step === 5 && (
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
            <span className={`w-2 h-2 transition-colors ${step >= 5 ? "bg-app-accent shadow-[0_0_6px_var(--app-glow)]" : "bg-[var(--app-border)]"}`} />
          </div>
          <div className="flex gap-2">
            <Button variant="ghost" size="sm" onClick={onClose}>取消</Button>
            {step > 1 && step < 5 && (
              <Button variant="ghost" size="sm" onClick={() => setStep((step - 1) as 1 | 2 | 3 | 4 | 5)}>
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
              <Button variant="primary" size="sm" onClick={goStep4}>下一步<ChevronRight size={13} /></Button>
            )}
            {step === 4 && (
              <Button variant="primary" size="sm" onClick={handleCreate} disabled={saving}>
                {saving ? "创建中..." : "创建 Profile"}
              </Button>
            )}
            {step === 5 && (
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
