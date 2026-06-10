import React from "react";
import { Layers, Blocks, Webhook, Folder, type LucideIcon } from "lucide-react";

export type ActivityKey = "profile" | "skills" | "hooks" | "projects";

interface ActivityItem {
  key: ActivityKey;
  icon: LucideIcon;
  label: string;
}

const ACTIVITIES: ActivityItem[] = [
  { key: "profile", icon: Layers, label: "Profiles" },
  { key: "skills", icon: Blocks, label: "Skills & Plugins" },
  { key: "hooks", icon: Webhook, label: "Hooks" },
  { key: "projects", icon: Folder, label: "Projects" },
];

interface ActivityBarProps {
  active: ActivityKey;
  onChange: (key: ActivityKey) => void;
}

/**
 * VS Code-style Activity Bar — narrow vertical icon strip on the far left.
 * Sits between the window edge and the sidebar/content area.
 *
 * Visual: darker than the sidebar, with a left-edge accent bar on the active icon.
 * Icons are small (16px) and monochrome, gaining color only when active.
 */
export function ActivityBar({ active, onChange }: ActivityBarProps) {
  return (
    <div className="w-12 shrink-0 flex flex-col items-center bg-[var(--app-statusbar)] border-r border-[var(--app-border)] select-none py-2">
      {ACTIVITIES.map((item, i) => {
        const isActive = active === item.key;
        const Icon = item.icon;
        return (
          <button
            key={item.key}
            onClick={() => onChange(item.key)}
            className={`
              group relative w-full flex justify-center py-2.5
              transition-colors duration-150 ease-out
              ${isActive
                ? "text-[var(--app-accent)]"
                : "text-[var(--app-text-muted)] hover:text-[var(--app-text-dim)]"
              }
            `}
          >
            {/* Left edge accent bar — visible when active */}
            {isActive && (
              <span
                className="absolute left-0 top-1/2 -translate-y-1/2 w-[3px] h-5 bg-[var(--app-accent)]"
                style={{ boxShadow: "0 0 6px var(--app-glow)" }}
              />
            )}
            <Icon size={18} strokeWidth={isActive ? 2 : 1.5} />

            {/* Tooltip — slides in from the left on hover */}
            <span
              className="absolute left-full ml-2 top-1/2 -translate-y-1/2 px-2 py-1
                text-2xs font-mono whitespace-nowrap
                bg-[var(--app-panel)] text-[var(--app-text)]
                border border-[var(--app-border)] shadow-panel
                opacity-0 group-hover:opacity-100
                pointer-events-none z-50
                transition-opacity duration-150 ease-out"
            >
              {item.label}
            </span>
          </button>
        );
      })}

      {/* Spacer pushes future items to bottom */}
      <div className="flex-1" />
    </div>
  );
}
