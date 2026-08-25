/// <reference types="vitest" />
import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import React from "react";
import { AgentHealthSection } from "../AgentHealthSection";
import type { AgentHealthSnapshot } from "../../lib/healthReport";

const health: AgentHealthSnapshot = {
  schemaVersion: 1,
  generatedAt: 1,
  agent: { version: "1.2.6", environment: "production" },
  connection: { state: "connected" },
  tools: [],
};

describe("AgentHealthSection", () => {
  it("keeps a visible loading message while refreshing an existing report", () => {
    render(
      <AgentHealthSection
        health={health}
        isLoading
        error={null}
        onRefresh={vi.fn()}
      />,
    );

    expect(screen.getByText("正在检查…")).toBeTruthy();
    expect(screen.getByText("已连接")).toBeTruthy();
  });

  it("collapses and restores the health details", () => {
    render(
      <AgentHealthSection
        health={health}
        isLoading={false}
        error={null}
        onRefresh={vi.fn()}
      />,
    );

    const toggle = screen.getByTitle("收起设备健康");
    fireEvent.click(toggle);

    expect(toggle.getAttribute("aria-expanded")).toBe("false");
    expect(screen.queryByText("已连接")).toBeNull();
    expect(screen.getByTitle("展开设备健康")).toBeTruthy();
  });
});
