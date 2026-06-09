/// <reference types="vitest" />
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import React from "react";
import { Sidebar } from "../Sidebar";
import type { ProfileSummary } from "../../lib/types";

const makeProfile = (overrides: Partial<ProfileSummary> = {}): ProfileSummary => ({
  name: "test-profile",
  desc: "",
  env_count: 2,
  is_default: false,
  cli_type: "claude",
  ...overrides,
});

const defaultProps = {
  profiles: ["alpha", "bravo", "charlie"].map((n) =>
    makeProfile({ name: n, is_default: n === "alpha" }),
  ),
  selectedName: "alpha",
  searchQuery: "",
  onSelect: vi.fn(),
  onSearch: vi.fn(),
  onCopy: vi.fn(),
  onRename: vi.fn(),
  onDelete: vi.fn(),
  onSetDefault: vi.fn(),
  usageCounts: {},
};

beforeEach(() => {
  vi.clearAllMocks();
});

describe("Sidebar — right-click context menu", () => {

  it("copy calls onCopy with right-clicked profile name", () => {
    const onCopy = vi.fn();
    render(<Sidebar {...defaultProps} onCopy={onCopy} />);
    fireEvent.contextMenu(screen.getByText("charlie"));
    fireEvent.click(screen.getByText("复制"));
    expect(onCopy).toHaveBeenCalledWith("charlie");
  });

  it("rename calls onRename with right-clicked profile name", () => {
    const onRename = vi.fn();
    render(<Sidebar {...defaultProps} onRename={onRename} />);
    fireEvent.contextMenu(screen.getByText("charlie"));
    fireEvent.click(screen.getByText("重命名"));
    expect(onRename).toHaveBeenCalledWith("charlie");
  });

  it("delete calls onDelete with right-clicked profile name", () => {
    const onDelete = vi.fn();
    render(<Sidebar {...defaultProps} onDelete={onDelete} />);
    fireEvent.contextMenu(screen.getByText("charlie"));
    fireEvent.click(screen.getByText("删除"));
    expect(onDelete).toHaveBeenCalledWith("charlie");
  });

  it("set-default calls onSetDefault with right-clicked profile name", () => {
    const onSetDefault = vi.fn();
    render(<Sidebar {...defaultProps} onSetDefault={onSetDefault} />);
    fireEvent.contextMenu(screen.getByText("charlie"));
    fireEvent.click(screen.getByText("设为默认"));
    expect(onSetDefault).toHaveBeenCalledWith("charlie");
  });
});

describe("Sidebar — left-click selection", () => {
  it("left-click calls onSelect with clicked profile name", () => {
    const onSelect = vi.fn();
    render(<Sidebar {...defaultProps} onSelect={onSelect} />);
    fireEvent.click(screen.getByText("bravo"));
    expect(onSelect).toHaveBeenCalledWith("bravo");
  });

  it("right-click does NOT call onSelect (selection unchanged)", () => {
    const onSelect = vi.fn();
    render(<Sidebar {...defaultProps} onSelect={onSelect} />);
    fireEvent.contextMenu(screen.getByText("bravo"));
    expect(onSelect).not.toHaveBeenCalled();
  });
});
