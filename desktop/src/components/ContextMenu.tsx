import React, { useEffect, useRef } from "react";
import { Copy, Pencil, Trash2, Star, ExternalLink } from "lucide-react";

export interface ContextMenuItem {
  label: string;
  icon?: React.ReactNode;
  onClick: () => void;
  danger?: boolean;
  disabled?: boolean;
  hint?: string;
}

interface ContextMenuProps {
  items: ContextMenuItem[];
  x: number;
  y: number;
  onClose: () => void;
}

export function ContextMenu({ items, x, y, onClose }: ContextMenuProps) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    };
    const keyHandler = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    document.addEventListener("mousedown", handler);
    document.addEventListener("keydown", keyHandler);
    return () => {
      document.removeEventListener("mousedown", handler);
      document.removeEventListener("keydown", keyHandler);
    };
  }, [onClose]);

  return (
    <div className="fixed inset-0 z-50" onClick={onClose}>
      <div
        ref={ref}
        className="absolute bg-app-panel border border-app-border shadow-dialog min-w-[180px] py-0.5"
        style={{ left: Math.min(x, window.innerWidth - 200), top: Math.min(y, window.innerHeight - items.length * 32 - 20) }}
      >
        {items.map((item, i) => (
          <button
            key={i}
            onClick={() => { item.onClick(); onClose(); }}
            disabled={item.disabled}
            className={`w-full flex items-center gap-2 px-3 py-1.5 text-sm font-mono transition-colors duration-fast
              ${item.disabled
                ? "text-app-text-muted cursor-not-allowed opacity-40"
                : item.danger
                  ? "text-app-red hover:bg-app-red-bg"
                  : "text-app-text-dim hover:text-app-text hover:bg-[var(--app-hover)]"
              }`}
          >
            {item.icon && <span className="shrink-0">{item.icon}</span>}
            <span className="flex-1 text-left">{item.label}</span>
            {item.hint && <span className="text-2xs text-app-text-muted">{item.hint}</span>}
          </button>
        ))}
      </div>
    </div>
  );
}
