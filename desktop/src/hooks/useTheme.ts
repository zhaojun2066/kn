import { useState, useEffect, useCallback } from "react";

export type ThemeMode = "light" | "dark" | "system";
export type ColorScheme = "forest" | "ocean" | "sepia" | "monochrome" | "neon" | "lavender";

const MODE_KEY = "kn-theme";
const SCHEME_KEY = "kn-color-scheme";

export const COLOR_SCHEMES: { id: ColorScheme; label: string; color: string }[] = [
  { id: "forest",     label: "Forest",     color: "#2d8a2d" },
  { id: "ocean",      label: "Ocean",      color: "#2563c0" },
  { id: "sepia",      label: "Sepia",      color: "#8b5e2a" },
  { id: "monochrome", label: "Mono",       color: "#666666" },
  { id: "neon",       label: "Neon",       color: "#d42d8a" },
  { id: "lavender",   label: "Lavender",   color: "#7c4dc4" },
];

function getSystemTheme(): "light" | "dark" {
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

function resolveTheme(mode: ThemeMode): "light" | "dark" {
  if (mode === "system") return getSystemTheme();
  return mode;
}

function applyTheme(resolved: "light" | "dark") {
  const root = document.documentElement;
  if (resolved === "dark") {
    root.classList.add("dark");
  } else {
    root.classList.remove("dark");
  }
}

function applyColorScheme(scheme: ColorScheme) {
  document.documentElement.setAttribute("data-theme", scheme);
}

export function useTheme() {
  const [mode, setModeState] = useState<ThemeMode>(() => {
    const stored = localStorage.getItem(MODE_KEY);
    if (stored === "light" || stored === "dark" || stored === "system") return stored;
    return "system";
  });

  const [resolved, setResolved] = useState<"light" | "dark">(() => resolveTheme(mode));

  const [colorScheme, setColorSchemeState] = useState<ColorScheme>(() => {
    const stored = localStorage.getItem(SCHEME_KEY);
    if (stored && COLOR_SCHEMES.some((s) => s.id === stored)) return stored as ColorScheme;
    return "forest";
  });

  // Apply theme to DOM
  useEffect(() => {
    const r = resolveTheme(mode);
    setResolved(r);
    applyTheme(r);
  }, [mode]);

  // Apply color scheme to DOM
  useEffect(() => {
    applyColorScheme(colorScheme);
  }, [colorScheme]);

  // Listen for system theme changes when mode is "system"
  useEffect(() => {
    if (mode !== "system") return;
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const handler = () => {
      const r = getSystemTheme();
      setResolved(r);
      applyTheme(r);
    };
    mq.addEventListener("change", handler);
    return () => mq.removeEventListener("change", handler);
  }, [mode]);

  const setTheme = useCallback((m: ThemeMode) => {
    localStorage.setItem(MODE_KEY, m);
    setModeState(m);
  }, []);

  const toggle = useCallback(() => {
    setTheme(resolved === "dark" ? "light" : "dark");
  }, [resolved, setTheme]);

  const setColorScheme = useCallback((s: ColorScheme) => {
    localStorage.setItem(SCHEME_KEY, s);
    setColorSchemeState(s);
  }, []);

  return { mode, resolved, colorScheme, setTheme, setColorScheme, toggle };
}
