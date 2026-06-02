import React, { useEffect, useLayoutEffect, useRef, useCallback, useImperativeHandle, forwardRef } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import "@xterm/xterm/css/xterm.css";

export interface XTermHandle {
  fit: () => void;
}

interface XTermProps {
  onTerminal: (term: Terminal, fitAddon: FitAddon) => void;
  onReady?: () => void;
  onResize?: (cols: number, rows: number) => void;
  fontSize?: number;
  className?: string;
}

const DARK_THEME = {
  background: "#070a07",
  foreground: "#b8d0b8",
  cursor: "#3dfc3d",
  cursorAccent: "#070a07",
  selectionBackground: "#2e436e",
  black: "#0a0c0a",
  red: "#e05555",
  green: "#5cdb5c",
  yellow: "#e6a940",
  blue: "#5c9edb",
  magenta: "#9c7cdb",
  cyan: "#3a9e8c",
  white: "#b8d0b8",
  brightBlack: "#4d664d",
  brightRed: "#ff6b6b",
  brightGreen: "#7deb7d",
  brightYellow: "#ffc44d",
  brightBlue: "#8ab4f8",
  brightMagenta: "#b8a0f0",
  brightCyan: "#5cdbcd",
  brightWhite: "#d8f0d8",
};

const LIGHT_THEME = {
  background: "#ece8df",
  foreground: "#0f1f0d",
  cursor: "#2d8a2d",
  cursorAccent: "#f4f1ea",
  selectionBackground: "#dce6d2",
  black: "#f4f1ea",
  red: "#c04040",
  green: "#2d8a2d",
  yellow: "#b8761a",
  blue: "#3a6db5",
  magenta: "#6d4db5",
  cyan: "#2d7a6d",
  white: "#1a2818",
  brightBlack: "#8a9680",
  brightRed: "#d06060",
  brightGreen: "#4a9a4a",
  brightYellow: "#d4922a",
  brightBlue: "#5a8dd5",
  brightMagenta: "#8d6dd5",
  brightCyan: "#4d9a8d",
  brightWhite: "#0a1808",
};

function isDarkMode(): boolean {
  return document.documentElement.classList.contains("dark");
}

export const XTerm = forwardRef<XTermHandle, XTermProps>(function XTerm({ onTerminal, onReady, onResize, fontSize = 13, className = "" }, ref) {
  const containerRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
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

  useImperativeHandle(ref, () => ({ fit }), [fit]);

  // Initial mount
  useEffect(() => {
    if (!containerRef.current || initRef.current) return;
    initRef.current = true;

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
      theme: isDarkMode() ? DARK_THEME : LIGHT_THEME,
    });

    const fitAddon = new FitAddon();
    term.loadAddon(fitAddon);

    // Try WebGL renderer for best Unicode support
    try {
      const webglAddon = new WebglAddon();
      term.loadAddon(webglAddon);
    } catch {
      // WebGL not available, fall back to default canvas renderer
    }

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

  // Theme observer
  useEffect(() => {
    const mo = new MutationObserver(() => {
      if (termRef.current) {
        termRef.current.options.theme = isDarkMode() ? DARK_THEME : LIGHT_THEME;
      }
    });
    mo.observe(document.documentElement, { attributes: true, attributeFilter: ["class"] });
    return () => mo.disconnect();
  }, []);

  const themeBg = isDarkMode() ? DARK_THEME.background : LIGHT_THEME.background;

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
