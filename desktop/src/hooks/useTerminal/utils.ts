/** Preserve the selected CLI identity all the way to the Agent. */
export function normalizeTool(tool: string): string {
  return tool;
}

export function parseAiCmd(cmd: string): { tool: string; profile: string } | null {
  const m = cmd.match(/^ai\s+(claude|codex|qoder|qoderclicn)\s+(\S+)/);
  if (!m) return null;
  return { tool: m[1], profile: m[2] };
}

export interface ProfileCliInfo {
  name: string;
  cli_type?: string;
}

/** CLI identity must match exactly; QoderCN uses qoderclicn while legacy qoder sessions remain distinct. */
export function normalizedCli(tool: string | undefined): string | null {
  switch (tool?.trim().toLowerCase()) {
    case "claude": return "claude";
    case "codex": return "codex";
    case "qoder": return "qoder";
    case "qoderclicn": return "qoderclicn";
    default: return null;
  }
}

/** A local history entry may only be resumed with a profile from its own CLI. */
export function isProfileCompatibleWithSession(
  command: string,
  profiles: readonly ProfileCliInfo[],
): boolean {
  const session = parseAiCmd(command);
  if (!session) return true;
  const profile = profiles.find((candidate) => candidate.name === session.profile);
  return profile !== undefined
    && normalizedCli(profile.cli_type) === normalizedCli(session.tool);
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
