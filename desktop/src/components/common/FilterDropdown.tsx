import React, { useState, useRef, useEffect } from "react";
import { ChevronDown, Filter } from "lucide-react";

export interface FilterOption {
  label: string;
  value: string;
}

interface FilterDropdownProps {
  value: string;
  options: readonly FilterOption[] | FilterOption[];
  onChange: (value: string) => void;
  allLabel?: string;
  /** Show border + bg styling (matches HookList's original style) */
  bordered?: boolean;
}

export const FilterDropdown = React.memo(function FilterDropdown({ value, options, onChange, allLabel = "全部", bordered }: FilterDropdownProps) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handle = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    if (open) {
      document.addEventListener("mousedown", handle);
      return () => document.removeEventListener("mousedown", handle);
    }
  }, [open]);

  const activeOption = (options as FilterOption[]).find((o) => o.value === value);
  const activeLabel = activeOption ? activeOption.label : allLabel;

  return (
    <div ref={ref} className="relative flex-1 min-w-0">
      <button
        onClick={() => setOpen((v) => !v)}
        className={`flex items-center gap-1 w-full px-2 py-1 text-2xs font-mono
          text-[var(--app-text-dim)] hover:text-[var(--app-text)] transition-colors
          ${bordered ? "border border-[var(--app-border)] bg-[var(--app-input)]" : ""}`}
      >
        {bordered && <Filter size={9} className="shrink-0" />}
        <span className="flex-1 text-left truncate">{activeLabel}</span>
        {value !== "all" && (
          <span
            onClick={(e) => { e.stopPropagation(); onChange("all"); }}
            className="text-[var(--app-text-muted)] hover:text-[var(--app-red)] shrink-0"
          >&#10005;</span>
        )}
        <ChevronDown size={9} className={`shrink-0 transition-transform ${open ? "rotate-180" : ""}`} />
      </button>
      {open && (
        <div className="absolute top-full left-0 right-0 z-50 bg-[var(--app-panel)] border border-[var(--app-border)] shadow-dialog py-0.5 max-h-[200px] overflow-y-auto">
          {(options as FilterOption[]).map((opt) => (
            <button
              key={opt.value}
              onClick={() => { onChange(opt.value); setOpen(false); }}
              className={`w-full text-left px-3 py-1 text-2xs font-mono transition-colors ${
                value === opt.value
                  ? "bg-[var(--app-accent)] text-[var(--app-bg)]"
                  : "text-[var(--app-text-dim)] hover:bg-[var(--app-hover)]"
              }`}
            >
              {opt.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
});
