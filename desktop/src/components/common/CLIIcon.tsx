import React from "react";

interface CLIIconProps {
  type: string;
  size?: number;
}

/* ── Anthropic / Claude — official brand SVG ────────────── */
function ClaudeIcon({ size = 16 }: { size: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
      <circle cx="12" cy="12" r="11.5" fill="#faf5ef" stroke="#d4a574" strokeWidth="0.6" />
      <path d="M17.3041 3.541h-3.6718l6.696 16.918H24Zm-10.6082 0L0 20.459h3.7442l1.3693-3.5527h7.0052l1.3693 3.5528h3.7442L10.5363 3.5409Zm-.3712 10.2232 2.2914-5.9456 2.2914 5.9456Z" fill="#d4a574" transform="translate(2.5, 0.8) scale(0.75)" />
    </svg>
  );
}

/* ── OpenAI / Codex — faithful hexagonal flower logo ────── */
function CodexIcon({ size = 16 }: { size: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 32 32" fill="none">
      <circle cx="16" cy="16" r="15" fill="#e8f5f0" stroke="#10a37f" strokeWidth="0.8" />
      {/* Three lozenge shapes rotated to form hexagon flower */}
      <g transform="translate(16, 16)">
        {[0, 60, 120, 180, 240, 300].map((deg, i) => {
          const rad = (deg * Math.PI) / 180;
          const cx = Math.cos(rad) * 5;
          const cy = Math.sin(rad) * 5;
          return (
            <ellipse
              key={i}
              cx={cx}
              cy={cy}
              rx={5}
              ry={3}
              transform={`rotate(${deg + 90} ${cx} ${cy})`}
              fill={i % 2 === 0 ? "#10a37f" : "#1a8a6e"}
              opacity="0.75"
            />
          );
        })}
        <circle cx="0" cy="0" r="2.5" fill="#fff" />
        <circle cx="0" cy="0" r="1.2" fill="#10a37f" />
      </g>
    </svg>
  );
}

/* ── Generic "other" O icon ─────────────────────────────── */
function OtherIcon({ size = 16 }: { size: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none">
      <circle cx="12" cy="12" r="10" fill="#3a6db5" opacity="0.12" stroke="#3a6db5" strokeWidth="1.2" />
      <text x="12" y="16" textAnchor="middle" fontSize="12" fontWeight="bold" fill="#3a6db5" fontFamily="monospace">O</text>
    </svg>
  );
}

/* ── Both — stacked Claude + Codex ──────────────────────── */
function BothIcon({ size = 16 }: { size: number }) {
  const half = size * 0.65;
  return (
    <span style={{ display: "inline-flex", gap: 0, position: "relative", width: size + 2, height: size }}>
      <span style={{ position: "absolute", left: 0, top: size - half, zIndex: 2 }}>
        <ClaudeIcon size={half} />
      </span>
      <span style={{ position: "absolute", left: size - half + 2, top: 0, zIndex: 1 }}>
        <CodexIcon size={half} />
      </span>
    </span>
  );
}

/* ── Export ──────────────────────────────────────────────── */
export function CLIIcon({ type, size = 16 }: CLIIconProps) {
  if (type === "claude" || type === "anthropic") return <ClaudeIcon size={size} />;
  if (type === "codex" || type === "openai") return <CodexIcon size={size} />;
  if (type === "both") return <BothIcon size={size} />;
  return <OtherIcon size={size} />;
}
