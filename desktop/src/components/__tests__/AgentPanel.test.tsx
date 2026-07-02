/// <reference types="vitest" />
import { describe, expect, it } from "vitest";
import { buildKillSessionIpcArgs } from "../AgentPanel";
import type { AgentSession } from "../../hooks/useAgent";

function session(nid: string): AgentSession {
  return {
    nid,
    kind: "Native",
    source: "desktop",
    tool: "claude",
    profile: "default",
    cwd: "/tmp/project",
    created_at: "2026-07-02T00:00:00Z",
    status: "running",
    remote_enabled: true,
  };
}

describe("AgentPanel", () => {
  it("marks desktop process termination with process_killed", () => {
    expect(buildKillSessionIpcArgs(session("s_1"))).toEqual({
      method: "kill_session",
      params: { nid: "s_1", reason: "process_killed" },
    });
  });
});
