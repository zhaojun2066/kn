/// <reference types="vitest" />
import { fireEvent, render, screen } from "@testing-library/react";
import React from "react";
import { describe, expect, it, vi } from "vitest";
import { ProfileDrawer } from "../ProfileDrawer";
import type { ProfileSummary } from "../../lib/types";

const profiles: ProfileSummary[] = [
  { name: "deepseek", desc: "DeepSeek profile", env_count: 2, is_default: true, cli_type: "claude" },
  { name: "work", desc: "Work profile", env_count: 1, is_default: false, cli_type: "codex", tags: ["team"] },
];

function renderDrawer(overrides: Partial<React.ComponentProps<typeof ProfileDrawer>> = {}) {
  const props: React.ComponentProps<typeof ProfileDrawer> = {
    open: true,
    profiles,
    selectedName: "deepseek",
    searchQuery: "",
    onClose: vi.fn(),
    onSelect: vi.fn(),
    onSearch: vi.fn(),
    onCopy: vi.fn(),
    onRename: vi.fn(),
    onDelete: vi.fn(),
    onSetDefault: vi.fn(),
    usageCounts: { work: 3 },
    isDefault: true,
    hasSelection: true,
    backupExists: true,
    onAdd: vi.fn(),
    onCopyProfile: vi.fn(),
    onInit: vi.fn(),
    onImport: vi.fn(),
    onExport: vi.fn(),
    onBatchDelete: vi.fn(),
    onBatchExport: vi.fn(),
    onRefresh: vi.fn(),
    onBackup: vi.fn(),
    onRestore: vi.fn(),
    // MainPanel props
    selectedProfile: null,
    allTags: [],
    history: [],
    envCheck: null,
    onSetEnv: vi.fn(),
    onDeleteEnv: vi.fn(),
    onPasteCommand: vi.fn(),
    onSplitCommand: vi.fn(),
    onRenameProfile: vi.fn(),
    onResumeSession: vi.fn(),
    onNewSessionFromHistory: vi.fn(),
    onDeleteHistory: vi.fn(),
    onClearProfileHistory: vi.fn(),
    onSetTags: vi.fn(),
    ...overrides,
  };

  return {
    props,
    ...render(<ProfileDrawer {...props} />),
  };
}

describe("ProfileDrawer", () => {
  it("renders a bottom drawer with the full profile list UI", () => {
    renderDrawer();

    expect(screen.getByText("Profile Management")).not.toBeNull();
    expect(screen.getByText("全部标签")).not.toBeNull();
    expect(screen.getByTitle("更多操作")).not.toBeNull();
    expect(screen.queryByText("全局 Profile")).toBeNull();
  });

  it("allows search, selection, context actions, and toolbar actions", () => {
    const onSearch = vi.fn();
    const onSelect = vi.fn();
    const onRename = vi.fn();
    const onAdd = vi.fn();
    const onRefresh = vi.fn();

    renderDrawer({ onSearch, onSelect, onRename, onAdd, onRefresh });

    fireEvent.change(screen.getByPlaceholderText("搜索 profile..."), { target: { value: "work" } });
    expect(onSearch).toHaveBeenCalledWith("work");
    fireEvent.click(screen.getByText("work"));
    expect(onSelect).toHaveBeenCalledWith("work");

    fireEvent.contextMenu(screen.getByText("work"));
    fireEvent.click(screen.getByText("重命名"));
    expect(onRename).toHaveBeenCalledWith("work");

    fireEvent.click(screen.getByTitle("新增 Profile"));
    expect(onAdd).toHaveBeenCalled();
    fireEvent.click(screen.getByTitle("更多操作"));
    fireEvent.click(screen.getByTitle("刷新配置"));
    expect(onRefresh).toHaveBeenCalled();
  });

  it("keeps drawer selection separate from the main profile selection", () => {
    const onDrawerSelect = vi.fn();
    const onGlobalSelect = vi.fn();

    renderDrawer({ onSelect: onDrawerSelect });

    fireEvent.click(screen.getByText("work"));

    expect(onDrawerSelect).toHaveBeenCalledWith("work");
    expect(onGlobalSelect).not.toHaveBeenCalled();
  });
});
