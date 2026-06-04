import React, { useState, useEffect, useRef } from "react";
import { X, Check, FileJson } from "lucide-react";
import { Button } from "./common/Button";
import { CLIIcon } from "./common/CLIIcon";

interface ImportData {
  name: string;
  desc?: string;
  env: Record<string, string>;
}

interface Props {
  open: boolean;
  data: ImportData | null;
  onConfirm: (name: string) => Promise<void>;
  onCancel: () => void;
}

const CLI_LABELS: Record<string, string> = {
  claude: "Claude Code",
  codex: "Codex CLI",
  qoderclicn: "Qoder CLI (国内版)",
};

function detectCLI(env: Record<string, string>): string | null {
  // Explicit stored type takes priority (skip legacy "both")
  if (env._KN_CLI_TYPE && env._KN_CLI_TYPE !== "both") return env._KN_CLI_TYPE;
  // Heuristic detection
  const keys = Object.keys(env).map((k) => k.toUpperCase());
  // Qoder uses OPENAI_API_KEY + OPENAI_BASE_URL; distinguish by dashscope endpoint
  if (env.OPENAI_BASE_URL?.includes("dashscope")) return "qoderclicn";
  if (keys.some((k) => k.startsWith("ANTHROPIC_"))) return "claude";
  if (keys.some((k) => k.startsWith("OPENAI_") || k.startsWith("OPENROUTER_"))) return "codex";
  return null;
}

export function ImportPreview({ open, data, onConfirm, onCancel }: Props) {
  const [saving, setSaving] = useState(false);
  const [name, setName] = useState("");
  const nameRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (open && data) {
      setName(data.name);
      setTimeout(() => nameRef.current?.focus(), 50);
    }
  }, [open, data]);

  if (!open || !data) return null;

  const cli = detectCLI(data.env);
  const envCount = Object.keys(data.env).length;

  const handleImport = async () => {
    setSaving(true);
    try {
      await onConfirm(name.trim() || data.name);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm animate-[fadeIn_100ms_ease-out]"
      onClick={(e) => e.target === e.currentTarget && onCancel()}
    >
      <div className="bg-app-panel border border-app-border shadow-dialog w-[480px] animate-[scaleIn_150ms_ease-out]">
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-app-border">
          <div className="flex items-center gap-2">
            <FileJson size={15} className="text-app-accent" />
            <h3 className="font-semibold text-sm font-mono">导入 Profile</h3>
          </div>
          <button onClick={onCancel} className="p-1 text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)] transition-colors">
            <X size={14} />
          </button>
        </div>

        {/* Body */}
        <div className="p-4 space-y-4">
          {/* Name + CLI type */}
          <div>
            <label className="block text-xs text-app-text-dim mb-1.5 font-mono">
              <span className="text-app-text-muted">$ </span>Profile 名称
            </label>
            <div className="flex items-center gap-2">
              <input
                ref={nameRef}
                value={name}
                onChange={(e) => setName(e.target.value)}
                className="flex-1 h-[30px] text-sm font-mono"
                spellCheck={false}
              />
              {cli && (
                <span className="flex items-center gap-1 px-2 py-1 bg-[var(--app-input)] border border-app-border">
                  <CLIIcon type={cli} size={18} />
                  <span className="text-2xs text-app-text-dim font-mono">
                    {CLI_LABELS[cli] ?? cli}
                  </span>
                </span>
              )}
            </div>
            {data.desc && (
              <p className="text-2xs text-app-text-muted mt-1 font-mono">{data.desc}</p>
            )}
          </div>

          {/* Env vars preview */}
          <div>
            <div className="flex items-center justify-between mb-1.5">
              <span className="text-xs text-app-text-dim font-mono">
                <span className="text-app-text-muted"># </span>
                环境变量 ({envCount})
              </span>
            </div>
            <div className="border border-app-border bg-[var(--app-input)] max-h-[200px] overflow-y-auto">
              {Object.entries(data.env).slice(0, 20).map(([k, v]) => (
                <div key={k} className="flex items-center gap-2 px-2 py-1 border-b border-app-border-light last:border-0 text-2xs font-mono">
                  <span className="text-app-accent w-[180px] shrink-0 truncate">{k}</span>
                  <span className="text-app-text-muted">=</span>
                  <span className="text-app-text-dim truncate flex-1">{v ? (v.length > 40 ? v.slice(0, 40) + "..." : v) : <span className="italic text-app-text-muted">空</span>}</span>
                </div>
              ))}
              {envCount > 20 && (
                <div className="px-2 py-1 text-2xs text-app-text-muted font-mono text-center">
                  ... 还有 {envCount - 20} 个变量
                </div>
              )}
            </div>
          </div>
        </div>

        {/* Footer */}
        <div className="flex justify-end gap-2 px-4 py-3 border-t border-app-border bg-[var(--app-subtle)]">
          <Button variant="ghost" size="sm" onClick={onCancel}>取消</Button>
          <Button variant="primary" size="sm" onClick={handleImport} disabled={saving}>
            <Check size={13} />
            {saving ? "导入中..." : "确认导入"}
          </Button>
        </div>
      </div>
    </div>
  );
}
