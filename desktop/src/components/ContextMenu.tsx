import { useEffect, useRef, useCallback } from "react";

export interface ContextMenuItem {
  label: string;
  icon?: React.ReactNode;
  onClick: () => void;
  danger?: boolean;
  disabled?: boolean;
}

export interface SeparatorItem {
  separator: true;
}

export type MenuItem = ContextMenuItem | SeparatorItem;

interface ContextMenuProps {
  x: number;
  y: number;
  items: MenuItem[];
  onClose: () => void;
}

export function ContextMenu({ x, y, items, onClose }: ContextMenuProps) {
  const ref = useRef<HTMLDivElement>(null);

  const close = useCallback(() => {
    onClose();
  }, [onClose]);

  useEffect(() => {
    const h = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        close();
      }
    };
    // Delay listener to avoid closing immediately from the same right-click
    const t = setTimeout(() => {
      document.addEventListener("mousedown", h);
      document.addEventListener("contextmenu", h);
    }, 0);
    return () => {
      clearTimeout(t);
      document.removeEventListener("mousedown", h);
      document.removeEventListener("contextmenu", h);
    };
  }, [close]);

  return (
    <div
      ref={ref}
      style={{
        position: "fixed",
        left: `${x}px`,
        top: `${y}px`,
        zIndex: 1000,
      }}
      className="bg-[var(--app-panel)] border border-[var(--app-border)] shadow-dialog py-1 min-w-[140px] animate-[fadeIn_100ms_ease-out]"
    >
      {items.map((item, i) => {
        if ("separator" in item && item.separator) {
          return (
            <div
              key={`sep-${i}`}
              className="my-1 border-t border-[var(--app-border-light)]"
            />
          );
        }
        const mi = item as ContextMenuItem;
        return (
          <button
            key={`${mi.label}-${i}`}
            onClick={() => {
              if (!mi.disabled) {
                mi.onClick();
                close();
              }
            }}
            disabled={mi.disabled}
            className={`w-full text-left px-3 py-1.5 text-xs font-mono flex items-center gap-2 transition-colors
              ${mi.disabled
                ? "text-[var(--app-text-muted)] opacity-40 cursor-not-allowed"
                : mi.danger
                  ? "text-[var(--app-red)] hover:bg-[var(--app-red-bg)]"
                  : "text-[var(--app-text-dim)] hover:bg-[var(--app-hover)] hover:text-[var(--app-text)]"
              }`}
          >
            {mi.icon && (
              <span className="shrink-0 w-4 flex items-center justify-center">
                {mi.icon}
              </span>
            )}
            <span>{mi.label}</span>
          </button>
        );
      })}
    </div>
  );
}
