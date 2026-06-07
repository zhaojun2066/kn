import React, { useState, useRef, useEffect } from "react";
import {
  Plus, Star, Ellipsis, Copy, Download, FileDown, Trash2,
  RefreshCw, Save, History, ChevronDown, Search,
} from "lucide-react";
import { Button } from "./common/Button";

interface ExpandableToolbarProps {
  selectedName: string | null;
  isDefault: boolean;
  hasSelection: boolean;
  backupExists: boolean;
  onAdd: () => void;
  onSetDefault: (name: string) => void;
  onCopyProfile: () => void;
  onInit: () => void;
  onImport: () => void;
  onExport: () => void;
  onDelete: (name: string) => void;
  batchNames?: string[];
  onBatchDelete?: (names: string[]) => void;
  onBatchExport?: (names: string[]) => void;
  onRefresh: () => void;
  onBackup: () => void;
  onRestore: () => void;
}

/* ── Inline dropdown for Import ──────────────────────── */
function ImportDrop({ onScan, onFile }: { onScan: () => void; onFile: () => void }) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const h = (e: MouseEvent) => { if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false); };
    const k = (e: KeyboardEvent) => { if (e.key === "Escape") setOpen(false); };
    document.addEventListener("mousedown", h);
    document.addEventListener("keydown", k);
    return () => { document.removeEventListener("mousedown", h); document.removeEventListener("keydown", k); };
  }, [open]);

  return (
    <div ref={ref} className="relative shrink-0">
      <button
        onClick={() => setOpen(!open)}
        className={`flex items-center gap-0.5 p-0.5 transition-colors duration-fast
          ${open ? "text-app-accent bg-[var(--app-hover)]" : "text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)]"}`}
        title="导入"
      >
        <Download size={13} />
        <ChevronDown size={9} className={open ? "rotate-180" : ""} />
      </button>
      {open && (
        <div className="absolute top-full left-0 mt-1 z-50 min-w-[150px] bg-app-panel border border-app-border shadow-dialog py-0.5 whitespace-nowrap">
          <button
            onClick={() => { onScan(); setOpen(false); }}
            className="w-full flex items-center gap-2 px-3 py-1.5 text-sm font-mono text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)] transition-colors whitespace-nowrap"
          >
            <Search size={13} className="shrink-0" />
            <span className="flex-1 text-left">扫描系统配置</span>
            <span className="text-2xs text-app-text-muted shrink-0">Claude/Codex</span>
          </button>
          <button
            onClick={() => { onFile(); setOpen(false); }}
            className="w-full flex items-center gap-2 px-3 py-1.5 text-sm font-mono text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)] transition-colors whitespace-nowrap"
          >
            <Download size={13} className="shrink-0" />
            <span className="flex-1 text-left">从文件导入</span>
            <span className="text-2xs text-app-text-muted shrink-0">JSON</span>
          </button>
        </div>
      )}
    </div>
  );
}

