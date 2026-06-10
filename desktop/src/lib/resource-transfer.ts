import { invoke } from "@tauri-apps/api/core";
import type { SelectedItem } from "../components/SkillManager";
import type { ProjectInfo } from "./types";

export interface ResourceData {
  id: string;
  name: string;
  path: string;
  cli?: string;
  projectName?: string;
}

export type ResourceType = "skill" | "agent" | "command";

export function getResourceData(item: SelectedItem): ResourceData {
  const data = item.data as Record<string, unknown>;
  return {
    id: (data.id as string) || "",
    name: (data.name as string) || "",
    path: (data.path as string) || "",
    cli: data.cli as string | undefined,
    projectName: data.projectName as string | undefined,
  };
}

export function getResourceType(item: SelectedItem): ResourceType {
  switch (item.type) {
    case "standalone":
    case "system":
    case "plugin-skill":
      return "skill";
    case "agent":
    case "plugin-agent":
      return "agent";
    default:
      return "command";
  }
}

export function getSubdir(resourceType: ResourceType): string {
  return resourceType === "skill" ? "skills" : resourceType === "agent" ? "agents" : "commands";
}

function detectCliFromPath(srcPath: string): string {
  const normalized = srcPath.replace(/\\/g, "/");
  if (normalized.includes("/.codex/")) return "codex";
  if (normalized.includes("/.qoder-cn/") || normalized.includes("/.qoder/")) return "qoder";
  return "claude";
}

function userConfigDir(cli: string): string {
  if (cli === "codex" || cli === "cx") return ".codex";
  if (cli === "qoder") return ".qoder-cn";
  return ".claude";
}

function projectConfigDir(cli: string): string {
  if (cli === "codex" || cli === "cx") return ".codex";
  if (cli === "qoder") return ".qoder";
  return ".claude";
}

function joinPath(base: string, ...segments: string[]): string {
  const sep = base.includes("\\") ? "\\" : "/";
  const trimmedBase = base.replace(/[\\/]+$/, "");
  const trimmedSegments = segments.map((segment) => segment.replace(/^[\\/]+|[\\/]+$/g, ""));
  return [trimmedBase, ...trimmedSegments].filter(Boolean).join(sep);
}

export async function buildDestDir(
  srcPath: string,
  cliFromData: string | undefined,
  toScope: "user" | "project",
  subdir: string,
  project?: ProjectInfo,
): Promise<string> {
  if (toScope === "user") {
    const homeDir = await invoke<string>("get_home_dir");
    const cli = detectCliFromPath(srcPath);
    return joinPath(homeDir, userConfigDir(cli), subdir);
  }

  if (!project) throw new Error("no project");
  const cli = cliFromData || "claude";
  return joinPath(project.path, projectConfigDir(cli), subdir);
}
