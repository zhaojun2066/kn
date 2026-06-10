/// <reference types="vitest" />
import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import React from "react";
import { StatusBar } from "../StatusBar";
import type { ProfileSummary } from "../../lib/types";

const profiles: ProfileSummary[] = [{
  name: "default",
  desc: "",
  env_count: 1,
  is_default: true,
  cli_type: "claude",
}];

const baseProps = {
  loading: false,
  profiles,
  terminalOpen: false,
  colorScheme: "light",
  selectedName: "default",
  defaultProfile: "default",
  appVersion: "1.0.0",
  onShowUsage: vi.fn(),
};

describe("StatusBar", () => {
  it("hides zero token usage while usage is still loading", () => {
    render(
      <StatusBar
        {...baseProps}
        usage={{ todayTokens: 0, loading: true }}
      />,
    );

    expect(screen.queryByTitle("查看 Token 用量")).toBeNull();
  });

  it("shows zero token usage after usage loading completes", () => {
    render(
      <StatusBar
        {...baseProps}
        usage={{ todayTokens: 0, loading: false }}
      />,
    );

    expect(screen.getByTitle("查看 Token 用量").textContent).toContain("◉ 0");
  });
});
