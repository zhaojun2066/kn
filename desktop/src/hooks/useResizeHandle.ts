import { useState, useCallback, useRef, useEffect } from "react";

export interface UseResizeHandleOptions {
  /** "horizontal" = left/right drag; "vertical" = up/down drag */
  direction: "horizontal" | "vertical";
  /** Minimum size in pixels */
  minSize: number;
  /** Maximum size in pixels */
  maxSize: number;
  /** Default size (used on first mount or when localStorage is empty) */
  defaultSize: number;
  /** localStorage key for persistence */
  storageKey: string;
  /**
   * External size state. When provided, the hook acts as a controlled adapter —
   * it reads the latest size via a ref (no useCallback dependency on size).
   * When omitted, the hook manages size internally via useState.
   */
  externalSize?: number;
  /** Required when externalSize is provided. */
  onExternalSizeChange?: (size: number) => void;
}

export interface UseResizeHandleReturn {
  /** Current size (px). Bind this to the panel's width/height style. */
  size: number;
  /**
   * Props for the resize handle <div>.
   * Spread onto the handle element: <div {...handleProps} />
   * Includes: onMouseDown, className (with select-none + cursor), style.
   */
  handleProps: {
    onMouseDown: (e: React.MouseEvent) => void;
    className: string;
    style: React.CSSProperties;
  };
}

/**
 * Stable resize-drag hook.
 *
 * Uses a ref for the current size so that the mousedown callback never
 * depends on changing state — it stays stable across renders.  This
 * eliminates the fragility that caused resize to break after refactors.
 *
 * The hook works in two modes:
 * 1. Internal state (drawers): omit externalSize / onExternalSizeChange.
 * 2. External state (terminal panels): pass externalSize + onExternalSizeChange
 *    to delegate size storage to an outside hook (e.g. useTerminal).
 */
export function useResizeHandle(opts: UseResizeHandleOptions): UseResizeHandleReturn {
  const {
    direction,
    minSize,
    maxSize,
    defaultSize,
    storageKey,
    externalSize,
    onExternalSizeChange,
  } = opts;
  const isHorizontal = direction === "horizontal";

  // ── Internal state (only used when externalSize is not provided) ──
  const [internalSize, setInternalSize] = useState(() => {
    try {
      const saved = localStorage.getItem(storageKey);
      if (saved) return clamp(parseInt(saved, 10), minSize, maxSize);
    } catch { /* */ }
    return clamp(defaultSize, minSize, maxSize);
  });

  const size = externalSize !== undefined ? externalSize : internalSize;

  // ── Stable ref so the callback never depends on size ──
  const sizeRef = useRef(size);
  sizeRef.current = size;

  // Also keep min/max in refs so the callback is truly stable.
  const minRef = useRef(minSize);
  minRef.current = minSize;
  const maxRef = useRef(maxSize);
  maxRef.current = maxSize;

  // ── Persist on unmount / when size changes ──
  useEffect(() => {
    try { localStorage.setItem(storageKey, String(size)); } catch { /* */ }
  }, [size, storageKey]);

  // ── The one stable resize handler ──
  const handleMouseDown = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      e.stopPropagation(); // prevent backdrop onClick from firing

      const startPos = isHorizontal ? e.clientX : e.clientY;
      const startSize = sizeRef.current;

      const onMouseMove = (ev: MouseEvent) => {
        ev.preventDefault();
        const currentPos = isHorizontal ? ev.clientX : ev.clientY;
        const delta = startPos - currentPos;
        const newSize = clamp(startSize + delta, minRef.current, maxRef.current);

        // Update internal or external state
        if (onExternalSizeChange) {
          onExternalSizeChange(newSize);
        } else {
          setInternalSize(newSize);
        }
      };

      const onMouseUp = () => {
        document.removeEventListener("mousemove", onMouseMove);
        document.removeEventListener("mouseup", onMouseUp);
        document.body.style.cursor = "";
        document.body.style.userSelect = "";
        try { localStorage.setItem(storageKey, String(sizeRef.current)); } catch { /* */ }
      };

      // Lock cursor + prevent text selection while dragging
      document.body.style.cursor = isHorizontal ? "col-resize" : "row-resize";
      document.body.style.userSelect = "none";
      document.addEventListener("mousemove", onMouseMove);
      document.addEventListener("mouseup", onMouseUp);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [isHorizontal, storageKey, onExternalSizeChange],
  );

  const cursorClass = isHorizontal ? "cursor-col-resize" : "cursor-row-resize";

  return {
    size,
    handleProps: {
      onMouseDown: handleMouseDown,
      className: `select-none ${cursorClass}`,
      style: {},
    },
  };
}

function clamp(val: number, min: number, max: number): number {
  return Math.min(Math.max(val, min), max);
}