/* ── ExpandableToolbar ───────────────────────────────── */
export function ExpandableToolbar({
  selectedName, isDefault, hasSelection, backupExists,
  onAdd, onSetDefault, onCopyProfile, onInit, onImport, onExport, onDelete,
  batchNames, onBatchDelete, onBatchExport,
  onRefresh, onBackup, onRestore,
}: ExpandableToolbarProps) {
  const [expanded, setExpanded] = useState(false);

  const batchCount = batchNames?.length || 0;
  const needsSelection = !hasSelection && batchCount === 0;
  const needsNonDefault = !hasSelection || isDefault;

  const handleDelete = () => {
    if (batchCount > 0 && onBatchDelete) {
      onBatchDelete(batchNames!);
    } else if (selectedName) {
      onDelete(selectedName);
    }
  };

  const handleExport = () => {
    if (batchCount > 0 && onBatchExport) {
      onBatchExport(batchNames!);
    } else {
      onExport();
    }
  };

  return (
    <div className="flex items-center gap-1 px-2 pt-1.5 pb-1">
      {/* ── Always visible ──────────────────────────── */}
      <Button variant="primary" size="sm" onClick={onAdd} title="新增 Profile">
        <Plus size={14} />
      </Button>

      <button
        disabled={needsNonDefault}
        onClick={() => selectedName && onSetDefault(selectedName)}
        className={`p-0.5 transition-colors duration-fast
          ${needsNonDefault
            ? "text-app-text-muted opacity-40 cursor-not-allowed"
            : "text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)]"
          }`}
        title="设为默认"
      >
        <Star size={13} />
      </button>

      {/* ── Divider ──────────────────────────────────── */}
      <div className={`h-5 border-l border-app-border mx-0.5 shrink-0 transition-opacity duration-200 ${expanded ? "opacity-100" : "opacity-0"}`} />

      {/* ── Expandable section (clipped) ─────────────── */}
      <div
        className="flex items-center gap-1 overflow-hidden transition-all duration-200 ease-out"
        style={{
          maxWidth: expanded ? "400px" : "0px",
          opacity: expanded ? 1 : 0,
        }}
      >
        <button
          disabled={needsSelection}
          onClick={onCopyProfile}
          className={`p-0.5 transition-colors duration-fast shrink-0
            ${needsSelection
              ? "text-app-text-muted opacity-40 cursor-not-allowed"
              : "text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)]"
            }`}
          title="复制"
        >
          <Copy size={13} />
        </button>

        <button
          disabled={needsSelection}
          onClick={handleExport}
          className={`p-0.5 transition-colors duration-fast shrink-0
            ${needsSelection
              ? "text-app-text-muted opacity-40 cursor-not-allowed"
              : "text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)]"
            }`}
          title={batchCount > 0 ? `导出已选 (${batchCount})` : "导出"}
        >
          <FileDown size={13} />
        </button>

        <button
          disabled={needsSelection}
          onClick={handleDelete}
          className={`p-0.5 transition-colors duration-fast shrink-0
            ${needsSelection
              ? "text-app-text-muted opacity-40 cursor-not-allowed"
              : "text-app-text-dim hover:text-app-red hover:bg-[var(--app-hover)]"
            }`}
          title={batchCount > 0 ? `删除已选 (${batchCount})` : "删除"}
        >
          <Trash2 size={13} />
        </button>

        {/* Divider */}
        <div className="h-5 border-l border-app-border mx-0.5 shrink-0" />

        {/* Config management */}
        <button
          onClick={onRefresh}
          className="p-0.5 text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)] transition-colors duration-fast shrink-0"
          title="刷新配置"
        >
          <RefreshCw size={13} />
        </button>

        <button
          onClick={onBackup}
          className="p-0.5 text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)] transition-colors duration-fast shrink-0"
          title="备份配置"
        >
          <Save size={13} />
        </button>

        <button
          onClick={onRestore}
          className={`p-0.5 transition-colors duration-fast shrink-0
            ${backupExists
              ? "text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)]"
              : "text-app-text-muted opacity-40 cursor-not-allowed"
            }`}
          title={backupExists ? "恢复配置" : "无可用备份"}
          disabled={!backupExists}
        >
          <History size={13} />
        </button>
      </div>

      {/* Import dropdown — outside overflow-hidden so dropdown isn't clipped */}
      {expanded && <ImportDrop onScan={onInit} onFile={onImport} />}

      {/* ── Ellipsis toggle ─────────────────────────── */}
      <button
        onClick={() => setExpanded(!expanded)}
        className={`shrink-0 p-0.5 transition-all duration-fast
          ${expanded
            ? "text-app-accent bg-[var(--app-hover)]"
            : "text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)]"
          }`}
        title={expanded ? "收起" : "更多操作"}
      >
        <Ellipsis size={13} />
      </button>
    </div>
  );
}
