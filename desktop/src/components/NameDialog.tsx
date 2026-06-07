import React, { useState, useRef, useEffect } from "react";
import { X, Terminal } from "lucide-react";
import { Button } from "./common/Button";
import { Dialog } from "./common/Dialog";

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

  const validate = () => {
    const trimmed = name.trim();
    if (!trimmed.match(/^[a-z0-9]([a-z0-9-]*[a-z0-9])?$/)) {
      setError("名称只能包含小写字母、数字和连字符");
      return false;
    }
    const reserved = ["claude", "codex", "qoderclicn", "profile", "ai", "help"];
    if (reserved.includes(trimmed)) {
      setError(`"${trimmed}" 是系统保留关键字，不能用作 Profile 名称`);
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
    <Dialog open={open} onClose={onCancel} width="400px">
      <div className="flex items-center justify-between px-4 py-3 border-b border-app-border">
        <div className="flex items-center gap-2">
          <Terminal size={14} className="text-app-accent" aria-hidden="true" />
          <h3 className="font-semibold text-sm font-mono">{title}</h3>
        </div>
        <button
          onClick={onCancel}
          aria-label="关闭"
          className="p-1 text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)] transition-colors"
        >
          <X size={14} aria-hidden="true" />
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
          onKeyDown={(e) => { if (e.key === "Enter") handleConfirm(); }}
          className="w-full h-[30px] text-sm font-mono px-2 bg-[var(--app-input)] border border-[var(--app-border)] text-[var(--app-text)]"
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
    </Dialog>
  );
}
