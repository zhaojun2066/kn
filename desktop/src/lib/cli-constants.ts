// Shared CLI color/label constants — single source of truth.
// Previously duplicated in ResourceList.tsx, HookList.tsx, ResourceDetail.tsx,
// HookDetail.tsx, and MarketplaceBrowser.tsx.

import type { CliKind } from "./types";

export function normalizeCliKind(cli: string | null | undefined): CliKind | null {
  if (cli === "claude" || cli === "codex" || cli === "qoder") return cli;
  if (cli === "qoderclicn") return "qoder";
  return null;
}

/** Hex colors for use in inline styles (e.g. dependency graph nodes) */
export const CLI_HEX_COLORS: Record<CliKind, string> = {
  claude: "#D97706",
  codex: "#7C3AED",
  qoder: "#65d76f",
};

/** CSS variable references for use in Tailwind/className contexts */
export const CLI_CSS_COLORS: Record<CliKind, string> = {
  claude: "#d97757",
  codex: "var(--app-blue)",
  qoder: "#65d76f",
};

/** Human-readable display names */
export const CLI_LABELS: Record<CliKind, string> = {
  claude: "Claude",
  codex: "Codex",
  qoder: "QoderCN",
};

export function cliDisplayName(cli: string | null | undefined): string {
  const normalized = normalizeCliKind(cli);
  return normalized ? CLI_LABELS[normalized] : (cli ?? "");
}

export function cliCssColor(cli: string | null | undefined): string {
  const normalized = normalizeCliKind(cli);
  return normalized ? CLI_CSS_COLORS[normalized] : "var(--app-text-muted)";
}

export function cliHexColor(cli: string | null | undefined): string {
  const normalized = normalizeCliKind(cli);
  return normalized ? CLI_HEX_COLORS[normalized] : "#6B7280";
}

/** Standard filter dropdown options */
export const CLI_FILTER_OPTIONS = [
  { value: "all", label: "全部 CLI" },
  { value: "claude", label: "Claude" },
  { value: "codex", label: "Codex" },
  { value: "qoder", label: "QoderCN" },
] as const;
