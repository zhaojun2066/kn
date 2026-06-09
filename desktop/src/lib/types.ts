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

