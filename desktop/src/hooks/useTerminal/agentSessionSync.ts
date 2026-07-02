import type { AgentSession } from "../useAgent";
import { createInitialLeaf, flattenPanes, replaceNode, type PaneLeaf } from "../../lib/pane-types";
import type { TabSession } from "./types";
import { syncActivePaneFields } from "./helpers";

let syncedTabCounter = 1;

export function isRunningNativeSession(session: AgentSession): boolean {
  return session.kind === "Native" && session.status === "running" && session.remote_enabled;
}

export function hasAgentSessionTab(tabs: TabSession[], nid: string): boolean {
  return tabs.some((tab) =>
    tab.agentNid === nid ||
    tab.sessionId === nid ||
    flattenPanes(tab.rootNode).some((leaf) => leaf.sessionId === nid)
  );
}

function tabFromAgentSession(session: AgentSession): TabSession {
  const name = `${session.tool} · 本地`;
  const id = `agent-tab-${syncedTabCounter++}-${session.nid}`;
  const leaf = {
    ...createInitialLeaf(name, session.cwd, session.nid),
    ptyRunning: false,
    agentNid: session.nid,
    agentRemoteEnabled: session.remote_enabled,
  };

  return syncActivePaneFields({
    id,
    name,
    workDir: session.cwd,
    rootNode: leaf,
    activePaneId: leaf.paneId,
    zoomedPaneId: null,
    sessionId: session.nid,
    ptyRunning: false,
    agentNid: session.nid,
    agentRemoteEnabled: session.remote_enabled,
  });
}

export function syncAgentSessionState(
  tabs: TabSession[],
  sessions: AgentSession[],
): TabSession[] {
  const remoteByNid = new Map(sessions.map((session) => [session.nid, session.remote_enabled]));
  return tabs.map((tab) => {
    let next = tab;
    const tabRemoteEnabled =
      tab.agentNid && remoteByNid.has(tab.agentNid) ? remoteByNid.get(tab.agentNid) : undefined;
    if (tabRemoteEnabled !== undefined && tab.agentRemoteEnabled !== tabRemoteEnabled) {
      next = { ...next, agentRemoteEnabled: tabRemoteEnabled };
    }

    for (const leaf of flattenPanes(next.rootNode)) {
      const leafAgentNid = leaf.agentNid ?? (leaf.sessionId.startsWith("s_") ? leaf.sessionId : undefined);
      if (!leafAgentNid || !remoteByNid.has(leafAgentNid)) continue;
      const agentRemoteEnabled = remoteByNid.get(leafAgentNid);
      if (leaf.agentNid === leafAgentNid && leaf.agentRemoteEnabled === agentRemoteEnabled) continue;
      const updatedLeaf: PaneLeaf = { ...leaf, agentNid: leafAgentNid, agentRemoteEnabled };
      next = syncActivePaneFields({
        ...next,
        rootNode: replaceNode(next.rootNode, leaf.paneId, updatedLeaf),
      });
    }

    return next;
  });
}

export function syncNativeAgentSessions(
  tabs: TabSession[],
  sessions: AgentSession[],
): TabSession[] {
  const updatedTabs = syncAgentSessionState(tabs, sessions);
  const additions = sessions
    .filter(isRunningNativeSession)
    .filter((session) => !hasAgentSessionTab(updatedTabs, session.nid))
    .map(tabFromAgentSession);

  return additions.length > 0 ? [...updatedTabs, ...additions] : updatedTabs;
}

export function filterInitialVisibleAgentSessions(
  sessions: AgentSession[],
  startupCutoffMs: number,
  dismissedAgentNids: Set<string>,
): AgentSession[] {
  return sessions.filter((session) => {
    const createdAt = Date.parse(session.created_at);
    return isRunningNativeSession(session) &&
      Number.isFinite(createdAt) &&
      createdAt <= startupCutoffMs &&
      !dismissedAgentNids.has(session.nid);
  });
}
