/// <reference types="vitest" />
import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useProjectContext } from "../useProjectContext";
import type { ProjectInfo } from "../../lib/types";

const projectA: ProjectInfo = {
  name: "kn",
  path: "/repo/kn",
  defaultProfile: "deepseek",
  pinned: false,
};

const projectB: ProjectInfo = {
  name: "site",
  path: "/repo/site",
  defaultProfile: "work",
  pinned: true,
};

beforeEach(() => {
  localStorage.clear();
  vi.clearAllMocks();
});

describe("useProjectContext", () => {
  it("stores selected project and persists it by name", () => {
    const { result } = renderHook(() => useProjectContext([projectA, projectB]));

    act(() => result.current.setActiveProject(projectA));

    expect(result.current.activeProject).toEqual(projectA);
    expect(localStorage.getItem("kn-active-project")).toBe("kn");
  });

  it("restores active project from localStorage when project still exists", () => {
    localStorage.setItem("kn-active-project", "site");

    const { result } = renderHook(() => useProjectContext([projectA, projectB]));

    expect(result.current.activeProject).toEqual(projectB);
  });

  it("clears stale stored project names", () => {
    localStorage.setItem("kn-active-project", "missing");

    const { result } = renderHook(() => useProjectContext([projectA]));

    expect(result.current.activeProject).toBeNull();
    expect(localStorage.getItem("kn-active-project")).toBeNull();
  });

  it("can activate project from a path", () => {
    const { result } = renderHook(() => useProjectContext([projectA, projectB]));

    act(() => result.current.activateProjectByPath("/repo/site/src"));

    expect(result.current.activeProject).toEqual(projectB);
  });
});
