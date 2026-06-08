import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ProjectInfo } from "../lib/types";

const STORAGE_KEY = "kn-active-project";

export interface UseProjectsReturn {
  projects: ProjectInfo[];
  activeProject: ProjectInfo | null;
  loading: boolean;
  setActiveProject: (project: ProjectInfo | null) => void;
  loadProjects: () => Promise<void>;
  addProject: (name: string, path: string) => Promise<void>;
  removeProject: (name: string) => Promise<void>;
}

export function useProjects(): UseProjectsReturn {
  const [projects, setProjects] = useState<ProjectInfo[]>([]);
  const [activeProject, setActiveProjectState] = useState<ProjectInfo | null>(null);
  const [loading, setLoading] = useState(true);

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
    } catch (e) {
      console.error("[useProjects] load failed:", e);
    } finally {
      setLoading(false);
    }
  }, []);

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
    // Clear active if it was removed
    setActiveProjectState((prev) => {
      if (prev?.name === name) {
        localStorage.removeItem(STORAGE_KEY);
        return null;
      }
      return prev;
    });
    await loadProjects();
  }, [loadProjects]);

  useEffect(() => {
    loadProjects();
  }, [loadProjects]);

  return {
    projects,
    activeProject,
    loading,
    setActiveProject,
    loadProjects,
    addProject,
    removeProject,
  };
}
