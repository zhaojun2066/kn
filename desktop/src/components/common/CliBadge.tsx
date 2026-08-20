import React from "react";
import type { CliKind } from "../../lib/types";
import { cliCssColor, cliDisplayName, cliHexColor } from "../../lib/cli-constants";
import { CLIIcon } from "./CLIIcon";

interface CliBadgeProps {
  cli: CliKind | string;
  /** Use hex colors instead of CSS vars (for graph/visual contexts) */
  variant?: "css" | "hex";
}

export const CliBadge = React.memo(function CliBadge({ cli, variant = "css" }: CliBadgeProps) {
  const label = cliDisplayName(cli);
  const color = variant === "hex" ? cliHexColor(cli) : cliCssColor(cli);
  return (
    <span
      className="inline-flex items-center gap-1.5 text-xs font-medium shrink-0"
      style={{
        color,
        opacity: 0.92,
      }}
    >
      <CLIIcon type={cli} size={14} />
      {label}
    </span>
  );
});
