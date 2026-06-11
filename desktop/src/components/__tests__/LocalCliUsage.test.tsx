/// <reference types="vitest" />
import { render, screen } from "@testing-library/react";
import React from "react";
import { describe, expect, it } from "vitest";
import { LocalCliUsage } from "../LocalCliUsage";

describe("LocalCliUsage", () => {
  it("renders one compact row per CLI with version and stats", () => {
    render(
      <LocalCliUsage
        rows={[
          { cli: "Claude", version: "1.0.0", installed: true, runs: 2, sessions: 1, tokens: 1200, lastUsed: "今天" },
          { cli: "Codex", version: "0.31.0", installed: true, runs: 5, sessions: 3, tokens: 4000, lastUsed: "刚刚" },
          { cli: "Qoder", version: null, installed: false, runs: 0, sessions: 0, tokens: 0, lastUsed: "-" },
        ]}
      />,
    );

    expect(screen.getByText("Claude")).not.toBeNull();
    expect(screen.getByText("1.0.0")).not.toBeNull();
    expect(screen.getByText("Codex")).not.toBeNull();
    expect(screen.getByText("Qoder")).not.toBeNull();
    expect(screen.getByText("未安装")).not.toBeNull();
  });
});
