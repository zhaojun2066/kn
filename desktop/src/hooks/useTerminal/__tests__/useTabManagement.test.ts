/// <reference types="vitest" />
import { describe, expect, it } from "vitest";
import type { TabSession } from "../types";
import { collectTerminalCloseKills } from "../useTabManagement";

function makeTab(sessionId: string, agentNid?: string): TabSession {
  return {
    id: `tab-${sessionId}`,
    name: "Claude",
    workDir: "/tmp/project",
    rootNode: {
      type: "leaf",
      paneId: `pane-${sessionId}`,
      sessionId,
      name: "Claude",
      ptyRunning: true,
      workDir: "/tmp/project",
    },
    activePaneId: `pane-${sessionId}`,
    zoomedPaneId: null,
    sessionId,
    ptyRunning: true,
    agentNid,
  };
}

describe("terminal tab close kill targets", () => {
  it("kills agent-owned s_ sessions instead of only dismissing the tab", () => {
    const targets = collectTerminalCloseKills(makeTab("s_remote", "s_remote"));

    expect(targets.agentNids).toEqual(["s_remote"]);
    expect(targets.ptySessionIds).toEqual([]);
  });

  it("deduplicates agent kill targets from tab and pane ids", () => {
    const targets = collectTerminalCloseKills(makeTab("s_same", "s_same"));

    expect(targets.agentNids).toEqual(["s_same"]);
  });

  it("kills plain local PTY panes through kill_pty", () => {
    const targets = collectTerminalCloseKills(makeTab("pty-local"));

    expect(targets.agentNids).toEqual([]);
    expect(targets.ptySessionIds).toEqual(["pty-local"]);
  });

  it("kills both the local PTY and Relay agent record when closing a desktop Run tab", () => {
    const targets = collectTerminalCloseKills(makeTab("pty-local", "s_relay"));

    expect(targets.agentNids).toEqual(["s_relay"]);
    expect(targets.ptySessionIds).toEqual(["pty-local"]);
  });
});
