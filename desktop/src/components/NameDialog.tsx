import React, { useState, useRef, useEffect } from "react";
import { X, Terminal } from "lucide-react";
import { Button } from "./common/Button";

interface NameDialogProps {
  open: boolean;
  title: string;
  initialName: string;
  confirmLabel?: string;
  onConfirm: (name: string) => Promise<void>;
  onCancel: () => void;
}

export function NameDialog({
  open, title, initialName, confirmLabel = "确定", onConfirm, onCancel,
}: NameDialogProps) {
  const [name, setName] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (open) {
      setName(initialName);
      setError("");
      setTimeout(() => inputRef.current?.focus(), 50);
    }
  }, [open, initialName]);

  if (!open) return null;

  const validate = () => {
    if (!name.trim().match(/^[a-z0-9]([a-z0-9-]*[a-z0-9])?$/)) {
      setError("名称只能包含小写字母、数字和连字符");
      return false;
    }
    setError("");
    return true;
  };

  const handleConfirm = async () => {
    if (!validate()) return;
    setSaving(true);
    try {
      await onConfirm(name.trim());
      onCancel();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm animate-[fadeIn_100ms_ease-out]"
      onClick={(e) => e.target === e.currentTarget && onCancel()}
    >
      <div
        onKeyDown={(e) => { if (e.key === "Escape") onCancel(); if (e.key === "Enter") handleConfirm(); }}
        className="bg-app-panel border border-app-border shadow-dialog w-[400px] animate-[scaleIn_150ms_ease-out]"
      >
        <div className="flex items-center justify-between px-4 py-3 border-b border-app-border">
          <div className="flex items-center gap-2">
            <Terminal size={14} className="text-app-accent" />
            <h3 className="font-semibold text-sm font-mono">{title}</h3>
          </div>
          <button onClick={onCancel} className="p-1 text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)] transition-colors">
            <X size={14} />
          </button>
        </div>

        <div className="p-4">
          <label className="block text-xs text-app-text-dim mb-1.5 font-mono">
            <span className="text-app-text-muted">$ </span>Profile 名称
          </label>
          <input
            ref={inputRef}
            value={name}
            onChange={(e) => { setName(e.target.value); setError(""); }}
            className="w-full h-[30px] text-sm font-mono"
            placeholder="profile-name"
            spellCheck={false}
          />
          {error && (
            <div className="mt-2 px-3 py-1.5 bg-app-red-bg border border-[var(--app-red-bg)] text-xs text-app-red font-mono">
              {error}
            </div>
          )}
        </div>

        <div className="flex justify-end gap-2 px-4 py-3 border-t border-app-border bg-[var(--app-subtle)]">
          <Button variant="ghost" size="sm" onClick={onCancel}>取消</Button>
          <Button variant="primary" size="sm" onClick={handleConfirm} disabled={saving}>
            {saving ? "处理中..." : confirmLabel}
          </Button>
        </div>
      </div>
    </div>
  );
}
