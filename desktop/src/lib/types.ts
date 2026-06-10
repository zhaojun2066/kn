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
