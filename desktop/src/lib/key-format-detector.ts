/**
 * API Key Format Detector
 *
 * Lightweight prefix-based detection to provide hints in the ProfileDialog
 * about which CLI tools are compatible with a given API key.
 *
 * Not used for validation — only for subtle inline hints.
 */

export interface KeyHint {
  /** Human-readable label displayed next to the input */
  label: string;
  /** Tool IDs compatible with this key format */
  compatibleTools: string[];
}

const KEY_HINTS: { pattern: RegExp; label: string; compatibleTools: string[] }[] = [
  {
    pattern: /^sk-ant-api03-/i,
    label: "Anthropic Key — compatible with Claude Code",
    compatibleTools: ["claude"],
  },
  {
    pattern: /^sk-or-v1-/i,
    label: "OpenRouter Key — compatible with Codex CLI",
    compatibleTools: ["codex"],
  },
  {
    pattern: /^AIzaSy/,
    label: "Google Key — compatible with Gemini CLI",
    compatibleTools: ["gemini"],
  },
  {
    pattern: /^gsk_/i,
    label: "Groq Key — compatible with Codex CLI",
    compatibleTools: ["codex"],
  },
  {
    pattern: /^sk-/i,
    label: "OpenAI-format Key — compatible with Codex CLI",
    compatibleTools: ["codex"],
  },
];

export function detectKeyFormat(value: string): KeyHint | null {
  const trimmed = value.trim();
  if (!trimmed) return null;
  for (const hint of KEY_HINTS) {
    if (hint.pattern.test(trimmed)) {
      return { label: hint.label, compatibleTools: hint.compatibleTools };
    }
  }
  return null;
}
