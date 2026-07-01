/// <reference types="vitest" />
import { describe, expect, it } from "vitest";
import type { AgentSession } from "../../useAgent";
import type { TabSession } from "../types";
import { syncNativeAgentSessions } from "../agentSessionSync";

const nativeSession: AgentSession = {
  nid: "s_native123",
  kind: "Native",
  source: "desktop",
  tool: "claude",
  profile: "claude-config",
  cwd: "/tmp/project",
  created_at: "2026-07-01T00:00:00Z",
  status: "running",
  remote_enabled: false,
};

const baseTab: TabSession = {
  id: "tab-1",
  name: "existing",
  workDir: "/tmp/project",
  rootNode: {
    type: "leaf",
    paneId: "pane-1",
    name: "existing",
    sessionId: "s_native123",
    workDir: "/tmp/project",
    ptyRunning: true,
  },
  activePaneId: "pane-1",
  zoomedPaneId: null,
  sessionId: "s_native123",
  ptyRunning: true,
  agentNid: "s_native123",
};

describe("agent session sync", () => {
  it("syncs running Native sessions into terminal tabs once", () => {
    const first = syncNativeAgentSessions([], [nativeSession]);
    const second = syncNativeAgentSessions(first, [nativeSession]);

    expect(first).toHaveLength(1);
    expect(first[0].agentNid).toBe("s_native123");
    expect(first[0].sessionId).toBe("s_native123");
    expect(second).toHaveLength(1);
  });

  it("ignores Relay and ended sessions when syncing tabs", () => {
    const relay: AgentSession = { ...nativeSession, nid: "s_relay", kind: "Relay" };
    const ended: AgentSession = { ...nativeSession, nid: "s_ended", status: "ended" };

    expect(syncNativeAgentSessions([], [relay, ended])).toEqual([]);
  });

  it("does not duplicate a tab whose root pane already uses the session id", () => {
    const synced = syncNativeAgentSessions([baseTab], [nativeSession]);

    expect(synced).toHaveLength(1);
    expect(synced[0]).toBe(baseTab);
  });
});
