import React, { useEffect, useLayoutEffect, useRef, useCallback, useImperativeHandle, forwardRef } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
// import { WebglAddon } from "@xterm/addon-webgl";  // uncomment to re-enable WebGL
import { SearchAddon } from "@xterm/addon-search";
import { getThemeByName, type TerminalTheme } from "../lib/terminalThemes";
import "@xterm/xterm/css/xterm.css";

export interface XTermHandle {
  fit: () => void;
  focus: () => void;
  getSearchAddon: () => SearchAddon | null;
}

interface XTermProps {
  onTerminal: (term: Terminal, fitAddon: FitAddon) => void;
  onReady?: () => void;
  onResize?: (cols: number, rows: number) => void;
  fontSize?: number;
  className?: string;
  themeName?: string;
}

export const XTerm = forwardRef<XTermHandle, XTermProps>(function XTerm({ onTerminal, onReady, onResize, fontSize = 13, className = "", themeName }, ref) {
  const containerRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const searchAddonRef = useRef<SearchAddon | null>(null);
  const initRef = useRef(false);
  const readyFiredRef = useRef(false);

  // Store onResize and onReady in refs so that `fit` doesn't depend on them.
  // Parents create new arrow functions on every render (inline in JSX),
  // which would otherwise cause fit → ResizeObserver teardown/recreate cycles,
  // making the observer miss resize events during drag/maximize transitions.
  const onResizeRef = useRef(onResize);
  onResizeRef.current = onResize;
  const onReadyRef = useRef(onReady);
  onReadyRef.current = onReady;

  const rafRef = useRef<number | null>(null);

  const fit = useCallback(() => {
    if (!fitAddonRef.current || !containerRef.current) return;
    const rect = containerRef.current.getBoundingClientRect();
    if (rect.width > 0 && rect.height > 0) {
      try { fitAddonRef.current.fit(); } catch { /* */ }
      // Fire onReady once after first successful fit
      if (!readyFiredRef.current && onReadyRef.current) {
        readyFiredRef.current = true;
        onReadyRef.current();
      }
      // Notify PTY immediately — same frame, like Tabby / Terminal.app.
      if (onResizeRef.current && termRef.current) {
        const cols = termRef.current.cols;
        const rows = termRef.current.rows;
        if (cols > 0 && rows > 0) onResizeRef.current(cols, rows);
      }
    }
  }, []);  // deps via refs — fit stays stable across renders, ResizeObserver never torn down

  useImperativeHandle(ref, () => ({
    fit,
    focus: () => termRef.current?.focus(),
    getSearchAddon: () => searchAddonRef.current,
  }), [fit]);

  // Initial mount
  useEffect(() => {
    if (!containerRef.current || initRef.current) return;
    initRef.current = true;

    const initialTheme = getThemeByName(themeName || "default-dark");

    const term = new Terminal({
      cursorBlink: true,
      cursorStyle: "bar",
      fontSize,
      fontWeight: "500",
      fontFamily: 'ui-monospace, "Cascadia Code", "Sarasa Mono SC", "Noto Sans Mono CJK SC", "SF Mono", Menlo, Monaco, "JetBrains Mono", "PingFang SC", "Microsoft YaHei", Consolas, "Courier New", monospace',
      letterSpacing: 0,
      lineHeight: 1.2,
      allowTransparency: true,
      scrollback: 5000,
      drawBoldTextInBrightColors: true,
      theme: initialTheme,
    });

    const fitAddon = new FitAddon();
    term.loadAddon(fitAddon);

    // Use canvas renderer (default) for maximum stability.
    // WebGL addon can cause rendering glitches with CJK fonts, box-drawing
    // characters, and Braille spinners — all heavily used by Claude Code's TUI.
    // The canvas renderer supports the same Unicode characters and is fast
    // enough for all practical terminal throughput.
    //
    // To re-enable WebGL for performance, uncomment the block below.
    // Make sure to handle WebGL context loss events for graceful fallback.
    // try {
    //   const webglAddon = new WebglAddon();
    //   term.loadAddon(webglAddon);
    // } catch {
    //   // WebGL not available, fall back to default canvas renderer
    // }

    // Search addon — for Cmd/Ctrl+F terminal search
    const searchAddon = new SearchAddon();
    term.loadAddon(searchAddon);
    searchAddonRef.current = searchAddon;

    term.open(containerRef.current);

    termRef.current = term;
    fitAddonRef.current = fitAddon;
    onTerminal(term, fitAddon);

    // Fit after render
    [60, 150, 400].forEach((d) => setTimeout(() => fit(), d));

    return () => {
      if (rafRef.current) cancelAnimationFrame(rafRef.current);
      term.dispose();
      termRef.current = null;
      fitAddonRef.current = null;
      initRef.current = false;
    };
  }, []);

  // Resize: ResizeObserver + window resize — RAF coalesced, no artificial delay.
  // fit() runs in the same animation frame as the resize event. xterm.js does
  // NOT reflow alternate screen buffer content. SIGWINCH is sent immediately
  // after fit() — Claude redraws at the new dimensions on its next frame.
  // The `refresh(0, rows-1)` that was here before has been removed — it forcibly
  // re-rendered every row, destroying alternate-buffer escape sequences.
  useEffect(() => {
    if (!containerRef.current) return;
    let pending = false;
    const scheduleFit = () => {
      if (pending) return;
      pending = true;
      rafRef.current = requestAnimationFrame(() => {
        pending = false;
        fit();
      });
    };
    const ro = new ResizeObserver(() => scheduleFit());
    ro.observe(containerRef.current);
    window.addEventListener("resize", scheduleFit);
    return () => {
      ro.disconnect();
      window.removeEventListener("resize", scheduleFit);
      if (rafRef.current) cancelAnimationFrame(rafRef.current);
    };
  }, [fit]);

  // Apply theme changes (from selector or external prop)
  useEffect(() => {
    const t = getThemeByName(themeName || "default-dark");
    if (termRef.current) {
      termRef.current.options.theme = t;
    }
  }, [themeName]);

  // Update the existing terminal in place when font size changes.
  // Recreating the xterm instance clears the visible buffer and drops
  // interactive TUI context, which is why Codex/Claude screens blank out.
  useEffect(() => {
    if (!termRef.current) return;
    termRef.current.options.fontSize = fontSize;
    fit();
  }, [fontSize, fit]);

  const themeBg = getThemeByName(themeName || "default-dark").background;

  return (
    <div
      ref={containerRef}
      className={className}
      style={{
        width: "100%",
        height: "100%",
        minWidth: 0,
        minHeight: 0,
        overflow: "hidden",
        padding: 0,
        margin: 0,
        background: themeBg,
      }}
    />
  );
});
