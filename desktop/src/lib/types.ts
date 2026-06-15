export interface ProfileSummary {
  name: string;
  desc: string;
  env_count: number;
  is_default: boolean;
  cli_type?: string;
  tags?: string[];       // from _KN_TAGS env var, max 3
}

export interface ProfileList {
  default: string;
  profiles: ProfileSummary[];
}

export interface ProfileDetail {
  name: string;
  desc: string;
  env: Record<string, string>;
  is_default: boolean;
  tags?: string[];
}

export interface EnvOutput {
  name: string;
  env: Record<string, string>;
}

export interface MutationResult {
  ok: boolean;
  error?: string;
  action?: string;
  profile?: string;
  key?: string;
}

// ── Project Management ──

export interface ProjectInfo {
  name: string;
  path: string;
  defaultProfile?: string;
  description?: string;
  pinned: boolean;
}

export interface ActiveProjectContext {
  activeProject: ProjectInfo | null;
  setActiveProject: (project: ProjectInfo | null) => void;
  activateProjectByPath: (path: string | null | undefined) => void;
}

export interface ProjectStats {
  projectPath: string;
  sessionCount: number;
  latestTimestamp: number;
  cliTypes: string[];
  claudeCount: number;
  codexCount: number;
  qoderCount: number;
}

export type ScopeTab = "user" | "project" | "all";

// ── Usage / Token Tracking ──

export interface ProjectUsage {
  project_path?: string;
  project_name?: string;
  tokens_in: number;
  tokens_out: number;
  percentage: number;
  models: { model: string; tokens_in: number; tokens_out: number; percentage: number }[];
}

// ── Hook Execution Logs ──

export interface HookExecutionLog {
  hookId: string;
  timestamp: string;
  exitCode?: number;
  durationMs?: number;
  outputPreview?: string;
  errorPreview?: string;
}

// ── Environment Check ──

export interface InstallOption {
  id: string;
  label: string;
  command?: string;
  description: string;
  recommended: boolean;
  platforms: string[];
}

export interface EnvCheckItem {
  name: string;
  label: string;
  status: "ok" | "warn" | "missing";
  severity?: "ok" | "info" | "warn" | "error";
  category?: "cli" | "shell" | "config";
  detail: string;
  detected_path?: string;
  version?: string;
  install_options?: InstallOption[];
  install_cmd?: string;
}

export type EnvCheckResult = { items: EnvCheckItem[]; all_ok: boolean } | null;

export function itemSeverity(item: EnvCheckItem): "ok" | "info" | "warn" | "error" {
  if (item.severity) return item.severity;
  if (item.status === "ok") return "ok";
  if (item.status === "warn") return "warn";
  return "error";
}

export function recommendedInstallOption(item: EnvCheckItem): InstallOption | null {
  return item.install_options?.find((o) => o.recommended) ?? item.install_options?.[0] ?? null;
}

export function recommendedInstallCommand(item: EnvCheckItem): string | null {
  return recommendedInstallOption(item)?.command ?? item.install_cmd ?? null;
}

// ── CLI type ──

export const CLI_TYPES = [
  { id: "claude", label: "Claude" },
  { id: "codex", label: "Codex" },
  { id: "qoder", label: "Qoder" },
] as const;

export type CliKind = (typeof CLI_TYPES)[number]["id"];

// ── Session (CLI native) ──

export interface SessionInfo {
  sessionId: string;
  title: string;
  cli: CliKind;
  profile: string | null;
  projectPath: string;
  workDir: string;
  timestamp: number;
  status: "active" | "ended";
}

// ── Project Overview ──

export interface CliCounts {
  total: number;
  claude: number;
  codex: number;
  qoder: number;
}

export interface OverviewResources {
  skills: CliCounts;
  plugins: CliCounts;
  commands: CliCounts;
  agents: CliCounts;
}

export interface CliConfigStatus {
  cli: "claude" | "codex" | "qoder";
  dirName: string;
  dirExists: boolean;
  hasConfig: boolean;
  hooksTotal: number;
  hooksEnabled: number;
  skillsCount: number;
  agentsCount: number;
}

export interface ProjectOverviewData {
  sessions: CliCounts;
  resources: OverviewResources;
  configMatrix: CliConfigStatus[];
  recentSessions: SessionInfo[];
}
