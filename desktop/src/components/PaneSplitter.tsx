import React, { useState, useRef, useCallback, useEffect } from "react";
import type { Terminal } from "@xterm/xterm";
import type { PaneNode, PaneSplit as PaneSplitData, SplitDirection } from "../lib/pane-types";
import { isLeaf, flattenPanes } from "../lib/pane-types";
import { PaneTerminal } from "./PaneTerminal";
import type { XTermHandle } from "./XTerm";

interface PaneSplitterProps {
  node: PaneNode;
  activePaneId: string;
  zoomedPaneId: string | null;
  fontSize?: number;
  themeName?: string;
  onAttach: (paneId: string, term: Terminal) => void;
  onReady?: (paneId: string) => void;
  onResize?: (paneId: string, cols: number, rows: number) => void;
  onFocus: (paneId: string) => void;
  onXTermHandle?: (paneId: string, handle: XTermHandle | null) => void;
  onPaneContextMenu?: (paneId: string, e: React.MouseEvent) => void;
}

// ── Layout types ──────────────────────────────────────────────────

interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

interface PaneLayout {
  paneId: string;
  rect: Rect;
}

interface DividerLayout {
  splitId: string;
  rect: Rect;
  direction: SplitDirection;
}

interface TreeLayout {
  panes: PaneLayout[];
  dividers: DividerLayout[];
}

// ── Layout computation ───────────────────────────────────────────

const DIVIDER_SIZE = 4;
const MIN_RATIO = 0.15;
const MAX_RATIO = 0.85;
const SNAP_THRESHOLD = 0.03;

function computeLayout(
  node: PaneNode,
  bounds: Rect,
  ratios: Map<string, number>,
): TreeLayout {
  if (isLeaf(node)) {
    return {
      panes: [{ paneId: node.paneId, rect: bounds }],
      dividers: [],
    };
  }

  const ratio = ratios.get(node.id) ?? node.ratio;
  const clamped = Math.max(MIN_RATIO, Math.min(MAX_RATIO, ratio));
  const isH = node.direction === "horizontal";

  // Child 0 gets `clamped` fraction of space
  const child0Rect: Rect = isH
    ? { x: bounds.x, y: bounds.y, w: bounds.w * clamped, h: bounds.h }
    : { x: bounds.x, y: bounds.y, w: bounds.w, h: bounds.h * clamped };

  // Divider
  const dividerRect: Rect = isH
    ? { x: child0Rect.x + child0Rect.w, y: bounds.y, w: DIVIDER_SIZE, h: bounds.h }
    : { x: bounds.x, y: child0Rect.y + child0Rect.h, w: bounds.w, h: DIVIDER_SIZE };

  // Child 1 gets the remainder
  const child1Rect: Rect = isH
    ? {
        x: dividerRect.x + DIVIDER_SIZE,
        y: bounds.y,
        w: bounds.w - child0Rect.w - DIVIDER_SIZE,
        h: bounds.h,
      }
    : {
        x: bounds.x,
        y: dividerRect.y + DIVIDER_SIZE,
        w: bounds.w,
        h: bounds.h - child0Rect.h - DIVIDER_SIZE,
      };

  const left = computeLayout(node.children[0], child0Rect, ratios);
  const right = computeLayout(node.children[1], child1Rect, ratios);

  return {
    panes: [...left.panes, ...right.panes],
    dividers: [
      { splitId: node.id, rect: dividerRect, direction: node.direction },
      ...left.dividers,
      ...right.dividers,
    ],
  };
}

/**
 * Pane tree renderer — all PaneTerminal components are rendered as
 * siblings so React preserves their DOM nodes (and XTerm instances)
 * when the tree structure changes.
 */
