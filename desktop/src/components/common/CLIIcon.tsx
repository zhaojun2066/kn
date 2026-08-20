import React from "react";
import claudeMarkPng from "../../assets/icons/claude-mark.png";
import codexMarkPng from "../../assets/icons/codex-mark.png";
import qoderMarkPng from "../../assets/icons/qoder-mark.png";

interface CLIIconProps {
  type: string;
  size?: number;
}

function MarkIcon({
  src,
  alt,
  size,
  padding,
}: {
  src: string;
  alt: string;
  size: number;
  padding: number;
}) {
  return (
    <span
      className="inline-flex items-center justify-center shrink-0"
      style={{ width: size, height: size, padding }}
      aria-hidden="true"
    >
      <img src={src} alt={alt} className="block w-full h-full object-contain" draggable={false} />
    </span>
  );
}

/* ── Claude Code — same mark as the iOS running profile UI ── */
function ClaudeIcon({ size = 16 }: { size: number }) {
  return <MarkIcon src={claudeMarkPng} alt="Claude Code" size={size} padding={Math.max(1, Math.round(size * 0.16))} />;
}

/* ── Codex — same mark as the iOS running profile UI ─────── */
function CodexIcon({ size = 16 }: { size: number }) {
  return <MarkIcon src={codexMarkPng} alt="Codex" size={size} padding={Math.max(1, Math.round(size * 0.11))} />;
}

/* ── QoderCN — same mark as the iOS running profile UI ───── */
function QoderCNIcon({ size = 16 }: { size: number }) {
  return <MarkIcon src={qoderMarkPng} alt="QoderCN" size={size} padding={Math.max(1, Math.round(size * 0.16))} />;
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
  if (type === "qoder" || type === "qoderclicn") return <QoderCNIcon size={size} />;
  return <OtherIcon size={size} />;
}
