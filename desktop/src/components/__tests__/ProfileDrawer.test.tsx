/// <reference types="vitest" />
import { fireEvent, render, screen } from "@testing-library/react";
import React from "react";
import { describe, expect, it, vi } from "vitest";
import { ProfileDrawer } from "../ProfileDrawer";
import type { ProfileDetail, ProfileSummary } from "../../lib/types";

const profiles: ProfileSummary[] = [
  { name: "deepseek", desc: "DeepSeek profile", env_count: 2, is_default: true, cli_type: "claude" },
  { name: "work", desc: "Work profile", env_count: 1, is_default: false, cli_type: "codex" },
];

const selectedProfile: ProfileDetail = {
  name: "deepseek",
  desc: "DeepSeek profile",
  env: { API_KEY: "secret" },
  is_default: true,
};

describe("ProfileDrawer", () => {
  it("renders profiles and allows selection/search/run", () => {
    const onSelect = vi.fn();
    const onSearch = vi.fn();
    const onRunInCurrentProject = vi.fn();

    render(
      <ProfileDrawer
        open
        profiles={profiles}
        selectedProfile={selectedProfile}
        selectedName="deepseek"
        searchQuery=""
        onClose={vi.fn()}
        onSelect={onSelect}
        onSearch={onSearch}
        onAdd={vi.fn()}
        onRunInCurrentProject={onRunInCurrentProject}
      />,
    );

    expect(screen.getByText("Profile Management")).not.toBeNull();
    expect(screen.getAllByText("deepseek").length).toBeGreaterThan(0);
    fireEvent.change(screen.getByPlaceholderText("搜索 profile..."), { target: { value: "work" } });
    expect(onSearch).toHaveBeenCalledWith("work");
    fireEvent.click(screen.getByText("work"));
    expect(onSelect).toHaveBeenCalledWith("work");
    fireEvent.click(screen.getByRole("button", { name: "在当前项目运行" }));
    expect(onRunInCurrentProject).toHaveBeenCalledWith("deepseek");
  });
});
