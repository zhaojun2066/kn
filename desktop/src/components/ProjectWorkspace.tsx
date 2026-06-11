import React, { useState } from "react";
import type { ProjectInfo, SessionInfo } from "../lib/types";
import { ProjectOverview } from "./ProjectOverview";
import type { LocalCliUsageRow } from "./LocalCliUsage";

type ProjectTab = "overview" | "sessions" | "skills" | "agents" | "hooks" | "files" | "usage";

const TABS: { key: ProjectTab; label: string }[] = [
  { key: "overview", label: "Overview" },
  { key: "sessions", label: "Sessions" },
  { key: "skills", label: "Project Skills" },
  { key: "agents", label: "Project Agents" },
  { key: "hooks", label: "Project Hooks" },
  { key: "files", label: "Files" },
  { key: "usage", label: "Usage" },
];

interface ProjectWorkspaceProps {
  project: ProjectInfo;
  sessions: SessionInfo[];
  cliUsageRows: LocalCliUsageRow[];
  onRunDefault: () => void;
  onChangeDefaultProfile: () => void;
}

export function ProjectWorkspace({
  project,
  sessions,
  cliUsageRows,
  onRunDefault,
  onChangeDefaultProfile,
}: ProjectWorkspaceProps) {
  const [activeTab, setActiveTab] = useState<ProjectTab>("overview");

  return (
    <div className="flex-1 flex flex-col min-h-0 bg-[var(--app-bg)]">
      <div className="h-[42px] shrink-0 flex items-center gap-2 px-3 border-b border-app-border">
        <div className="text-sm font-mono text-app-text truncate">{project.name}</div>
        <div className="text-2xs font-mono text-app-text-muted truncate">{project.path}</div>
      </div>
      <div className="flex shrink-0 border-b border-app-border px-2 overflow-x-auto">
        {TABS.map((tab) => (
          <button
            key={tab.key}
            onClick={() => setActiveTab(tab.key)}
            className={`px-3 py-2 text-2xs font-mono border-b whitespace-nowrap ${
              activeTab === tab.key
                ? "text-app-accent border-app-accent"
                : "text-app-text-muted border-transparent hover:text-app-text"
            }`}
          >
            {tab.label}
          </button>
        ))}
      </div>
      {activeTab === "overview" && (
        <ProjectOverview
          project={project}
          sessions={sessions}
          cliUsageRows={cliUsageRows}
          onRunDefault={onRunDefault}
          onChangeDefaultProfile={onChangeDefaultProfile}
        />
      )}
      {activeTab !== "overview" && (
        <div className="flex-1 flex items-center justify-center text-xs font-mono text-app-text-muted">
          {activeTab === "sessions" && "当前项目 Sessions"}
          {activeTab === "skills" && "当前项目 Skills"}
          {activeTab === "agents" && "当前项目 Agents"}
          {activeTab === "hooks" && "当前项目 Hooks"}
          {activeTab === "files" && "当前项目 Files"}
          {activeTab === "usage" && "当前项目 Usage"}
        </div>
      )}
    </div>
  );
}
