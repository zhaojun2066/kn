/** Normalize tool name: parseAiCmd returns "qoderclicn" but agent expects "qoder" */
export function normalizeTool(tool: string): string {
  if (tool === "qoderclicn") return "qoder";
  return tool;
}

export function parseAiCmd(cmd: string): { tool: string; profile: string } | null {
  const m = cmd.match(/^ai\s+(claude|codex|qoderclicn)\s+(\S+)/);
  if (!m) return null;
  return { tool: m[1], profile: m[2] };
}

export interface RunCommandPolicy {
  execution: "local-pty";
  registerRelay: boolean;
  tool: string | null;
  profile: string | null;
}

export function getRunCommandPolicy(cmd: string): RunCommandPolicy {
  const parsed = parseAiCmd(cmd);
  return {
    execution: "local-pty",
    registerRelay: parsed !== null,
    tool: parsed?.tool ?? null,
    profile: parsed?.profile ?? null,
  };
}

export function buildResumeCmd(cmd: string): string | null {
  const parsed = parseAiCmd(cmd);
  if (!parsed) return null;
  if (parsed.tool === "claude") return `ai ${parsed.tool} ${parsed.profile} --resume`;
  if (parsed.tool === "codex") return `ai ${parsed.tool} ${parsed.profile} resume`;
  if (parsed.tool === "qoderclicn") return `ai ${parsed.tool} ${parsed.profile} -r`;
  return null;
}

export function buildResumeLastCmd(cmd: string): string | null {
  const parsed = parseAiCmd(cmd);
  if (!parsed) return null;
  if (parsed.tool === "claude") return `ai ${parsed.tool} ${parsed.profile} -c`;
  if (parsed.tool === "codex") return `ai ${parsed.tool} ${parsed.profile} resume --last`;
  if (parsed.tool === "qoderclicn") return `ai ${parsed.tool} ${parsed.profile} -c`;
  return null;
}
