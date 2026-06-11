/// <reference types="vitest" />
import { fireEvent, render, screen } from "@testing-library/react";
import React from "react";
import { describe, expect, it, vi } from "vitest";
import { ProjectWorkspace } from "../ProjectWorkspace";
import type { ProjectInfo } from "../../lib/types";

const project: ProjectInfo = {
  name: "kn",
  path: "/repo/kn",
  defaultProfile: "deepseek",
  pinned: false,
};

describe("ProjectWorkspace", () => {
  it("shows project tabs without a Profiles tab", () => {
    render(
      <ProjectWorkspace
        project={project}
        sessions={[]}
        cliUsageRows={[]}
        onRunDefault={vi.fn()}
        onChangeDefaultProfile={vi.fn()}
      />,
    );

    expect(screen.getByText("Overview")).not.toBeNull();
    expect(screen.getByText("Sessions")).not.toBeNull();
    expect(screen.getByText("Project Skills")).not.toBeNull();
    expect(screen.queryByText("Profiles")).toBeNull();
  });

  it("switches to Project Hooks tab", () => {
    render(
      <ProjectWorkspace
        project={project}
        sessions={[]}
        cliUsageRows={[]}
        onRunDefault={vi.fn()}
        onChangeDefaultProfile={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByText("Project Hooks"));

    expect(screen.getByText("当前项目 Hooks")).not.toBeNull();
  });
});
