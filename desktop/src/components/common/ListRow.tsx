import React from "react";
import { CheckSquare, Square, Circle, Lock } from "lucide-react";

export interface ListRowProps {
  icon?: React.ReactNode;
  label: string;
  secondary?: string;
  badge?: React.ReactNode;
  enabled: boolean;
  selected: boolean;
  checked: boolean;
  showCheck: boolean;
  noCheck?: boolean;
  indent?: boolean;
  readonly?: boolean;
  dense?: boolean;
  onClick: (e: React.MouseEvent) => void;
  onContextMenu?: (e: React.MouseEvent) => void;
}

export const ListRow = React.memo(function ListRow({
  icon,
  label,
  secondary,
  badge,
  enabled,
  selected,
  checked,
  showCheck,
  noCheck,
  indent,
  readonly,
  dense,
  onClick,
  onContextMenu,
}: ListRowProps) {
  const height = dense ? "h-8" : "h-10";

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      onClick(e as unknown as React.MouseEvent);
    }
  };

  return (
    <div
      role="option"
      aria-selected={selected}
      tabIndex={0}
      onClick={onClick}
      onContextMenu={onContextMenu}
      onKeyDown={handleKeyDown}
      className={`flex items-center gap-2 ${height} cursor-pointer select-none
        transition-all duration-100 ease-out group outline-none
        focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-[var(--app-focus)]
        ${selected
          ? "bg-[var(--app-selected)] border-l-[3px] border-l-[var(--app-accent)] text-[var(--app-text)]"
          : checked
            ? "bg-[var(--app-hover)] border-l-[3px] border-l-[var(--app-amber)] text-[var(--app-text)]"
            : "border-l-[3px] border-l-transparent text-[var(--app-text-dim)] hover:bg-[var(--app-hover)] hover:text-[var(--app-text)]"
        }
        ${indent ? "pl-10" : "pl-3"}
        pr-2`}
      style={selected ? { boxShadow: "inset 0 0 8px var(--app-glow)" } : undefined}
    >
      {!noCheck && (
        <span
          data-checkbox
          aria-hidden="true"
          className={`shrink-0 cursor-pointer transition-opacity duration-fast
            ${checked || showCheck ? "opacity-100" : "opacity-0 group-hover:opacity-100"}`}
        >
          {checked
            ? <CheckSquare size={12} className="text-[var(--app-amber)]" />
            : <Square size={12} className="text-[var(--app-text-muted)]" />
          }
        </span>
      )}

      {icon ?? (
        readonly
          ? <Lock size={10} className="text-[var(--app-text-muted)] shrink-0" aria-hidden="true" />
          : <Circle
              size={5}
              aria-hidden="true"
              className={`shrink-0 ${
                enabled
                  ? "fill-[var(--app-accent)] text-[var(--app-accent)]"
                  : "fill-[var(--app-text-muted)] text-[var(--app-text-muted)] opacity-50"
              }`}
              style={enabled ? { boxShadow: "0 0 4px var(--app-glow)" } : undefined}
            />
      )}

      <div className="flex-1 min-w-0">
        <div className={`text-xs font-mono truncate ${!enabled && !readonly ? "opacity-60 line-through" : ""}`}>
          {label}
        </div>
        {secondary && (
          <div className="text-2xs text-[var(--app-text-muted)] font-mono truncate mt-0.5">
            {secondary}
          </div>
        )}
      </div>

      {badge}
    </div>
  );
});
