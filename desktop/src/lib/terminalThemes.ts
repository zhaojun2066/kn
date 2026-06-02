/** Terminal color scheme presets — independent of app theme.
 *
 * Each panel (right / bottom) stores its own theme independently.
 * When "sync" is enabled, both panels share the same theme.
 */

export interface TerminalTheme {
  name: string;
  label: string;
  background: string;
  foreground: string;
  cursor: string;
  cursorAccent: string;
  selectionBackground: string;
  black: string;
  red: string;
  green: string;
  yellow: string;
  blue: string;
  magenta: string;
  cyan: string;
  white: string;
  brightBlack: string;
  brightRed: string;
  brightGreen: string;
  brightYellow: string;
  brightBlue: string;
  brightMagenta: string;
  brightCyan: string;
  brightWhite: string;
}

export const TERMINAL_THEMES: TerminalTheme[] = [
  {
    name: "default-dark",
    label: "默认深色",
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
  },
  {
    name: "default-light",
    label: "默认浅色",
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
  },
  {
    name: "dracula",
    label: "Dracula",
    background: "#282a36",
    foreground: "#f8f8f2",
    cursor: "#f8f8f2",
    cursorAccent: "#282a36",
    selectionBackground: "#44475a",
    black: "#21222c",
    red: "#ff5555",
    green: "#50fa7b",
    yellow: "#f1fa8c",
    blue: "#bd93f9",
    magenta: "#ff79c6",
    cyan: "#8be9fd",
    white: "#f8f8f2",
    brightBlack: "#6272a4",
    brightRed: "#ff6e6e",
    brightGreen: "#69ff94",
    brightYellow: "#ffffa5",
    brightBlue: "#d6acff",
    brightMagenta: "#ff92df",
    brightCyan: "#a4ffff",
    brightWhite: "#ffffff",
  },
  {
    name: "solarized-dark",
    label: "Solarized Dark",
    background: "#002b36",
    foreground: "#839496",
    cursor: "#839496",
    cursorAccent: "#002b36",
    selectionBackground: "#073642",
    black: "#073642",
    red: "#dc322f",
    green: "#859900",
    yellow: "#b58900",
    blue: "#268bd2",
    magenta: "#d33682",
    cyan: "#2aa198",
    white: "#eee8d5",
    brightBlack: "#002b36",
    brightRed: "#cb4b16",
    brightGreen: "#586e75",
    brightYellow: "#657b83",
    brightBlue: "#839496",
    brightMagenta: "#6c71c4",
    brightCyan: "#93a1a1",
    brightWhite: "#fdf6e3",
  },
  {
    name: "solarized-light",
    label: "Solarized Light",
    background: "#fdf6e3",
    foreground: "#657b83",
    cursor: "#657b83",
    cursorAccent: "#fdf6e3",
    selectionBackground: "#eee8d5",
    black: "#eee8d5",
    red: "#dc322f",
    green: "#859900",
    yellow: "#b58900",
    blue: "#268bd2",
    magenta: "#d33682",
    cyan: "#2aa198",
    white: "#073642",
    brightBlack: "#fdf6e3",
    brightRed: "#cb4b16",
    brightGreen: "#586e75",
    brightYellow: "#657b83",
    brightBlue: "#839496",
    brightMagenta: "#6c71c4",
    brightCyan: "#93a1a1",
    brightWhite: "#002b36",
  },
  {
    name: "monokai",
    label: "Monokai",
    background: "#272822",
    foreground: "#f8f8f2",
    cursor: "#f8f8f2",
    cursorAccent: "#272822",
    selectionBackground: "#49483e",
    black: "#272822",
    red: "#f92672",
    green: "#a6e22e",
    yellow: "#f4bf75",
    blue: "#66d9ef",
    magenta: "#ae81ff",
    cyan: "#a1efe4",
    white: "#f8f8f2",
    brightBlack: "#75715e",
    brightRed: "#f92672",
    brightGreen: "#a6e22e",
    brightYellow: "#f4bf75",
    brightBlue: "#66d9ef",
    brightMagenta: "#ae81ff",
    brightCyan: "#a1efe4",
    brightWhite: "#f9f8f5",
  },
];

const SYNC_KEY = "kn-terminal-theme-synced";
const SHARED_KEY = "kn-terminal-theme";

function panelKey(mode: string): string {
  return `kn-terminal-theme-${mode}`;
}

export function isThemeSync(): boolean {
  try { return localStorage.getItem(SYNC_KEY) === "1"; } catch { return false; }
}

export function setThemeSync(synced: boolean): void {
  try {
    if (synced) {
      localStorage.setItem(SYNC_KEY, "1");
    } else {
      localStorage.removeItem(SYNC_KEY);
    }
  } catch { /* */ }
}

export function loadTerminalTheme(mode: string): string {
  try {
    const key = isThemeSync() ? SHARED_KEY : panelKey(mode);
    return localStorage.getItem(key) || "default-dark";
  } catch {
    return "default-dark";
  }
}

export function saveTerminalTheme(name: string, mode: string): void {
  try {
    // Always save to panel-specific key (so unsync keeps last value)
    localStorage.setItem(panelKey(mode), name);
    // If synced, also save to shared key
    if (isThemeSync()) {
      localStorage.setItem(SHARED_KEY, name);
    }
  } catch { /* */ }
}

export function getThemeByName(name: string): TerminalTheme {
  return TERMINAL_THEMES.find((t) => t.name === name) || TERMINAL_THEMES[0];
}
