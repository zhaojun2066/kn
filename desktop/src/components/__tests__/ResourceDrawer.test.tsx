/// <reference types="vitest" />
import { fireEvent, render, screen } from "@testing-library/react";
import React from "react";
import { describe, expect, it, vi } from "vitest";
import { ResourceDrawer } from "../ResourceDrawer";

describe("ResourceDrawer", () => {
  it("renders global resource categories and close button", () => {
    const onClose = vi.fn();

    render(<ResourceDrawer open onClose={onClose} />);

    expect(screen.getByText("Resource Management")).not.toBeNull();
    expect(screen.getByText("Plugins")).not.toBeNull();
    expect(screen.getByText("Skills")).not.toBeNull();
    expect(screen.getByText("Agents")).not.toBeNull();
    expect(screen.getByText("Hooks")).not.toBeNull();
    expect(screen.getByText("Commands")).not.toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "关闭资源管理" }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
