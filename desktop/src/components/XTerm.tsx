import React, { useEffect, useLayoutEffect, useRef, useCallback, useImperativeHandle, forwardRef } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
// import { WebglAddon } from "@xterm/addon-webgl";  // uncomment to re-enable WebGL
import { SearchAddon } from "@xterm/addon-search";
import { getThemeByName, type TerminalTheme } from "../lib/terminalThemes";
import "@xterm/xterm/css/xterm.css";

export interface XTermHandle {
  fit: () => void;
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

  const rafRef = useRef<number | null>(null);

  const fit = useCallback(() => {
    if (!fitAddonRef.current || !containerRef.current) return;
    const rect = containerRef.current.getBoundingClientRect();
    if (rect.width > 0 && rect.height > 0) {
      try { fitAddonRef.current.fit(); } catch { /* */ }
      // Force full viewport re-render after resize.
      // Without this, shrink→expand cycles can leave the cursor/input positioned
      // below the visible TUI area — the canvas resizes but doesn't fully sync.
      termRef.current?.refresh(0, termRef.current.rows - 1);
      // Fire onReady once after first successful fit
      if (!readyFiredRef.current && onReady) {
        readyFiredRef.current = true;
        onReady();
      }
      // Notify PTY immediately — same frame, like Tabby / Terminal.app.
      // The kernel handles coalescing of frequent TIOCSWINSZ calls during rapid resize.
      if (onResize && termRef.current) {
        const cols = termRef.current.cols;
        const rows = termRef.current.rows;
        if (cols > 0 && rows > 0) onResize(cols, rows);
      }
    }
  }, [onReady, onResize]);

  useImperativeHandle(ref, () => ({ fit, getSearchAddon: () => searchAddonRef.current }), [fit]);

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
      fontFamily: 'ui-monospace, "SF Mono", "Cascadia Code", Menlo, Monaco, "JetBrains Mono", "PingFang SC", "Microsoft YaHei", "Noto Sans CJK SC", Consolas, "Courier New", monospace',
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
