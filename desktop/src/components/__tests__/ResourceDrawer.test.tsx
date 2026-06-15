/// <reference types="vitest" />
import { render, screen } from "@testing-library/react";
import React from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

// Mock Tauri APIs before importing ResourceDrawer
const { invokeMock } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
}));

const mockDefaultInvoke = () => {
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "scan_skills") return { plugins: [], standaloneSkills: [], systemSkills: [], commands: [] };
    if (cmd === "scan_agents") return { agents: [] };
    if (cmd === "list_projects") return [];
    if (cmd === "get_project_stats") return {};
    return {};
  });
};

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

vi.mock("../../hooks/useProjects", () => ({
  useProjects: () => ({
    projects: [],
    activeProject: null,
    loading: false,
    statsMap: {},
    setActiveProject: vi.fn(),
    loadProjects: vi.fn(),
    addProject: vi.fn(),
    removeProject: vi.fn(),
    updateProject: vi.fn(),
    setDefaultProfile: vi.fn(),
    setDescription: vi.fn(),
    togglePin: vi.fn(),
  }),
}));

vi.mock("../../hooks/useToasts", () => ({
  useToasts: () => ({
    addToast: vi.fn(),
    toasts: [],
    setToasts: vi.fn(),
    toastIdRef: { current: 0 },
    dismissToast: vi.fn(),
  }),
}));

import { ResourceDrawer } from "../ResourceDrawer";

describe("ResourceDrawer", () => {
  beforeEach(() => {
    mockDefaultInvoke();
  });

  it("renders nothing when closed", () => {
    const { container } = render(<ResourceDrawer open={false} onClose={vi.fn()} />);
    expect(container.innerHTML).toBe("");
  });

  it("renders with Resource title when open", () => {
    // Just verify the drawer renders — "Resource" appears in both header and ResourceList
    render(<ResourceDrawer open onClose={vi.fn()} />);
    const elements = screen.getAllByText("Resource");
    expect(elements.length).toBeGreaterThanOrEqual(1);
    expect(screen.getByRole("button", { name: "关闭资源管理" })).toBeTruthy();
  });

  it("does not show project-level resources", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "scan_skills") {
        return {
          plugins: [
            {
              id: "codex:plugin:user@market",
              cli: "codex",
              name: "user-plugin",
              marketplace: "market",
              enabled: true,
              source: "user",
              skills: [],
              agents: [],
              commands: [],
            },
            {
              id: "codex:project-plugin:abc:project@market",
              cli: "codex",
              name: "project-plugin",
              marketplace: "market",
              enabled: true,
              source: "project",
              skills: [],
              agents: [],
              commands: [],
            },
          ],
          standaloneSkills: [
            {
              id: "codex:skill:user-skill",
              cli: "codex",
              name: "user-skill",
              enabled: true,
              linkType: "file",
              path: "/home/test/.codex/skills/user/SKILL.md",
            },
            {
              id: "codex:project-skill:abc:project-skill",
              cli: "codex",
              name: "project-skill",
              enabled: true,
              linkType: "file",
              path: "/repo/kn/.codex/skills/project/SKILL.md",
              projectName: "kn",
            },
          ],
          systemSkills: [],
          commands: [
            {
              id: "codex:command:user-command",
              cli: "codex",
              name: "user-command",
              path: "/home/test/.codex/commands/user.md",
              description: "",
              enabled: true,
            },
            {
              id: "codex:project-command:abc:project-command",
              cli: "codex",
              name: "project-command",
              path: "/repo/kn/.codex/commands/project.md",
              description: "",
              enabled: true,
              projectName: "kn",
            },
          ],
        };
      }
      if (cmd === "scan_agents") {
        return {
          agents: [
            {
              id: "codex:agent:user-agent",
              cli: "codex",
              name: "user-agent",
              description: "",
              enabled: true,
              source: "user",
              tools: [],
              path: "/home/test/.codex/agents/user.toml",
              skills: [],
            },
            {
              id: "codex:project-agent:abc:project-agent",
              cli: "codex",
              name: "project-agent",
              description: "",
              enabled: true,
              source: "project",
              tools: [],
              path: "/repo/kn/.codex/agents/project.toml",
              skills: [],
              projectName: "kn",
            },
          ],
        };
      }
      if (cmd === "list_projects") return [];
      if (cmd === "get_project_stats") return {};
      return {};
    });

    render(<ResourceDrawer open onClose={vi.fn()} />);

    expect(await screen.findByText("user-plugin")).toBeTruthy();
    expect(await screen.findByText("user-skill")).toBeTruthy();
    expect(await screen.findByText("user-agent")).toBeTruthy();
    expect(screen.queryByText("project-plugin")).toBeNull();
    expect(screen.queryByText("project-skill")).toBeNull();
    expect(screen.queryByText("project-agent")).toBeNull();
  });

  it("labels plugin source by real scope instead of raw marketplace source", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "scan_skills") {
        return {
          plugins: [
            {
              id: "claude:plugin:claude-user@market",
              cli: "claude",
              name: "claude-user",
              marketplace: "market",
              enabled: true,
              source: "marketplace",
              skills: [],
              agents: [],
              commands: [],
            },
            {
              id: "codex:plugin:codex-user@market",
              cli: "codex",
              name: "codex-user",
              marketplace: "market",
              enabled: true,
              source: "user@market",
              skills: [],
              agents: [],
              commands: [],
            },
            {
              id: "codex:plugin:codex-bundled@openai-bundled",
              cli: "codex",
              name: "codex-bundled",
              marketplace: "openai-bundled",
              enabled: true,
              source: "bundled@openai-bundled",
              skills: [],
              agents: [],
              commands: [],
            },
          ],
          standaloneSkills: [],
          systemSkills: [],
          commands: [],
        };
      }
      if (cmd === "scan_agents") return { agents: [] };
      if (cmd === "list_projects") return [];
      if (cmd === "get_project_stats") return {};
      return {};
    });

    render(<ResourceDrawer open onClose={vi.fn()} />);

    expect(await screen.findByText("claude-user")).toBeTruthy();
    expect(await screen.findByText("codex-user")).toBeTruthy();
    expect(await screen.findByText("codex-bundled")).toBeTruthy();
    expect(screen.getAllByTitle("用户")).toHaveLength(2);
    expect(screen.getAllByTitle("内置")).toHaveLength(1);
  });
});
