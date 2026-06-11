/// <reference types="vitest" />
import { describe, expect, it, vi } from "vitest";
import { buildDestDir, getResourceData, getResourceType } from "../resource-transfer";
import type { SelectedItem } from "../../components/SkillManager";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async () => "/Users/alice"),
}));

describe("resource-transfer helpers", () => {
  it("detects resource data defensively", () => {
    const item = {
      type: "standalone",
      data: { id: "codex:skill:test", name: "test", path: "/tmp/test", cli: "codex" },
    } as SelectedItem;

    expect(getResourceData(item)).toEqual({
      id: "codex:skill:test",
      name: "test",
      path: "/tmp/test",
      cli: "codex",
      projectName: undefined,
    });
    expect(getResourceType(item)).toBe("skill");
  });

  it("builds user destination with Unix separators", async () => {
    await expect(buildDestDir("/Users/alice/.codex/skills/demo", undefined, "user", "skills"))
      .resolves.toBe("/Users/alice/.codex/skills");
  });

  it("builds project destination with Windows separators", async () => {
    await expect(buildDestDir(
      "C:\\Users\\alice\\.qoder-cn\\skills\\demo",
      "qoder",
      "project",
      "skills",
      { name: "repo", path: "C:\\work\\repo", pinned: false },
    )).resolves.toBe("C:\\work\\repo\\.qoder\\skills");
  });
});
