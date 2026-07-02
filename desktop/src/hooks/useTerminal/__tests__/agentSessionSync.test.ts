/// <reference types="vitest" />
import { describe, expect, it } from "vitest";
import type { AgentSession } from "../../useAgent";
import type { TabSession } from "../types";
import { filterInitialVisibleAgentSessions, syncNativeAgentSessions } from "../agentSessionSync";

const nativeSession: AgentSession = {
  nid: "s_native123",
  kind: "Native",
  source: "desktop",
  tool: "claude",
  profile: "claude-config",
  cwd: "/tmp/project",
  created_at: "2026-07-01T00:00:00Z",
  status: "running",
  remote_enabled: true,
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
  agentRemoteEnabled: true,
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

  it("ignores Relay, ended, and remote-disabled Native sessions when syncing tabs", () => {
    const relay: AgentSession = { ...nativeSession, nid: "s_relay", kind: "Relay" };
    const ended: AgentSession = { ...nativeSession, nid: "s_ended", status: "ended" };
    const remoteDisabled: AgentSession = { ...nativeSession, nid: "s_local", remote_enabled: false };

    expect(syncNativeAgentSessions([], [relay, ended, remoteDisabled])).toEqual([]);
  });

  it("does not duplicate a tab whose root pane already uses the session id", () => {
    const synced = syncNativeAgentSessions([baseTab], [nativeSession]);

    expect(synced).toHaveLength(1);
    expect(synced[0].id).toBe(baseTab.id);
    expect(synced[0].rootNode.type).toBe("leaf");
    if (synced[0].rootNode.type === "leaf") {
      expect(synced[0].rootNode.agentNid).toBe("s_native123");
      expect(synced[0].rootNode.agentRemoteEnabled).toBe(true);
    }
  });

  it("updates existing tab remote-enabled state from agent session state", () => {
    const staleTab: TabSession = { ...baseTab, agentRemoteEnabled: false };
    const synced = syncNativeAgentSessions([staleTab], [nativeSession]);

    expect(synced).toHaveLength(1);
    expect(synced[0].agentRemoteEnabled).toBe(true);
  });

  it("only treats running remote-enabled Native sessions as initial visible sessions", () => {
    const dismissed = new Set<string>();
    const sessions: AgentSession[] = [
      { ...nativeSession, nid: "s_local", remote_enabled: false },
      { ...nativeSession, nid: "s_relay", kind: "Relay" },
      { ...nativeSession, nid: "s_ended", status: "ended" },
      { ...nativeSession, nid: "s_remote" },
    ];

    const visible = filterInitialVisibleAgentSessions(
      sessions,
      Date.parse(nativeSession.created_at) + 1,
      dismissed,
    );

    expect(visible.map((s) => s.nid)).toEqual(["s_remote"]);
  });
});
