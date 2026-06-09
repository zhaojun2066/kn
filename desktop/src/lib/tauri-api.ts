import { invoke } from "@tauri-apps/api/core";
import type { ProfileList, ProfileDetail, MutationResult, HookExecutionLog } from "./types";

export async function listProfiles(): Promise<ProfileList> {
  return invoke("list_profiles");
}

export async function showProfile(name: string): Promise<ProfileDetail> {
  return invoke("show_profile", { name });
}

export async function addProfile(
  name: string,
  desc?: string
): Promise<MutationResult> {
  return invoke("add_profile", { name, desc });
}

export async function removeProfile(
  name: string
): Promise<MutationResult> {
  return invoke("remove_profile", { name });
}

export async function setEnvVar(
  name: string,
  key: string,
  value: string
): Promise<MutationResult> {
  return invoke("set_env_var", { name, key, value });
}

export async function unsetEnvVar(
  name: string,
  key: string
): Promise<MutationResult> {
  return invoke("unset_env_var", { name, key });
}

export async function setDefaultProfile(
  name: string
): Promise<MutationResult> {
  return invoke("set_default_profile", { name });
}

export async function getDefaultProfile(): Promise<string> {
  return invoke("get_default_profile");
}

export async function initProfiles(): Promise<MutationResult> {
  return invoke("init_profiles");
}

export interface UsageSummary {
  total_tokens_in: number;
  total_tokens_out: number;
  by_model: ModelUsage[];
}

export interface ModelUsage {
  model: string;
  tokens_in: number;
  tokens_out: number;
  percentage: number;
}

export interface DailyUsage {
  date: string;
  tokens_in: number;
  tokens_out: number;
}

export interface ModelPricing {
  input: number;
  output: number;
  currency: string;
}

export interface ProjectUsage {
  project_path?: string;
  project_name?: string;
  tokens_in: number;
  tokens_out: number;
  percentage: number;
  models: ModelUsage[];
}

export async function getUsage(days: number): Promise<UsageSummary> {
  return invoke("get_usage", { days });
}

export async function getDailyUsage(days: number): Promise<DailyUsage[]> {
  return invoke("get_daily_usage", { days });
}

export async function getUsageByProject(days: number): Promise<ProjectUsage[]> {
  return invoke("get_usage_by_project", { days });
}

export async function getPricing(): Promise<Record<string, ModelPricing>> {
  return invoke("get_pricing");
}

export async function setPricing(model: string, pricing: ModelPricing): Promise<void> {
  return invoke("set_pricing", { model, pricing });
}

export async function replacePricing(pricing: Record<string, ModelPricing>): Promise<void> {
  return invoke("replace_pricing", { pricing });
}

export async function getUsageTrackingEnabled(): Promise<boolean> {
  return invoke("get_usage_tracking_enabled");
}

export async function setUsageTrackingEnabled(enabled: boolean): Promise<void> {
  return invoke("set_usage_tracking_enabled", { enabled });
}

export async function getHookExecutionLogs(
  hookId?: string,
  limit?: number,
): Promise<HookExecutionLog[]> {
  return invoke("get_hook_execution_logs", { hookId, limit });
}
