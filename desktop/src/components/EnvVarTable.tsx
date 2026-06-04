import React, { useState } from "react";
import { EnvVarRow } from "./EnvVarRow";
import { Button } from "./common/Button";
import { Plus, Check, X, Eye, EyeOff } from "lucide-react";

interface EnvVarTableProps {
  env: Record<string, string>;
  onSet: (key: string, value: string) => Promise<void>;
  onDelete: (key: string) => Promise<void>;
}

export function EnvVarTable({ env, onSet, onDelete }: EnvVarTableProps) {
  const [adding, setAdding] = useState(false);
  const [newKey, setNewKey] = useState("");
  const [newValue, setNewValue] = useState("");
  const [saving, setSaving] = useState(false);
  const [showAll, setShowAll] = useState(false);

  // System-managed keys — read-only in detail view
  const PROTECTED_KEYS: ReadonlySet<string> = new Set(["_KN_CLI_TYPE"]);

  // User vars first, protected keys at bottom
  const entries = Object.entries(env).sort(([a], [b]) => {
    const aProtected = PROTECTED_KEYS.has(a);
    const bProtected = PROTECTED_KEYS.has(b);
    if (aProtected && !bProtected) return 1;
    if (!aProtected && bProtected) return -1;
    return a.localeCompare(b);
  });

  const handleAdd = async () => {
    if (!newKey.trim()) return;
    setSaving(true);
    try {
      await onSet(newKey.trim(), newValue);
      setNewKey("");
      setNewValue("");
      setAdding(false);
    } catch { /* hook handles error */ }
    finally { setSaving(false); }
  };

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center gap-2 px-3 py-1.5 bg-[var(--app-subtle)] border-b border-app-border select-none">
        <span className="text-2xs text-app-text-muted uppercase tracking-[0.2em] w-[240px] shrink-0 font-mono">
          变量名
        </span>
        <span className="text-app-text-muted text-xs w-4 text-center font-mono">=</span>
        <span className="text-2xs text-app-text-muted uppercase tracking-[0.2em] flex-1 font-mono">
          值
        </span>
        <button
          onClick={() => setShowAll(!showAll)}
          className="text-2xs text-app-text-muted hover:text-app-text font-mono transition-colors mr-2"
          title={showAll ? "隐藏密钥" : "显示全部"}
        >
          {showAll ? <EyeOff size={11} /> : <Eye size={11} />}
        </button>
        <span className="w-[80px] shrink-0" />
      </div>

      {/* Rows */}
      <div className="flex-1 overflow-y-auto">
        {entries.length === 0 && !adding ? (
          <div className="flex flex-col items-center justify-center h-full gap-3 text-center py-8">
            <div className="w-12 h-12 flex items-center justify-center">
              <Plus size={22} className="text-app-text-muted opacity-30" />
            </div>
            <div>
              <div className="text-sm text-app-text-dim">
                <span className="text-app-text-muted"># </span>
                暂无环境变量
              </div>
              <div className="text-xs text-app-text-muted mt-1.5">
                添加键值对来配置此 profile
              </div>
            </div>
            <Button variant="primary" size="sm" onClick={() => setAdding(true)}>
              <Plus size={13} />
              添加变量
            </Button>
          </div>
        ) : (
          entries.map(([key, value]) => (
            <EnvVarRow
              key={key}
              envKey={key}
              value={value}
              onSave={async (k, v) => onSet(k, v)}
              onDelete={async (k) => onDelete(k)}
              showAll={showAll}
              readonly={PROTECTED_KEYS.has(key)}
            />
          ))
        )}

        {/* Inline add row */}
        {adding && (
          <div className="flex items-center gap-2 px-3 py-1.5 bg-[var(--app-selected)] border-b border-app-accent animate-[fadeIn_150ms_ease-out]">
            <input
              value={newKey}
              onChange={(e) => setNewKey(e.target.value)}
              className="w-[240px] font-mono text-xs h-[26px] bg-app-input"
              placeholder="KEY"
              disabled={saving}
              autoFocus
              onKeyDown={(e) => e.key === "Enter" && handleAdd()}
            />
            <span className="text-app-text-muted text-xs font-mono">=</span>
            <input
              value={newValue}
              onChange={(e) => setNewValue(e.target.value)}
              className="flex-1 font-mono text-xs h-[26px] bg-app-input"
              placeholder="value"
              disabled={saving}
              onKeyDown={(e) => e.key === "Enter" && handleAdd()}
            />
            <Button size="sm" variant="primary" onClick={handleAdd} disabled={saving}>
              <Check size={12} />
            </Button>
            <Button size="sm" variant="ghost" onClick={() => { setAdding(false); setNewKey(""); setNewValue(""); }} disabled={saving}>
              <X size={12} />
            </Button>
          </div>
        )}
      </div>

      {/* Bottom toolbar */}
      {entries.length > 0 && !adding && (
        <div className="px-2 py-1 bg-app-panel border-t border-app-border shrink-0">
          <button
            onClick={() => setAdding(true)}
            className="flex items-center gap-1.5 px-2 py-1 text-xs text-app-text-dim font-mono
              hover:text-app-accent hover:bg-[var(--app-hover)] transition-colors duration-fast"
          >
            <Plus size={12} />
            <span>添加变量</span>
          </button>
        </div>
      )}
    </div>
  );
}
