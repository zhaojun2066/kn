import React from "react";
import type { CliKind } from "../SkillManager";
import { CLI_LABELS, CLI_CSS_COLORS } from "../../lib/cli-constants";

interface CliBadgeProps {
  cli: CliKind | string;
  /** Use hex colors instead of CSS vars (for graph/visual contexts) */
  variant?: "css" | "hex";
}

const HEX: Record<string, string> = {
  claude: "#D97706",
  codex: "#7C3AED",
  qoder: "#059669",
};

export const CliBadge = React.memo(function CliBadge({ cli, variant = "css" }: CliBadgeProps) {
  const label = CLI_LABELS[cli as CliKind] || cli;
  const color = variant === "hex" ? (HEX[cli] || "#6B7280") : (CLI_CSS_COLORS[cli as CliKind] || "var(--app-text-muted)");
  return (
    <span
      className="text-2xs px-1.5 py-0.5 border font-mono shrink-0"
      style={{
        color,
        borderColor: color,
        opacity: 0.7,
      }}
    >
      {label}
    </span>
  );
});
