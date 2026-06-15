/// <reference types="vitest" />
import { fireEvent, render, screen } from "@testing-library/react";
import React from "react";
import { describe, expect, it, vi } from "vitest";

import { ProjectSidebar } from "../ProjectSidebar";
import type { ProfileSummary, ProjectInfo } from "../../lib/types";

const project: ProjectInfo = {
  name: "kn",
  path: "/repo/kn",
  defaultProfile: "project-profile",
  pinned: false,
};

const profiles: ProfileSummary[] = [
  {
    name: "global-default",
    desc: "",
    env_count: 1,
    is_default: true,
    cli_type: "claude",
  },
  {
    name: "project-profile",
    desc: "",
    env_count: 1,
    is_default: false,
    cli_type: "codex",
  },
];

describe("ProjectSidebar", () => {
  it("focuses the project .ai-profile binding when opening the run profile picker", () => {
    const onRunProfile = vi.fn();

    render(
      <ProjectSidebar
        projects={[project]}
        selectedProject={project}
        onSelect={vi.fn()}
        onAddProject={vi.fn()}
        onDeleteProject={vi.fn()}
        onRunProfile={onRunProfile}
        onSetDescription={vi.fn()}
        onTogglePin={vi.fn()}
        onOpenInEditor={vi.fn()}
        profiles={profiles}
        statsMap={{}}
      />,
    );

    fireEvent.click(screen.getByTitle("运行"));

    expect(screen.getByText("项目默认")).not.toBeNull();

    fireEvent.keyDown(screen.getByText("运行 kn").closest(".outline-none")!, {
      key: "Enter",
    });

    expect(onRunProfile).toHaveBeenCalledWith(
      "/repo/kn",
      "kn",
      "project-profile",
      "codex",
    );
  });
});
