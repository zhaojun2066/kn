import { describe, expect, it } from "vitest";
import { buildPromptLibraryChanges, canAddPrompt, orderPromptsByCategory, promptCategories, systemPrompts } from "../promptLibrary";

describe("prompt library", () => {
  it("keeps the fixed categories and system presets outside the custom limit", () => {
    expect(promptCategories).toEqual(["review", "development", "other"]);
    expect(systemPrompts.length).toBeGreaterThan(0);
    expect(canAddPrompt(29)).toBe(true);
    expect(canAddPrompt(30)).toBe(false);
  });

  it("uses one category order for visual rows and keyboard selection", () => {
    const ordered = orderPromptsByCategory([
      { uuid: "other", title: "other", content: "other", category: "other", sortOrder: 0 },
      { uuid: "review", title: "review", content: "review", category: "review", sortOrder: 0 },
      { uuid: "dev", title: "dev", content: "dev", category: "development", sortOrder: 0 },
    ]);
    expect(ordered.map((prompt) => prompt.uuid)).toEqual(["review", "dev", "other"]);
  });

  it("does not delete an old UUID that was intentionally removed when sync was disabled", () => {
    const oldPrompt = { uuid: "old", title: "本机", content: "内容", category: "other" as const, sortOrder: 0, revision: 3 };
    const newPrompt = { ...oldPrompt, uuid: "new", revision: 0, cloudDeletedLocallyRetained: false };
    const changes = buildPromptLibraryChanges([oldPrompt], [newPrompt], false);
    expect(changes).toEqual([
      expect.objectContaining({ type: "create", uuid: "new" }),
    ]);
  });
});
