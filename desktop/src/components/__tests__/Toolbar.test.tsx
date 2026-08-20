/// <reference types="vitest" />
import { fireEvent, render, screen } from "@testing-library/react";
import React from "react";
import { describe, expect, it, vi } from "vitest";
import { Toolbar } from "../Toolbar";

vi.mock("../../hooks/useTheme", () => ({
  useTheme: () => ({ mode: "light", setTheme: vi.fn() }),
}));

const baseProps = {
  onToggleTerminal: vi.fn(),
  onShowHelp: vi.fn(),
  onShowOnboarding: vi.fn(),
  onShowShortcuts: vi.fn(),
  onCheckUpdate: vi.fn(),
  onAbout: vi.fn(),
  onSettings: vi.fn(),
  sidebarVisible: true,
  onToggleSidebar: vi.fn(),
  terminalVisible: false,
  rightTerminalVisible: false,
  onToggleRightTerminal: vi.fn(),
  envCheck: null,
  onQuickSwitcher: vi.fn(),
  onQuickHistory: vi.fn(),
};

describe("Toolbar", () => {
  it("shows a separate onboarding entry in the gear menu", () => {
    const onShowOnboarding = vi.fn();
    render(<Toolbar {...baseProps} onShowOnboarding={onShowOnboarding} />);

    const buttons = screen.getAllByRole("button");
    fireEvent.click(buttons[buttons.length - 1]);
    fireEvent.click(screen.getByText("引导"));

    expect(onShowOnboarding).toHaveBeenCalledTimes(1);
    expect(baseProps.onShowHelp).not.toHaveBeenCalled();
  });
});
