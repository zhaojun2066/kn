/// <reference types="vitest" />
import { fireEvent, render, screen } from "@testing-library/react";
import React from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

// Mock Tauri APIs before importing ProjectWorkspace
const { invokeMock } = vi.hoisted(() => ({
  invokeMock: vi.fn().mockImplementation((cmd: string) => {
    if (cmd === "scan_hooks") return Promise.resolve({ hooks: [] });
    if (cmd === "get_home_dir") return Promise.resolve("/home/test");
    return Promise.resolve({ plugins: [], standaloneSkills: [], systemSkills: [], commands: [] });
  }),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

import { ProjectWorkspace } from "../ProjectWorkspace";
import type { ProjectInfo } from "../../lib/types";

const project: ProjectInfo = {
  name: "kn",
  path: "/repo/kn",
  defaultProfile: "deepseek",
  pinned: false,
};

describe("ProjectWorkspace", () => {
  beforeEach(() => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "scan_hooks") return Promise.resolve({ hooks: [] });
      if (cmd === "get_home_dir") return Promise.resolve("/home/test");
      return Promise.resolve({ plugins: [], standaloneSkills: [], systemSkills: [], commands: [] });
    });
  });

  it("shows project tabs without a Profiles tab", () => {
    render(
      <ProjectWorkspace
        project={project}
        sessions={[]}
        sessionsLoading={false}
        cliUsageRows={[]}
        profiles={[]}
        onRunProfile={vi.fn()}
        onSetDefaultProfile={vi.fn()}
        onScanSessions={vi.fn()}
        onResumeSession={vi.fn()}
        addToast={vi.fn()}
        setToasts={vi.fn()}
        toastIdRef={{ current: 0 }}
        projects={[]}
        onAddProject={vi.fn()}
      />,
    );

    expect(screen.getByText("Overview")).not.toBeNull();
    expect(screen.getByText("Sessions")).not.toBeNull();
    expect(screen.getByText("Resource")).not.toBeNull();
    expect(screen.queryByText("Profiles")).toBeNull();
  });

  it("switches to Project Hooks tab", () => {
    render(
      <ProjectWorkspace
        project={project}
        sessions={[]}
        sessionsLoading={false}
        cliUsageRows={[]}
        profiles={[]}
        onRunProfile={vi.fn()}
        onSetDefaultProfile={vi.fn()}
        onScanSessions={vi.fn()}
        onResumeSession={vi.fn()}
        addToast={vi.fn()}
        setToasts={vi.fn()}
        toastIdRef={{ current: 0 }}
        projects={[]}
        onAddProject={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByText("Hooks"));

    // After switching to hooks tab, HookDetail renders its empty state
    // (HookList + HookDetail layout replaces the old placeholder)
    // In ProjectWorkspace, scope="project" so the hint shows the project-level message
    expect(screen.getByText("项目级 Hook 存储在项目目录的 CLI 配置文件中，可通过 Git 与团队共享。")).not.toBeNull();
  });

  it("shows inherited user hooks in the project Hooks tab", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_home_dir") return Promise.resolve("/home/test");
      if (cmd === "scan_skills") return Promise.resolve({ plugins: [], standaloneSkills: [], systemSkills: [], commands: [] });
      if (cmd === "scan_agents") return Promise.resolve({ agents: [] });
      if (cmd === "scan_hooks") {
        return Promise.resolve({
          hooks: [
            {
              id: "claude:hook:user",
              cli: "claude",
              eventType: "UserPromptSubmit",
              command: "echo user",
              hookType: "command",
              enabled: true,
              source: "user",
              path: "/home/test/.claude/settings.json",
              groupIdx: 0,
              hookIdx: 0,
              name: "user-hook",
            },
            {
              id: "claude:project-hook:abc:project",
              cli: "claude",
              eventType: "UserPromptSubmit",
              command: "echo project",
              hookType: "command",
              enabled: true,
              source: "project",
              path: "/repo/kn/.claude/settings.json",
              groupIdx: 0,
              hookIdx: 1,
              projectName: "kn",
              name: "project-hook",
            },
          ],
        });
      }
      return Promise.resolve({});
    });

    render(
      <ProjectWorkspace
        project={project}
        sessions={[]}
        sessionsLoading={false}
        cliUsageRows={[]}
        profiles={[]}
        onRunProfile={vi.fn()}
        onSetDefaultProfile={vi.fn()}
        onScanSessions={vi.fn()}
        onResumeSession={vi.fn()}
        addToast={vi.fn()}
        setToasts={vi.fn()}
        toastIdRef={{ current: 0 }}
        projects={[]}
        onAddProject={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByText("Hooks"));

    expect(await screen.findByText("user-hook")).not.toBeNull();
    expect(await screen.findByText("project-hook")).not.toBeNull();
    expect(screen.queryAllByTitle("kn")).toHaveLength(0);

    fireEvent.click(screen.getByText("全部来源"));
    fireEvent.click(screen.getByText("本项目"));

    expect(screen.queryByText("user-hook")).toBeNull();
    expect(await screen.findByText("project-hook")).not.toBeNull();

    fireEvent.click(screen.getByText("本项目"));
    fireEvent.click(screen.getByText("继承"));

    expect(await screen.findByText("user-hook")).not.toBeNull();
    expect(screen.queryByText("project-hook")).toBeNull();
  });

  it("shows inherited user plugins in the project Resource tab", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "scan_hooks") return Promise.resolve({ hooks: [] });
      if (cmd === "get_home_dir") return Promise.resolve("/home/test");
      if (cmd === "scan_agents") return Promise.resolve({ agents: [] });
      if (cmd === "scan_skills") {
        return Promise.resolve({
          plugins: [
            {
              id: "codex:plugin:chrome@openai-bundled",
              cli: "codex",
              name: "chrome",
              marketplace: "openai-bundled",
              enabled: true,
              source: "bundled",
              skills: [],
              agents: [],
              commands: [],
            },
            {
              id: "codex:project-plugin:abc:browser@openai-bundled",
              cli: "codex",
              name: "browser",
              marketplace: "openai-bundled",
              enabled: true,
              source: "project",
              skills: [],
              agents: [],
              commands: [],
            },
          ],
          standaloneSkills: [],
          systemSkills: [],
          commands: [],
        });
      }
      return Promise.resolve({});
    });

    render(
      <ProjectWorkspace
        project={project}
        sessions={[]}
        sessionsLoading={false}
        cliUsageRows={[]}
        profiles={[]}
        onRunProfile={vi.fn()}
        onSetDefaultProfile={vi.fn()}
        onScanSessions={vi.fn()}
        onResumeSession={vi.fn()}
        addToast={vi.fn()}
        setToasts={vi.fn()}
        toastIdRef={{ current: 0 }}
        projects={[]}
        onAddProject={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByText("Resource"));

    expect(await screen.findByText("chrome")).not.toBeNull();
    expect(await screen.findByText("browser")).not.toBeNull();
    expect(await screen.findByText("继承")).not.toBeNull();
  });

  it("filters the project Resource tab to project-local resources", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "scan_hooks") return Promise.resolve({ hooks: [] });
      if (cmd === "get_home_dir") return Promise.resolve("/home/test");
      if (cmd === "scan_agents") return Promise.resolve({ agents: [] });
      if (cmd === "scan_skills") {
        return Promise.resolve({
          plugins: [
            {
              id: "codex:plugin:chrome@openai-bundled",
              cli: "codex",
              name: "chrome",
              marketplace: "openai-bundled",
              enabled: true,
              source: "bundled",
              inherited: true,
              skills: [],
              agents: [],
              commands: [],
            },
            {
              id: "codex:project-plugin:abc:browser@openai-bundled",
              cli: "codex",
              name: "browser",
              marketplace: "openai-bundled",
              enabled: true,
              source: "project",
              skills: [],
              agents: [],
              commands: [],
            },
          ],
          standaloneSkills: [
            {
              id: "codex:project-skill:abc:lint",
              cli: "codex",
              name: "lint",
              enabled: true,
              linkType: "file",
              path: "/repo/kn/.codex/skills/lint/SKILL.md",
              projectName: "kn",
            },
          ],
          systemSkills: [],
          commands: [],
        });
      }
      return Promise.resolve({});
    });

    render(
      <ProjectWorkspace
        project={project}
        sessions={[]}
        sessionsLoading={false}
        cliUsageRows={[]}
        profiles={[]}
        onRunProfile={vi.fn()}
        onSetDefaultProfile={vi.fn()}
        onScanSessions={vi.fn()}
        onResumeSession={vi.fn()}
        addToast={vi.fn()}
        setToasts={vi.fn()}
        toastIdRef={{ current: 0 }}
        projects={[]}
        onAddProject={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByText("Resource"));

    expect(await screen.findByText("chrome")).not.toBeNull();
    expect(await screen.findByText("browser")).not.toBeNull();
    expect(await screen.findByText("lint")).not.toBeNull();

    fireEvent.click(screen.getByText("全部来源"));
    fireEvent.click(screen.getByText("本项目"));

    expect(screen.queryByText("chrome")).toBeNull();
    expect(await screen.findByText("browser")).not.toBeNull();
    expect(await screen.findByText("lint")).not.toBeNull();
  });

  it("shows inherited user skills commands and agents in the project Resource tab", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "scan_hooks") return Promise.resolve({ hooks: [] });
      if (cmd === "get_home_dir") return Promise.resolve("/home/test");
      if (cmd === "scan_agents") {
        return Promise.resolve({
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
        });
      }
      if (cmd === "scan_skills") {
        return Promise.resolve({
          plugins: [],
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
        });
      }
      return Promise.resolve({});
    });

    render(
      <ProjectWorkspace
        project={project}
        sessions={[]}
        sessionsLoading={false}
        cliUsageRows={[]}
        profiles={[]}
        onRunProfile={vi.fn()}
        onSetDefaultProfile={vi.fn()}
        onScanSessions={vi.fn()}
        onResumeSession={vi.fn()}
        addToast={vi.fn()}
        setToasts={vi.fn()}
        toastIdRef={{ current: 0 }}
        projects={[]}
        onAddProject={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByText("Resource"));

    expect(await screen.findByText("user-skill")).not.toBeNull();
    expect(await screen.findByText("project-skill")).not.toBeNull();
    expect(await screen.findByText("/user-command")).not.toBeNull();
    expect(await screen.findByText("/project-command")).not.toBeNull();
    expect(await screen.findByText("user-agent")).not.toBeNull();
    expect(await screen.findByText("project-agent")).not.toBeNull();
    expect(screen.queryAllByTitle("kn")).toHaveLength(0);

    fireEvent.click(screen.getByText("全部来源"));
    fireEvent.click(screen.getByText("本项目"));

    expect(screen.queryByText("user-skill")).toBeNull();
    expect(screen.queryByText("/user-command")).toBeNull();
    expect(screen.queryByText("user-agent")).toBeNull();
    expect(await screen.findByText("project-skill")).not.toBeNull();
    expect(await screen.findByText("/project-command")).not.toBeNull();
    expect(await screen.findByText("project-agent")).not.toBeNull();
  });

  it("shows the project name in the project marketplace dialog title", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "scan_hooks") return Promise.resolve({ hooks: [] });
      if (cmd === "get_home_dir") return Promise.resolve("/home/test");
      if (cmd === "scan_agents") return Promise.resolve({ agents: [] });
      if (cmd === "scan_skills") {
        return Promise.resolve({ plugins: [], standaloneSkills: [], systemSkills: [], commands: [] });
      }
      if (cmd === "list_marketplace_plugins") {
        return Promise.resolve({ plugins: [], marketplaces: [] });
      }
      return Promise.resolve({});
    });

    render(
      <ProjectWorkspace
        project={project}
        sessions={[]}
        sessionsLoading={false}
        cliUsageRows={[]}
        profiles={[]}
        onRunProfile={vi.fn()}
        onSetDefaultProfile={vi.fn()}
        onScanSessions={vi.fn()}
        onResumeSession={vi.fn()}
        addToast={vi.fn()}
        setToasts={vi.fn()}
        toastIdRef={{ current: 0 }}
        projects={[]}
        onAddProject={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByText("Resource"));
    fireEvent.click(await screen.findByTitle("浏览 Marketplace"));

    expect(await screen.findByText("Plugin Marketplace")).not.toBeNull();
    expect(await screen.findByText("项目 · kn")).not.toBeNull();
  });
});