export function PaneSplitter({
  node,
  activePaneId,
  zoomedPaneId,
  fontSize,
  themeName,
  onAttach,
  onReady,
  onResize,
  onFocus,
  onXTermHandle,
  onPaneContextMenu,
}: PaneSplitterProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [containerW, setContainerW] = useState(0);
  const [containerH, setContainerH] = useState(0);

  // Per-split local ratio overrides (from divider dragging)
  const [ratios, setRatios] = useState<Map<string, number>>(new Map());

  // ResizeObserver for container dimensions
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const ro = new ResizeObserver((entries) => {
      const r = entries[0]?.contentRect;
      if (r) {
        setContainerW(r.width);
        setContainerH(r.height);
      }
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // Compute flat layout from tree + container size + local ratios
  const layout =
    containerW > 0 && containerH > 0
      ? computeLayout(node, { x: 0, y: 0, w: containerW, h: containerH }, ratios)
      : null;

  // ── Divider drag ──────────────────────────────────────────────

  const handleDividerMouseDown = useCallback(
    (splitId: string, direction: SplitDirection, e: React.MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();

      const isH = direction === "horizontal";
      const startPos = isH ? e.clientX : e.clientY;
      const currentRatio =
        ratios.get(splitId) ??
        (() => {
          // Walk tree to find the original ratio (fallback)
          return 0.5;
        })();

      // Actually, we need the original ratio from the tree. Since we only
      // override ratios that have been dragged, the fallback is the tree's
      // own ratio for this split. We compute it from the current layout.
      const startRatio = currentRatio;

      const onMouseMove = (ev: MouseEvent) => {
        if (!containerRef.current) return;
        const currentPos = isH ? ev.clientX : ev.clientY;
        const delta = currentPos - startPos;
        const containerSize = isH
          ? containerRef.current.offsetWidth
          : containerRef.current.offsetHeight;
        if (containerSize <= 0) return;

        let newRatio = startRatio + delta / containerSize;

        // Clamp
        newRatio = Math.max(MIN_RATIO, Math.min(MAX_RATIO, newRatio));

        // Snap to 50% within ±3%
        if (Math.abs(newRatio - 0.5) < SNAP_THRESHOLD) {
          newRatio = 0.5;
        }

        setRatios((prev) => {
          const next = new Map(prev);
          next.set(splitId, newRatio);
          return next;
        });
      };

      const onMouseUp = () => {
        document.removeEventListener("mousemove", onMouseMove);
        document.removeEventListener("mouseup", onMouseUp);
      };

      document.addEventListener("mousemove", onMouseMove);
      document.addEventListener("mouseup", onMouseUp);
    },
    [ratios],
  );

  // ── Zoom mode ──────────────────────────────────────────────────

  // Collect all leaves from the tree
  const allLeaves = flattenPanes(node);

  return (
    <div ref={containerRef} className="relative w-full h-full overflow-hidden">
      {/* Panes — rendered as absolutely positioned siblings with stable keys */}
      {allLeaves.map((leaf) => {
        const hidden = zoomedPaneId !== null && leaf.paneId !== zoomedPaneId;
        // When zoomed, the zoomed pane fills the entire container
        const isZoomed = zoomedPaneId === leaf.paneId;
        const rect = isZoomed
          ? { x: 0, y: 0, w: containerW, h: containerH }
          : layout?.panes.find((p) => p.paneId === leaf.paneId)?.rect;

        return (
          <div
            key={leaf.paneId}
            className="absolute"
            style={{
              display: hidden ? "none" : undefined,
              left: rect?.x ?? 0,
              top: rect?.y ?? 0,
              width: rect?.w ?? 0,
              height: rect?.h ?? 0,
              visibility: rect ? "visible" : "hidden",
            }}
          >
            <PaneTerminal
              pane={leaf}
              active={leaf.paneId === activePaneId}
              fontSize={fontSize}
              themeName={themeName}
              onAttach={onAttach}
              onReady={onReady}
              onResize={onResize}
              onFocus={onFocus}
              onXTermHandle={onXTermHandle}
              onContextMenu={onPaneContextMenu}
            />
          </div>
        );
      })}

      {/* Dividers — overlaid on top of panes */}
      {!zoomedPaneId &&
        layout?.dividers.map((d) => {
          const isH = d.direction === "horizontal";
          const currentRatio = ratios.get(d.splitId);
          const isDragging = currentRatio !== undefined && currentRatio !== 0.5; // heuristic

          return (
            <div
              key={d.splitId}
              className={`absolute shrink-0 transition-colors duration-150 group/divider flex items-center justify-center
                ${isDragging ? "bg-[var(--app-accent)]/40 z-10" : "hover:bg-[var(--app-accent)]/20"}
                ${isH ? "cursor-col-resize" : "cursor-row-resize"}`}
              style={{
                left: d.rect.x,
                top: d.rect.y,
                width: d.rect.w,
                height: d.rect.h,
                touchAction: "none",
              }}
              onMouseDown={(e) => handleDividerMouseDown(d.splitId, d.direction, e)}
            >
              <div
                className={`shrink-0 bg-app-border group-hover/divider:bg-[var(--app-accent)]/50 transition-colors duration-150
                  ${isDragging ? "bg-[var(--app-accent)]" : ""}
                  ${isH ? "w-px h-full" : "h-px w-full"}`}
              />
            </div>
          );
        })}
    </div>
  );
}
