import React from "react";
import type { ScopeTab } from "../lib/types";

interface ScopeTabsProps {
  active: ScopeTab;
  onChange: (scope: ScopeTab) => void;
  userCount: number;
  projectCount: number;
}

export const ScopeTabs = React.memo(function ScopeTabs({
  active,
  onChange,
  userCount,
  projectCount,
}: ScopeTabsProps) {
  const allCount = userCount + projectCount;

  const tabs: { key: ScopeTab; label: string; count: number }[] = [
    { key: "all", label: "全部", count: allCount },
    { key: "user", label: "用户级", count: userCount },
    { key: "project", label: "项目级", count: projectCount },
  ];

  return (
    <div className="flex gap-0 bg-[var(--app-input)] border border-[var(--app-border)] p-0.5">
      {tabs.map((tab) => (
        <button
          key={tab.key}
          onClick={() => onChange(tab.key)}
          className={`flex-1 py-1 text-2xs font-mono tracking-[0.05em] text-center
            transition-all duration-100 ease-out cursor-pointer border-none outline-none
            ${
              active === tab.key
                ? "text-[var(--app-accent)] bg-[var(--app-selected)]"
                : "text-[var(--app-text-muted)] hover:text-[var(--app-text-dim)] hover:bg-[var(--app-hover)]"
            }`}
          style={
            active === tab.key
              ? { boxShadow: "inset 0 0 6px var(--app-glow)" }
              : undefined
          }
        >
          {tab.label}
          <span className="ml-1 opacity-60 text-3xs">{tab.count}</span>
        </button>
      ))}
    </div>
  );
});
