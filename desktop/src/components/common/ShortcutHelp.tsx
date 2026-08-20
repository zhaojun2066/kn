import React from "react";
import type { ShortcutGroup } from "../../lib/shortcut-definitions";

interface ShortcutHelpProps {
  groups: ShortcutGroup[];
  dense?: boolean;
}

export function ShortcutHelp({ groups, dense = false }: ShortcutHelpProps) {
  return (
    <div className={dense ? "space-y-3" : "grid grid-cols-1 lg:grid-cols-2 gap-3"}>
      {groups.map((group) => (
        <section
          key={group.id}
          className="border border-app-border bg-app-panel rounded-lg overflow-hidden"
        >
          <div className="px-3 py-2 border-b border-app-border bg-[var(--app-subtle)]">
            <div className="text-xs font-semibold text-app-text">{group.title}</div>
            <div className="text-2xs text-app-text-muted mt-0.5">{group.description}</div>
          </div>
          <div className="divide-y divide-[var(--app-border-light)]">
            {group.items.map((item) => (
              <div
                key={`${group.id}-${item.desc}`}
                className="px-3 py-2 flex items-center justify-between gap-3 hover:bg-[var(--app-hover)] transition-colors"
              >
                <div className="min-w-0">
                  <div className="text-xs text-app-text-dim">{item.desc}</div>
                  {item.note && <div className="text-2xs text-app-text-muted mt-0.5">{item.note}</div>}
                </div>
                <div className="flex items-center gap-1 shrink-0">
                  {item.keys.map((key, index) => (
                    <React.Fragment key={`${key}-${index}`}>
                      <kbd className="px-1.5 py-0.5 min-w-[20px] text-center text-2xs bg-app-input border border-app-border rounded font-mono text-app-text shadow-sm">
                        {key}
                      </kbd>
                      {index < item.keys.length - 1 && <span className="text-app-text-muted text-2xs">+</span>}
                    </React.Fragment>
                  ))}
                </div>
              </div>
            ))}
          </div>
        </section>
      ))}
    </div>
  );
}
