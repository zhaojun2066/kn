import React, { useState, useEffect } from "react";
import { X, Check, Search, CheckSquare, Square } from "lucide-react";
import { Button } from "./common/Button";
import { CLIIcon } from "./common/CLIIcon";
import { shortenPath } from "../lib/path-utils";

export interface ScanProfile {
  name: string;
  cli_type: string;
  env: Record<string, string>;
  source: string;
}

interface Props {
  open: boolean;
  profiles: ScanProfile[];
  onImport: (profiles: ScanProfile[]) => Promise<void>;
  onCancel: () => void;
}

export function ScanPreview({ open, profiles, onImport, onCancel }: Props) {
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [saving, setSaving] = useState(false);
  const [names, setNames] = useState<string[]>([]);
  const [edited, setEdited] = useState<Set<number>>(new Set());

  useEffect(() => {
    if (open && profiles.length > 0) {
      setSelected(new Set(profiles.map((_, i) => i)));
      setNames(profiles.map((p) => p.name));
      setEdited(new Set());
    }
  }, [open, profiles]);

  if (!open) return null;

  const toggle = (i: number) => {
    setSelected((prev) => {
      const next = new Set(prev);
      next.has(i) ? next.delete(i) : next.add(i);
      return next;
    });
  };

  const toggleAll = () => {
    if (selected.size === profiles.length) { setSelected(new Set()); }
    else { setSelected(new Set(profiles.map((_, i) => i))); }
  };

  const updateName = (i: number, val: string) => {
    setNames((prev) => prev.map((n, idx) => (idx === i ? val : n)));
    setEdited((prev) => new Set(prev).add(i));
  };

  const handleImport = async () => {
    const items = profiles
      .map((p, i) => ({ ...p, name: names[i]?.trim() || p.name }))
      .filter((_, i) => selected.has(i));
    if (items.length === 0) return;
    setSaving(true);
    try { await onImport(items); } finally { setSaving(false); }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm animate-[fadeIn_100ms_ease-out]"
      onClick={(e) => e.target === e.currentTarget && onCancel()}
    >
      <div className="bg-app-panel border border-app-border shadow-dialog w-[560px] animate-[scaleIn_150ms_ease-out]">
        <div className="flex items-center justify-between px-4 py-3 border-b border-app-border">
          <div className="flex items-center gap-2">
            <Search size={15} className="text-app-accent" />
            <h3 className="font-semibold text-sm font-mono">扫描结果 ({profiles.length})</h3>
          </div>
          <button onClick={onCancel} className="p-1 text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)] transition-colors">
            <X size={14} />
          </button>
        </div>

        <div className="p-3 max-h-[400px] overflow-y-auto space-y-1">
          {profiles.length === 0 ? (
            <div className="text-center py-8 text-sm text-app-text-muted font-mono">未找到任何配置</div>
          ) : (
            profiles.map((p, i) => (
              <div
                key={i}
                onClick={() => toggle(i)}
                className={`flex items-start gap-3 px-3 py-2.5 border cursor-pointer transition-colors
                  ${selected.has(i) ? "border-app-accent bg-[var(--app-selected)]" : "border-app-border hover:bg-[var(--app-hover)]"}`}
              >
                <span className="mt-1 shrink-0" onClick={(e) => { e.stopPropagation(); toggle(i); }}>
                  {selected.has(i) ? <CheckSquare size={16} className="text-app-accent" /> : <Square size={16} className="text-app-text-muted" />}
                </span>

                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2 mb-1">
                    <CLIIcon type={p.cli_type} size={18} />
                    <input
                      value={names[i] || ""}
                      onChange={(e) => updateName(i, e.target.value)}
                      onClick={(e) => e.stopPropagation()}
                      className={`h-[24px] text-sm font-mono font-medium bg-[var(--app-input)] border px-1.5 flex-1 min-w-0
                        ${edited.has(i) ? "border-app-accent text-app-text" : "border-transparent text-app-text"}`}
                      spellCheck={false}
                    />
                    <span className={`text-2xs px-1 py-px font-mono shrink-0 ${
                      p.cli_type === "claude" ? "text-app-accent bg-app-green-bg" :
                      p.cli_type === "codex" ? "text-app-blue bg-[var(--app-selected)]" :
                      p.cli_type === "gemini" ? "text-[#4285F4] bg-[#e8f0fe] bg-opacity-20" :
                      p.cli_type === "qoderclicn" ? "text-[#FF6A00] bg-[#fff3e6] bg-opacity-20" :
                      "text-app-text-dim bg-[var(--app-input)]"
                    }`}>
                      {p.cli_type === "claude" ? "Claude Code" :
                       p.cli_type === "codex" ? "Codex CLI" :
                       p.cli_type === "gemini" ? "Gemini CLI" :
                       p.cli_type === "qoderclicn" ? "Qoder CLI (国内版)" :
                       p.cli_type}
                    </span>
                  </div>
                  <div className="text-2xs text-app-text-muted font-mono truncate">
                    {Object.keys(p.env).length} 个变量 · {shortenPath(p.source)}
                  </div>
                  <div className="mt-1 space-y-0.5">
                    {Object.entries(p.env).slice(0, 3).map(([k, v]) => (
                      <div key={k} className="text-2xs font-mono flex gap-1">
                        <span className="text-app-accent shrink-0">{k}</span>
                        <span className="text-app-text-muted">=</span>
                        <span className="text-app-text-dim truncate">
                          {v ? (v.length > 30 ? v.slice(0, 30) + "..." : v) : <span className="italic">空</span>}
                        </span>
                      </div>
                    ))}
                  </div>
                </div>
              </div>
            ))
          )}
        </div>

        <div className="flex justify-between items-center px-4 py-3 border-t border-app-border bg-[var(--app-subtle)]">
          <button onClick={toggleAll} className="text-2xs text-app-text-dim hover:text-app-text font-mono transition-colors">
            {selected.size === profiles.length ? "取消全选" : "全选"}
          </button>
          <div className="flex gap-2">
            <Button variant="ghost" size="sm" onClick={onCancel}>取消</Button>
            <Button variant="primary" size="sm" onClick={handleImport} disabled={saving || selected.size === 0}>
              <Check size={13} />
              {saving ? "导入中..." : `导入选中 (${selected.size})`}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}
