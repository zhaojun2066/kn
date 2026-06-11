import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ProjectInfo, ProjectStats } from "../lib/types";

const STORAGE_KEY = "kn-active-project";

export interface UseProjectsReturn {
  projects: ProjectInfo[];
  activeProject: ProjectInfo | null;
  loading: boolean;
  statsMap: Record<string, ProjectStats>;
  setActiveProject: (project: ProjectInfo | null) => void;
  loadProjects: () => Promise<void>;
  addProject: (name: string, path: string) => Promise<void>;
  removeProject: (name: string) => Promise<void>;
  updateProject: (name: string, newName?: string, newPath?: string, defaultProfile?: string, description?: string, pinned?: boolean) => Promise<void>;
  setDefaultProfile: (projectName: string, profile: string | null) => Promise<void>;
  setDescription: (projectName: string, description: string) => Promise<void>;
  togglePin: (projectName: string, pinned: boolean) => Promise<void>;
}

export function useProjects(): UseProjectsReturn {
  const [projects, setProjects] = useState<ProjectInfo[]>([]);
  const [activeProject, setActiveProjectState] = useState<ProjectInfo | null>(null);
  const [loading, setLoading] = useState(true);
  const [statsMap, setStatsMap] = useState<Record<string, ProjectStats>>({});

  const loadStats = useCallback(async (list: ProjectInfo[]) => {
    if (list.length === 0) { setStatsMap({}); return; }
    try {
      const paths = list.map((p) => p.path);
      const stats: Record<string, ProjectStats> = await invoke("get_project_stats", { projectPaths: paths });
      setStatsMap(stats);
    } catch (e) {
      console.error("[useProjects] loadStats failed:", e);
    }
  }, []);

  const loadProjects = useCallback(async () => {
    try {
      const list = await invoke<ProjectInfo[]>("list_projects");
      setProjects(list);

      // Restore active project from localStorage, validate it still exists
      const stored = localStorage.getItem(STORAGE_KEY);
      if (stored) {
        const found = list.find((p) => p.name === stored);
        if (found) {
          setActiveProjectState(found);
        } else {
          localStorage.removeItem(STORAGE_KEY);
          setActiveProjectState(null);
        }
      }

      // Async load stats (non-blocking)
      loadStats(list);
    } catch (e) {
      console.error("[useProjects] load failed:", e);
    } finally {
      setLoading(false);
    }
  }, [loadStats]);

  const setActiveProject = useCallback((project: ProjectInfo | null) => {
    setActiveProjectState(project);
    if (project) {
      localStorage.setItem(STORAGE_KEY, project.name);
    } else {
      localStorage.removeItem(STORAGE_KEY);
    }
  }, []);

  const addProject = useCallback(async (name: string, path: string) => {
    await invoke("add_project", { name, path });
    await loadProjects();
  }, [loadProjects]);

  const removeProject = useCallback(async (name: string) => {
    await invoke("remove_project", { name });
    setActiveProjectState((prev) => {
      if (prev?.name === name) {
        localStorage.removeItem(STORAGE_KEY);
        return null;
      }
      return prev;
    });
    await loadProjects();
  }, [loadProjects]);

  const updateProject = useCallback(async (name: string, newName?: string, newPath?: string, defaultProfile?: string, description?: string, pinned?: boolean) => {
    await invoke("update_project", {
      name,
      newName: newName ?? null,
      newPath: newPath ?? null,
      defaultProfile: defaultProfile ?? null,
      description: description ?? null,
      pinned: pinned ?? null,
    });
    await loadProjects();
  }, [loadProjects]);

  const setDefaultProfile = useCallback(async (projectName: string, profile: string | null) => {
    await invoke("update_project", {
      name: projectName,
      newName: null,
      newPath: null,
      defaultProfile: profile ?? "",
      description: null,
      pinned: null,
    });
    await loadProjects();
  }, [loadProjects]);

  const setDescription = useCallback(async (projectName: string, description: string) => {
    await invoke("update_project", {
      name: projectName,
      newName: null,
      newPath: null,
      defaultProfile: null,
      description,
      pinned: null,
    });
    await loadProjects();
  }, [loadProjects]);

  const togglePin = useCallback(async (projectName: string, pinned: boolean) => {
    await invoke("toggle_pin_project", { name: projectName, pinned });
    await loadProjects();
  }, [loadProjects]);

  useEffect(() => {
    loadProjects();
  }, [loadProjects]);

  return {
    projects,
    activeProject,
    loading,
    statsMap,
    setActiveProject,
    loadProjects,
    addProject,
    removeProject,
    updateProject,
    setDefaultProfile,
    setDescription,
    togglePin,
  };
}
