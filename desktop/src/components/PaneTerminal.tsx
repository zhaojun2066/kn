import { useRef, useEffect, useCallback } from "react";
import { XTerm, XTermHandle } from "./XTerm";
import type { Terminal } from "@xterm/xterm";
import type { PaneLeaf } from "../lib/pane-types";

interface PaneTerminalProps {
  pane: PaneLeaf;
  active: boolean;
  fontSize?: number;
  themeName?: string;
  onAttach: (paneId: string, term: Terminal) => void;
  onReady?: (paneId: string) => void;
  onResize?: (paneId: string, cols: number, rows: number) => void;
  onFocus: (paneId: string) => void;
  onXTermHandle?: (paneId: string, handle: XTermHandle | null) => void;
  onContextMenu?: (paneId: string, e: React.MouseEvent) => void;
}

/**
 * Single terminal pane wrapping an XTerm instance.
 * Extracted from TerminalPanel's inline TabTerminal component.
 *
 * The terminal owns its visual focus state.  Keeping the pane wrapper neutral
 * avoids a competing blue frame around right and bottom terminal panels.
 */
export function PaneTerminal({
  pane,
  active,
  fontSize,
  themeName,
  onAttach,
  onReady,
  onResize,
  onFocus,
  onXTermHandle,
  onContextMenu,
}: PaneTerminalProps) {
  const xtermRef = useRef<XTermHandle>(null);
  const attached = useRef(false);

  const handleTerminal = useCallback(
    (term: Terminal) => {
      if (!attached.current) {
        attached.current = true;
        onAttach(pane.paneId, term);
        if (onXTermHandle) onXTermHandle(pane.paneId, xtermRef.current);
      }
    },
    [onAttach, onXTermHandle, pane.paneId],
  );

  useEffect(() => {
    attached.current = false;
    if (onXTermHandle && xtermRef.current) {
      onXTermHandle(pane.paneId, xtermRef.current);
    }
    return () => {
      if (onXTermHandle) onXTermHandle(pane.paneId, null);
    };
  }, [pane.paneId]);

  // Auto-focus the XTerm textarea when this pane becomes active,
  // so keyboard input goes to the right pane after split / navigate.
  useEffect(() => {
    if (active) {
      // Delay to let XTerm finish initializing after first mount
      const timer = setTimeout(() => xtermRef.current?.focus(), 50);
      return () => clearTimeout(timer);
    }
  }, [active]);

  return (
    <div
      className="relative w-full h-full"
      onClick={() => onFocus(pane.paneId)}
      onContextMenu={(e) => {
        e.preventDefault();
        onContextMenu?.(pane.paneId, e);
      }}
      style={{ boxSizing: "border-box" }}
    >
      <XTerm
        ref={xtermRef}
        onTerminal={handleTerminal}
        onReady={onReady ? () => onReady(pane.paneId) : undefined}
        onResize={
          onResize
            ? (cols, rows) => onResize(pane.paneId, cols, rows)
            : undefined
        }
        fontSize={fontSize}
        themeName={themeName}
      />
    </div>
  );
}
