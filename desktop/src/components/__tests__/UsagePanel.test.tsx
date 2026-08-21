/// <reference types="vitest" />
import { fireEvent, render, screen } from "@testing-library/react";
import React from "react";
import { describe, expect, it, vi } from "vitest";

const daily = Array.from({ length: 30 }, (_, day) => ({
  date: `2026-08-${String(day + 1).padStart(2, "0")}`,
  tokens_in: (day + 1) * 100,
  tokens_out: (day + 1) * 50,
}));

vi.mock("../../hooks/useUsage", () => ({
  useUsage: () => ({
    summary: {
      total_tokens_in: 4_650,
      total_tokens_out: 2_325,
      by_model: [],
    },
    daily,
    projectUsage: [],
    loading: false,
    refresh: vi.fn(),
  }),
}));

import { UsagePanel } from "../UsagePanel";

describe("UsagePanel", () => {
  it("keeps a 30-day trend inside a horizontally scrollable track", () => {
    render(<UsagePanel open onClose={vi.fn()} />);

    fireEvent.click(screen.getByRole("button", { name: "近 30 天" }));

    const scrollArea = screen.getByTestId("usage-trend-scroll");
    expect(scrollArea.className).toContain("overflow-x-auto");
    expect(scrollArea.firstElementChild?.className).toContain("min-w-max");
    expect(scrollArea.firstElementChild?.children).toHaveLength(30);
  });
});
