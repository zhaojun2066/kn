import { invoke } from "@tauri-apps/api/core";
import type { ProfileList, ProfileDetail, MutationResult } from "./types";

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
