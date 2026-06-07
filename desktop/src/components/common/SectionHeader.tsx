import React from "react";
import { ChevronRight } from "lucide-react";

interface SectionHeaderProps {
  label: string;
  count: number;
  collapsed: boolean;
  onToggle: () => void;
}

export const SectionHeader = React.memo(function SectionHeader({ label, count, collapsed, onToggle }: SectionHeaderProps) {
  return (
    <div className="flex items-center gap-1 pl-2 pr-1 pt-2.5 pb-1" role="group">
      <button
        onClick={onToggle}
        aria-expanded={!collapsed}
        aria-label={`${collapsed ? "展开" : "折叠"} ${label}`}
        className="flex items-center gap-1.5 flex-1 min-w-0 text-left
          hover:bg-[var(--app-hover)] transition-colors duration-fast"
      >
        <ChevronRight
          size={10}
          className={`shrink-0 text-[var(--app-text-muted)] transition-transform duration-200
            ${collapsed ? "" : "rotate-90"}`}
        />
        <span className="text-2xs text-[var(--app-text-muted)] uppercase tracking-[0.2em] font-mono flex-1">
          {label}
        </span>
        <span className="text-2xs text-[var(--app-text-muted)] font-mono tabular-nums">
          {count}
        </span>
      </button>
    </div>
  );
});
