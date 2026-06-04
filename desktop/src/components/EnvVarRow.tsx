import React, { useState } from "react";
import { Eye, EyeOff, Pencil, Trash2, Check, X, AlertTriangle, Lock } from "lucide-react";
import { Button } from "./common/Button";

const SECRET_TOKENS = new Set([
  "KEY", "TOKEN", "SECRET", "PASSWORD", "AUTH_TOKEN", "API_KEY",
]);

function isSecretKey(name: string): boolean {
  const tokens = new Set(name.toUpperCase().replace(/-/g, "_").split("_"));
  for (const t of tokens) {
    if (SECRET_TOKENS.has(t)) return true;
  }
  return false;
}

function maskValue(value: string): string {
  if (value.length >= 8) return value.slice(0, 4) + "****" + value.slice(-4);
  return "****";
}

interface EnvVarRowProps {
  envKey: string;
  value: string;
  onSave: (key: string, value: string) => Promise<void>;
  onDelete: (key: string) => Promise<void>;
  showAll?: boolean;
  readonly?: boolean;
}

export function EnvVarRow({ envKey, value, onSave, onDelete, showAll, readonly }: EnvVarRowProps) {
  const [visible, setVisible] = useState(false);
  const [editing, setEditing] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [editKey, setEditKey] = useState(envKey);
  const [editValue, setEditValue] = useState(value);
  const [saving, setSaving] = useState(false);
  const [deleting, setDeleting] = useState(false);

  const secret = isSecretKey(envKey);
  const displayValue = (secret && !visible && !showAll) ? maskValue(value) : value;

  const handleSave = async () => {
    if (!editKey.trim()) return;
    setSaving(true);
    try {
      // Save new key first, then delete old — prevents data loss if save fails
      await onSave(editKey.trim(), editValue);
      if (editKey !== envKey) await onDelete(envKey);
      setEditing(false);
    } catch { /* hook handles error */ }
    finally { setSaving(false); }
  };

  const handleDelete = async () => {
    setDeleting(true);
    try { await onDelete(envKey); }
    catch { /* hook handles error */ }
    finally {
      setDeleting(false);
      setConfirming(false);
    }
  };

  if (readonly && (editing || confirming)) {
    setEditing(false); setConfirming(false);
  }

  if (confirming && !readonly) {
    return (
      <div className="flex items-center gap-2 px-3 py-1.5 bg-app-red-bg border-b border-[var(--app-red-bg)] animate-[fadeIn_100ms_ease-out]">
        <AlertTriangle size={13} className="text-app-red shrink-0" />
        <span className="flex-1 text-xs text-app-red font-mono truncate">
          删除 <span className="text-app-text font-medium">{envKey}</span>？
        </span>
        <Button size="sm" variant="danger" onClick={handleDelete} disabled={deleting}>
          <Check size={12} />
          确认
        </Button>
        <Button size="sm" variant="ghost" onClick={() => setConfirming(false)} disabled={deleting}>
          <X size={12} />
          取消
        </Button>
      </div>
    );
  }

  if (editing) {
    return (
      <div className="flex items-center gap-2 px-3 py-1.5 bg-[var(--app-selected)] border-b border-app-accent animate-[fadeIn_150ms_ease-out]">
        <input
          value={editKey}
          onChange={(e) => setEditKey(e.target.value)}
          className="w-[240px] font-mono text-xs h-[26px] bg-app-input"
          placeholder="KEY"
          disabled={saving}
          autoFocus
        />
        <span className="text-app-text-muted text-xs font-mono">=</span>
        <input
          value={editValue}
          onChange={(e) => setEditValue(e.target.value)}
          className="flex-1 font-mono text-xs h-[26px] bg-app-input"
          placeholder="value"
          disabled={saving}
        />
        <Button size="sm" variant="primary" onClick={handleSave} disabled={saving}>
          <Check size={12} />
        </Button>
        <Button size="sm" variant="ghost" onClick={() => { setEditKey(envKey); setEditValue(value); setEditing(false); }} disabled={saving}>
          <X size={12} />
        </Button>
      </div>
    );
  }

  return (
    <div className="flex items-center gap-2 px-3 py-1.5 border-b border-app-border-light group hover:bg-app-hover transition-colors duration-fast">
      {/* Key */}
      <span className="font-mono text-xs text-app-accent w-[240px] shrink-0 truncate select-all">
        {envKey}
      </span>

      <span className="text-app-text-muted text-xs shrink-0 font-mono">=</span>

      {/* Value */}
      <span className={`flex-1 font-mono text-xs truncate select-all min-w-0 ${secret && !visible ? "text-app-text-muted" : ""}`}>
        {displayValue}
      </span>

      {/* Actions */}
      <span className="flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity duration-fast shrink-0">
        {readonly ? (
          <span title="系统变量，不可修改"><Lock size={11} className="text-app-text-muted" /></span>
        ) : (
          <>
            {secret && (
              <button
                onClick={() => setVisible(!visible)}
                className="p-1 text-app-text-dim hover:text-app-accent hover:bg-[var(--app-hover)] transition-colors"
                title={visible ? "隐藏" : "显示"}
              >
                {visible ? <EyeOff size={12} /> : <Eye size={12} />}
              </button>
            )}
            <button
              onClick={() => { setEditKey(envKey); setEditValue(value); setEditing(true); }}
              className="p-1 text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)] transition-colors"
              title="编辑"
            >
              <Pencil size={12} />
            </button>
            <button
              onClick={() => setConfirming(true)}
              className="p-1 text-app-text-dim hover:text-app-red hover:bg-app-red-bg transition-colors"
              title="删除"
            >
              <Trash2 size={12} />
            </button>
          </>
        )}
      </span>
    </div>
  );
}
