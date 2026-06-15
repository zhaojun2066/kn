import React from "react";
import claudeCodeSvg from "../../assets/icons/claude-code-color.svg";
import codexSvg from "../../assets/icons/codex-color.svg";
import qoderSvg from "../../assets/icons/qoder-color.svg";

interface CLIIconProps {
  type: string;
  size?: number;
}

/* ── Claude Code — official brand icon (coral #D97757) ───── */
function ClaudeIcon({ size = 16 }: { size: number }) {
  return <img src={claudeCodeSvg} alt="Claude Code" width={size} height={size} />;
}

/* ── Codex — official brand icon (purple-blue gradient) ──── */
function CodexIcon({ size = 16 }: { size: number }) {
  return <img src={codexSvg} alt="Codex" width={size} height={size} />;
}

/* ── Qoder — official brand icon (green #2ADB5C) ─────────── */
function QoderclicnIcon({ size = 16 }: { size: number }) {
  return <img src={qoderSvg} alt="Qoder" width={size} height={size} />;
}

/* ── Generic "other" icon ────────────────────────────────── */
function OtherIcon({ size = 16 }: { size: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none">
      <circle cx="12" cy="12" r="10" fill="#3a6db5" opacity="0.12" stroke="#3a6db5" strokeWidth="1.2" />
      <text x="12" y="16" textAnchor="middle" fontSize="12" fontWeight="bold" fill="#3a6db5" fontFamily="monospace">?</text>
    </svg>
  );
}

/* ── Export ──────────────────────────────────────────────── */
export function CLIIcon({ type, size = 16 }: CLIIconProps) {
  if (type === "claude" || type === "anthropic") return <ClaudeIcon size={size} />;
  if (type === "codex" || type === "openai") return <CodexIcon size={size} />;
  if (type === "qoderclicn") return <QoderclicnIcon size={size} />;
  return <OtherIcon size={size} />;
}
