/**
 * Terminal split-pane data model: a binary tree where:
 *   - PaneLeaf  = a running terminal pane (one PTY session)
 *   - PaneSplit = a container with two children and a draggable divider
 *
 * Pure utility functions for tree traversal and manipulation.
 * Zero runtime dependencies — no React, no Tauri IPC.
 */

export type SplitDirection = "horizontal" | "vertical";
export type NavDirection = "up" | "down" | "left" | "right";

export interface PaneLeaf {
  type: "leaf";
  paneId: string;
  sessionId: string; // PTY session ID, 1:1 with a pane
  name: string;
  ptyRunning: boolean;
  workDir: string;
}

export interface PaneSplit {
  type: "split";
  id: string; // stable identity for replaceNode operations
  direction: SplitDirection;
  ratio: number; // 0.0–1.0, fraction given to children[0]
  children: [PaneNode, PaneNode];
}

export type PaneNode = PaneLeaf | PaneSplit;

// ── Type guards ─────────────────────────────────────────────────

export function isLeaf(node: PaneNode): node is PaneLeaf {
  return node.type === "leaf";
}

export function isSplit(node: PaneNode): node is PaneSplit {
  return node.type === "split";
}

// ── Tree traversal ──────────────────────────────────────────────

/** In-order traversal: collect all leaves */
export function flattenPanes(node: PaneNode): PaneLeaf[] {
  if (isLeaf(node)) return [node];
  return [...flattenPanes(node.children[0]), ...flattenPanes(node.children[1])];
}

/** Find a leaf by paneId */
export function findLeaf(root: PaneNode, paneId: string): PaneLeaf | null {
  if (isLeaf(root)) return root.paneId === paneId ? root : null;
  return findLeaf(root.children[0], paneId) || findLeaf(root.children[1], paneId);
}

/** Find the parent split and child index (0 or 1) for a given node id */
export function findParentSplit(
  root: PaneNode,
  targetId: string,
): { parent: PaneSplit; index: 0 | 1 } | null {
  if (isLeaf(root)) return null;

  for (const i of [0, 1] as const) {
    const child = root.children[i];
    if (isLeaf(child)) {
      if (child.paneId === targetId) return { parent: root, index: i };
    } else {
      if (child.id === targetId) return { parent: root, index: i };
      const found = findParentSplit(child, targetId);
      if (found) return found;
    }
  }
  return null;
}

/** Immutable tree replacement: returns a new tree with targetId replaced */
export function replaceNode(
  root: PaneNode,
  targetId: string,
  replacement: PaneNode,
): PaneNode {
  if (isLeaf(root)) {
    return root.paneId === targetId ? replacement : root;
  }

  // PaneSplit — check if root itself is the target
  if (root.id === targetId) return replacement;

  const newChildren = root.children.map((child) => {
    if (isLeaf(child)) {
      return child.paneId === targetId ? replacement : child;
    }
    if (child.id === targetId) return replacement;
    return replaceNode(child, targetId, replacement);
  }) as [PaneNode, PaneNode];

  return { ...root, children: newChildren };
}

/** Find the first leaf in a tree (used after closePane for focus fallback) */
export function firstLeaf(node: PaneNode): PaneLeaf | null {
  if (isLeaf(node)) return node;
  return firstLeaf(node.children[0]) || firstLeaf(node.children[1]);
}

// ── Navigation ──────────────────────────────────────────────────

/**
 * Cardinal-direction navigation in a binary tree.
 *
 * Flattens leaves, finds current index, wraps around.
 * For nested splits this is a simplification; full geometry-based
 * navigation would need bounding-box computation (deferred to v2).
 */
export function navigateFromLeaf(
  root: PaneNode,
  fromPaneId: string,
  direction: NavDirection,
): PaneLeaf | null {
  const leaves = flattenPanes(root);
  const idx = leaves.findIndex((l) => l.paneId === fromPaneId);
  if (idx < 0 || leaves.length <= 1) return null;

  const nextIdx = (() => {
    switch (direction) {
      case "left":
      case "up":
        return idx > 0 ? idx - 1 : leaves.length - 1;
      case "right":
      case "down":
        return idx < leaves.length - 1 ? idx + 1 : 0;
    }
  })();

  return leaves[nextIdx] || null;
}

// ── Factory ─────────────────────────────────────────────────────

let paneCounter = 1;

export function createInitialLeaf(
  tabName: string,
  workDir: string,
  sessionId: string,
): PaneLeaf {
  return {
    type: "leaf",
    paneId: `pane-${Date.now().toString(36)}-${paneCounter++}`,
    sessionId,
    name: tabName,
    ptyRunning: false,
    workDir,
  };
}

/** Reset pane counter (for tests) */
export function resetPaneCounter(n = 1): void {
  paneCounter = n;
}
