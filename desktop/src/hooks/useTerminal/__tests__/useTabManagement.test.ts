/// <reference types="vitest" />
import { describe, expect, it } from "vitest";
import type { TabSession } from "../types";
import { collectPaneCloseKills, collectTerminalCloseKills } from "../useTabManagement";
import { findAgentSessionPane } from "../useSessionCommands";

function makeTab(sessionId: string, agentNid?: string, agentRemoteEnabled?: boolean): TabSession {
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
    agentRemoteEnabled,
  };
}

function makeSplitTab(): TabSession {
  const remoteLeaf = {
    type: "leaf" as const,
    paneId: "pane-remote",
    sessionId: "s_remote",
    name: "Remote",
    ptyRunning: true,
    workDir: "/tmp/project",
  };
  const localLeaf = {
    type: "leaf" as const,
    paneId: "pane-local",
    sessionId: "pty-local",
    name: "Local",
    ptyRunning: true,
    workDir: "/tmp/project",
  };
  return {
    id: "tab-split",
    name: "Split",
    workDir: "/tmp/project",
    rootNode: {
      type: "split",
      id: "split-1",
      direction: "horizontal",
      ratio: 0.5,
      children: [remoteLeaf, localLeaf],
    },
    activePaneId: "pane-remote",
    zoomedPaneId: null,
    sessionId: "s_remote",
    ptyRunning: true,
    agentNid: "s_remote",
    agentRemoteEnabled: true,
  };
}

function makeRelaySplitTab(): TabSession {
  const relayLeaf = {
    type: "leaf" as const,
    paneId: "pane-relay",
    sessionId: "pty-relay",
    name: "Relay",
    ptyRunning: true,
    workDir: "/tmp/project",
    agentNid: "s_relay",
    agentRemoteEnabled: true,
  };
  const localLeaf = {
    type: "leaf" as const,
    paneId: "pane-local",
    sessionId: "pty-local",
    name: "Local",
    ptyRunning: true,
    workDir: "/tmp/project",
  };
  return {
    id: "tab-relay-split",
    name: "Relay Split",
    workDir: "/tmp/project",
    rootNode: {
      type: "split",
      id: "split-relay",
      direction: "horizontal",
      ratio: 0.5,
      children: [relayLeaf, localLeaf],
    },
    activePaneId: "pane-relay",
    zoomedPaneId: null,
    sessionId: "pty-relay",
    ptyRunning: true,
    agentNid: "s_relay",
    agentRemoteEnabled: true,
  };
}

describe("terminal tab close kill targets", () => {
  it("detaches agent-owned s_ sessions instead of killing them", () => {
    const targets = collectTerminalCloseKills(makeTab("s_remote", "s_remote"));

    expect(targets.agentNids).toEqual([]);
    expect(targets.ptySessionIds).toEqual([]);
    expect(targets.relayExitNids).toEqual([]);
  });

  it("does not kill agent sessions even when tab and pane ids both reference the same nid", () => {
    const targets = collectTerminalCloseKills(makeTab("s_same", "s_same"));

    expect(targets.agentNids).toEqual([]);
    expect(targets.relayExitNids).toEqual([]);
  });

  it("kills plain local PTY panes through kill_pty", () => {
    const targets = collectTerminalCloseKills(makeTab("pty-local"));

    expect(targets.agentNids).toEqual([]);
    expect(targets.ptySessionIds).toEqual(["pty-local"]);
    expect(targets.relayExitNids).toEqual([]);
  });

  it("kills remote-disabled Relay/local PTY panes through local tab semantics", () => {
    const targets = collectTerminalCloseKills(makeTab("pty-local", "s_relay", false));

    expect(targets.agentNids).toEqual([]);
    expect(targets.ptySessionIds).toEqual(["pty-local"]);
    expect(targets.relayExitNids).toEqual(["s_relay"]);
  });

  it("ends remote-enabled Relay tabs when the local tab is closed", () => {
    const targets = collectTerminalCloseKills(makeTab("pty-local", "s_relay", true));

    expect(targets.agentNids).toEqual([]);
    expect(targets.ptySessionIds).toEqual(["pty-local"]);
    expect(targets.relayExitNids).toEqual(["s_relay"]);
  });

  it("still kills local split panes when the same tab also contains a remote pane", () => {
    const targets = collectTerminalCloseKills(makeSplitTab());

    expect(targets.agentNids).toEqual([]);
    expect(targets.ptySessionIds).toEqual(["pty-local"]);
    expect(targets.relayExitNids).toEqual([]);
  });

  it("detaches a single remote pane without killing it", () => {
    const tab = makeSplitTab();
    const targets = collectPaneCloseKills(tab, "pane-remote");

    expect(targets.agentNids).toEqual([]);
    expect(targets.ptySessionIds).toEqual([]);
    expect(targets.relayExitNids).toEqual([]);
  });

  it("kills a single local split pane next to a remote pane", () => {
    const tab = makeSplitTab();
    const targets = collectPaneCloseKills(tab, "pane-local");

    expect(targets.agentNids).toEqual([]);
    expect(targets.ptySessionIds).toEqual(["pty-local"]);
    expect(targets.relayExitNids).toEqual([]);
  });

  it("ends a remote-enabled Relay pane while killing a sibling local pane", () => {
    const targets = collectTerminalCloseKills(makeRelaySplitTab());

    expect(targets.agentNids).toEqual([]);
    expect(targets.ptySessionIds).toEqual(["pty-relay", "pty-local"]);
    expect(targets.relayExitNids).toEqual(["s_relay"]);
  });

  it("finds the Relay pane rather than the active sibling when returning to a split terminal", () => {
    const relayPane = findAgentSessionPane(makeRelaySplitTab(), "s_relay");

    expect(relayPane?.paneId).toBe("pane-relay");
  });
});
