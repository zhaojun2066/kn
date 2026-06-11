import { useCallback, useEffect, useState } from "react";
import type { ActiveProjectContext, ProjectInfo } from "../lib/types";

const STORAGE_KEY = "kn-active-project";

function pathBelongsToProject(path: string, projectPath: string): boolean {
  const cleanPath = path.replace(/[\\/]+$/, "");
  const cleanProjectPath = projectPath.replace(/[\\/]+$/, "");
  return cleanPath === cleanProjectPath
    || cleanPath.startsWith(`${cleanProjectPath}/`)
    || cleanPath.startsWith(`${cleanProjectPath}\\`);
}

export function useProjectContext(projects: ProjectInfo[]): ActiveProjectContext {
  const [activeProject, setActiveProjectState] = useState<ProjectInfo | null>(null);

  useEffect(() => {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (!stored) {
      setActiveProjectState((prev) => {
        if (!prev) return null;
        return projects.find((p) => p.name === prev.name) ?? null;
      });
      return;
    }

    const found = projects.find((p) => p.name === stored);
    if (found) {
      setActiveProjectState(found);
    } else {
      localStorage.removeItem(STORAGE_KEY);
      setActiveProjectState(null);
    }
  }, [projects]);

  const setActiveProject = useCallback((project: ProjectInfo | null) => {
    setActiveProjectState(project);
    if (project) localStorage.setItem(STORAGE_KEY, project.name);
    else localStorage.removeItem(STORAGE_KEY);
  }, []);

  const activateProjectByPath = useCallback((path: string | null | undefined) => {
    if (!path) return;
    const found = projects.find((project) => pathBelongsToProject(path, project.path));
    if (found) setActiveProject(found);
  }, [projects, setActiveProject]);

  return { activeProject, setActiveProject, activateProjectByPath };
}
