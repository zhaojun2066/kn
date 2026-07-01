import type { AgentSession } from "../useAgent";
import { createInitialLeaf, flattenPanes } from "../../lib/pane-types";
import type { TabSession } from "./types";
import { syncActivePaneFields } from "./helpers";

let syncedTabCounter = 1;

export function isRunningNativeSession(session: AgentSession): boolean {
  return session.kind === "Native" && session.status === "running";
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
  });
}

export function syncNativeAgentSessions(
  tabs: TabSession[],
  sessions: AgentSession[],
): TabSession[] {
  const additions = sessions
    .filter(isRunningNativeSession)
    .filter((session) => !hasAgentSessionTab(tabs, session.nid))
    .map(tabFromAgentSession);

  return additions.length > 0 ? [...tabs, ...additions] : tabs;
}
