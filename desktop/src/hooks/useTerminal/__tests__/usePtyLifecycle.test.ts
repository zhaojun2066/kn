/// <reference types="vitest" />
import { describe, expect, it } from "vitest";
import type { PaneLeaf } from "../../../lib/pane-types";
import type { TabSession } from "../types";
import { getPtyExitAgentNotification } from "../usePtyLifecycle";

const pane: PaneLeaf = {
  type: "leaf",
  paneId: "pane-1",
  sessionId: "pty-local",
  name: "Claude",
  ptyRunning: true,
  workDir: "/tmp/project",
};

function tab(agentNid?: string, sessionId = "pty-local"): TabSession {
  const leaf = { ...pane, sessionId };
  return {
    id: "tab-1",
    name: "Claude",
    workDir: "/tmp/project",
    rootNode: leaf,
    activePaneId: leaf.paneId,
    zoomedPaneId: null,
    sessionId,
    ptyRunning: true,
    agentNid,
  };
}

function relaySplitTab(): TabSession {
  return {
    ...tab("s_relay", "pty-relay"),
    rootNode: {
      type: "split",
      id: "split-1",
      direction: "horizontal",
      ratio: 0.5,
      children: [
        { ...pane, paneId: "pane-relay", sessionId: "pty-relay", agentNid: "s_relay", agentRemoteEnabled: true },
        { ...pane, paneId: "pane-local", sessionId: "pty-local" },
      ],
    },
    activePaneId: "pane-relay",
  };
}

describe("PTY exit agent notification", () => {
  it("does not notify agent for pure local PTY exits", () => {
    expect(getPtyExitAgentNotification(tab(), pane)).toBeNull();
  });

  it("does not notify agent for agent-owned attached PTY views", () => {
    const attachedPane = { ...pane, sessionId: "s_native" };

    expect(getPtyExitAgentNotification(tab("s_native", "s_native"), attachedPane)).toBeNull();
  });

  it("uses relay_exit for desktop-owned Relay PTY exits", () => {
    expect(getPtyExitAgentNotification(tab("s_relay"), { ...pane, agentNid: "s_relay" })).toEqual({
      method: "relay_exit",
      params: { nid: "s_relay", reason: "process_exit" },
    });
  });

  it("does not end a Relay session when a sibling local split pane exits", () => {
    const localPane = { ...pane, paneId: "pane-local", sessionId: "pty-local" };

    expect(getPtyExitAgentNotification(relaySplitTab(), localPane)).toBeNull();
  });
});
