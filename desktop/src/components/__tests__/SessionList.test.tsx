/// <reference types="vitest" />
import { fireEvent, render, screen } from "@testing-library/react";
import React from "react";
import { describe, expect, it, vi } from "vitest";
import { SessionList } from "../SessionList";

describe("SessionList", () => {
  it("requires a manually selected compatible Profile before resuming", () => {
    const onResume = vi.fn();
    render(
      <SessionList
        sessions={[{
          sessionId: "session-1",
          title: "History session",
          cli: "codex",
          profile: "old-profile",
          projectPath: "/repo",
          workDir: "/repo",
          timestamp: Date.now(),
          status: "ended",
        }]}
        loading={false}
        profiles={[
          { name: "new-codex", desc: "", env_count: 0, is_default: false, cli_type: "codex" },
          { name: "claude-only", desc: "", env_count: 0, is_default: false, cli_type: "claude" },
        ]}
        onResume={onResume}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "恢复" }));

    const confirm = screen.getByRole("button", { name: "恢复会话" });
    expect((confirm as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getByRole("option", { name: "new-codex" })).not.toBeNull();
    expect(screen.queryByRole("option", { name: "claude-only" })).toBeNull();

    fireEvent.change(screen.getByRole("combobox"), { target: { value: "new-codex" } });
    fireEvent.click(confirm);

    expect(onResume).toHaveBeenCalledWith(expect.objectContaining({ sessionId: "session-1" }), "new-codex");
  });
});
