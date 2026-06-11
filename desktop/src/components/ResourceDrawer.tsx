import React, { useState } from "react";

type ResourceTab = "plugins" | "skills" | "agents" | "hooks" | "commands";

const TABS: { key: ResourceTab; label: string }[] = [
  { key: "plugins", label: "Plugins" },
  { key: "skills", label: "Skills" },
  { key: "agents", label: "Agents" },
  { key: "hooks", label: "Hooks" },
  { key: "commands", label: "Commands" },
];

interface ResourceDrawerProps {
  open: boolean;
  onClose: () => void;
}

export function ResourceDrawer({ open, onClose }: ResourceDrawerProps) {
  const [active, setActive] = useState<ResourceTab>("plugins");
  if (!open) return null;

  return (
    <div className="fixed inset-0 z-40 flex justify-end">
      <button
        aria-label="关闭资源管理遮罩"
        className="absolute inset-0 bg-black/45"
        onClick={onClose}
      />
      <section className="relative z-10 w-[min(1080px,88vw)] h-full bg-app-bg border-l border-app-border shadow-dialog flex flex-col">
        <div className="h-[44px] shrink-0 flex items-center gap-3 px-4 border-b border-app-border bg-app-toolbar">
          <div className="text-sm font-mono text-app-text font-semibold">Resource Management</div>
          <div className="text-2xs font-mono text-app-text-muted">全局资源库</div>
          <button aria-label="关闭资源管理" onClick={onClose} className="ml-auto text-app-text-muted hover:text-app-text">
            x
          </button>
        </div>
        <div className="flex-1 min-h-0 grid grid-cols-[280px_1fr]">
          <aside className="border-r border-app-border bg-app-sidebar p-2">
            {TABS.map((tab) => (
              <button
                key={tab.key}
                onClick={() => setActive(tab.key)}
                className={`w-full text-left px-3 py-2 text-xs font-mono border-l-[3px] ${
                  active === tab.key
                    ? "border-l-app-accent bg-app-selected text-app-text"
                    : "border-l-transparent text-app-text-dim hover:bg-app-hover"
                }`}
              >
                {tab.label}
              </button>
            ))}
          </aside>
          <main className="min-h-0 overflow-y-auto p-4">
            <div className="text-xs font-mono text-app-text-muted">
              {TABS.find((tab) => tab.key === active)?.label} 全局管理内容将在 Task 10 接入现有组件。
            </div>
          </main>
        </div>
      </section>
    </div>
  );
}
