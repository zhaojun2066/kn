// Shared CLI color/label constants — single source of truth.
// Previously duplicated in ResourceList.tsx, HookList.tsx, ResourceDetail.tsx,
// HookDetail.tsx, and MarketplaceBrowser.tsx.

import type { CliKind } from "./types";

/** Hex colors for use in inline styles (e.g. dependency graph nodes) */
export const CLI_HEX_COLORS: Record<CliKind, string> = {
  claude: "#D97706",
  codex: "#7C3AED",
  qoder: "#059669",
};

/** CSS variable references for use in Tailwind/className contexts */
export const CLI_CSS_COLORS: Record<CliKind, string> = {
  claude: "var(--app-accent)",
  codex: "var(--app-blue)",
  qoder: "var(--app-purple)",
};

/** Human-readable display names */
export const CLI_LABELS: Record<CliKind, string> = {
  claude: "Claude",
  codex: "Codex",
  qoder: "Qoder",
};

/** Standard filter dropdown options */
export const CLI_FILTER_OPTIONS = [
  { value: "all", label: "全部 CLI" },
  { value: "claude", label: "Claude" },
  { value: "codex", label: "Codex" },
  { value: "qoder", label: "Qoder" },
] as const;
