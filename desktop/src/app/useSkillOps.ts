//! Skill/Agent toggle, batch, uninstall, and command handlers.
//! Extracted from App.tsx.

import { useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ResourceScanData, AgentManagerData, SelectedItem, BatchToggleItem } from "../components/ResourceList";

type AddToast = (type: "error" | "success", msg: string) => void;

interface SkillOpsDeps {
  scanMultiProject: (paths: (string | null)[]) => Promise<{ skills: ResourceScanData; agents: AgentManagerData }>;
  scanProjectPathsRef: React.MutableRefObject<(string | null)[]>;
  setSkillData: (d: ResourceScanData | null) => void;
  setAgentData: (d: AgentManagerData | null) => void;
  setSelectedSkillItem: React.Dispatch<React.SetStateAction<SelectedItem | null>>;
  setSkillDataLoading: (v: boolean) => void;
  addToast: AddToast;
  syncSelection: (data: ResourceScanData, prev: SelectedItem | null) => SelectedItem | null;
}

/** Shared re-scan wrapper: sets loading, scans, updates state, clears loading. */
async function withLoading(
  setSkillDataLoading: (v: boolean) => void,
  scanMultiProject: (paths: (string | null)[]) => Promise<{ skills: ResourceScanData; agents: AgentManagerData }>,
  scanProjectPathsRef: React.MutableRefObject<(string | null)[]>,
  setSkillData: (d: ResourceScanData | null) => void,
  setAgentData: (d: AgentManagerData | null) => void,
  onSuccess: (skills: ResourceScanData, agents: AgentManagerData) => void,
) {
  setSkillDataLoading(true);
  try {
    const { skills, agents } = await scanMultiProject(scanProjectPathsRef.current);
    setSkillData(skills); setAgentData(agents);
    onSuccess(skills, agents);
  } finally {
    setSkillDataLoading(false);
  }
}

export function useSkillOps(deps: SkillOpsDeps) {
  const { scanMultiProject, scanProjectPathsRef, setSkillData, setAgentData,
    setSelectedSkillItem, setSkillDataLoading, addToast, syncSelection } = deps;

  const handleTogglePlugin = useCallback(async (cli: string, pluginId: string, enabled: boolean) => {
    try {
      await invoke("toggle_plugin", { cli, pluginId, enabled });
      await withLoading(setSkillDataLoading, scanMultiProject, scanProjectPathsRef,
        setSkillData, setAgentData,
        (skills) => setSelectedSkillItem((prev) => syncSelection(skills, prev)));
    } catch (e) { addToast("error", `操作失败: ${e}`); }
  }, [addToast, syncSelection]);

  const handleToggleStandaloneSkill = useCallback(async (cli: string, skillId: string, enabled: boolean, path?: string) => {
    try {
      await invoke("toggle_standalone_skill", { cli, skillId, enabled, path: path ?? null });
      await withLoading(setSkillDataLoading, scanMultiProject, scanProjectPathsRef,
        setSkillData, setAgentData,
        (skills) => setSelectedSkillItem((prev) => syncSelection(skills, prev)));
    } catch (e) { addToast("error", `操作失败: ${e}`); }
  }, [addToast, syncSelection]);

  const handleToggleAgent = useCallback(async (cli: string, name: string, enabled: boolean, path?: string) => {
    try {
      await invoke("toggle_agent", { cli, name, enabled, path: path ?? null });
      await withLoading(setSkillDataLoading, scanMultiProject, scanProjectPathsRef,
        setSkillData, setAgentData,
        (_skills, agents) => setSelectedSkillItem((prev) => {
          if (prev?.type === "agent" && prev.data.name === name) {
            const found = agents.agents.find((a) => a.name === name && a.cli === cli);
            return found ? { type: "agent", data: found } : null;
          }
          return prev;
        }));
    } catch (e) { addToast("error", `操作失败: ${e}`); }
  }, [addToast]);

  const handleDeleteAgent = useCallback(async (cli: string, name: string, path?: string) => {
    try {
      await invoke("delete_agent", { cli, name, path: path ?? null });
      await withLoading(setSkillDataLoading, scanMultiProject, scanProjectPathsRef,
        setSkillData, setAgentData, () => {});
      setSelectedSkillItem(null);
      addToast("success", `已删除 Agent "${name}"`);
    } catch (e) { addToast("error", `删除失败: ${e}`); }
  }, [addToast]);

  const handleBatchToggle = useCallback(async (items: BatchToggleItem[], enabled: boolean) => {
    try {
      for (const item of items) {
        if (item.id.includes(":plugin:")) await invoke("toggle_plugin", { cli: item.cli, pluginId: item.id, enabled });
        else if (item.id.includes(":agent:")) await invoke("toggle_agent", { cli: item.cli, name: item.id.split(":").pop()!, enabled, path: item.path ?? null });
        else if (item.id.includes(":command:") || item.id.includes("-command:")) await invoke("toggle_command", { cli: item.cli, name: item.id.split(":").pop()!, enabled, path: item.path ?? null });
        else await invoke("toggle_standalone_skill", { cli: item.cli, skillId: item.id, enabled, path: item.path ?? null });
      }
      await withLoading(setSkillDataLoading, scanMultiProject, scanProjectPathsRef,
        setSkillData, setAgentData,
        (skills) => setSelectedSkillItem((prev) => syncSelection(skills, prev)));
    } catch (e) { addToast("error", `批量操作失败: ${e}`); }
  }, [addToast, syncSelection]);

  const handleBatchUninstall = useCallback(async (items: BatchToggleItem[]) => {
    try {
      for (const item of items) {
        const isCommand = item.id.includes(":command:") || item.id.includes("-command:");
        if (item.id.includes(":plugin:")) await invoke("uninstall_plugin", { cli: item.cli, pluginId: item.id });
        else if (item.id.includes(":agent:")) await invoke("delete_agent", { cli: item.cli, name: item.id.split(":").pop()!, path: item.path ?? null });
        else if (isCommand) await invoke("uninstall_command", { cli: item.cli, name: item.id.split(":").pop()!, path: item.path ?? null });
        else await invoke("uninstall_standalone_skill", { cli: item.cli, skillId: item.id, skillPath: item.path ?? null, skillName: item.id.split(":").pop()! });
      }
      await withLoading(setSkillDataLoading, scanMultiProject, scanProjectPathsRef,
        setSkillData, setAgentData, () => {});
      setSelectedSkillItem(null);
      addToast("success", `已删除 ${items.length} 项`);
    } catch (e) { addToast("error", `批量删除失败: ${e}`); }
  }, [addToast]);

  const handleToggleCommand = useCallback(async (cli: string, name: string, enabled: boolean, path?: string) => {
    try {
      await invoke("toggle_command", { cli, name, enabled, path: path ?? null });
      await withLoading(setSkillDataLoading, scanMultiProject, scanProjectPathsRef,
        setSkillData, setAgentData,
        (skills) => setSelectedSkillItem((prev) => syncSelection(skills, prev)));
    } catch (e) { addToast("error", `操作失败: ${e}`); }
  }, [addToast, syncSelection]);

  const handleUninstallPlugin = useCallback(async (cli: string, pluginId: string) => {
    try {
      const msg = await invoke<string>("uninstall_plugin", { cli, pluginId });
      addToast("success", msg);
      await withLoading(setSkillDataLoading, scanMultiProject, scanProjectPathsRef,
        setSkillData, setAgentData,
        (skills) => setSelectedSkillItem((prev) => syncSelection(skills, prev)));
    } catch (e) { addToast("error", `删除失败: ${e}`); }
  }, [addToast, syncSelection]);

  const handleUninstallStandaloneSkill = useCallback(async (cli: string, skillId: string, path?: string, name?: string) => {
    try {
      const msg = await invoke<string>("uninstall_standalone_skill", { cli, skillId, skillPath: path ?? null, skillName: name ?? null });
      addToast("success", msg);
      await withLoading(setSkillDataLoading, scanMultiProject, scanProjectPathsRef,
        setSkillData, setAgentData, () => {});
      setSelectedSkillItem(null);
    } catch (e) { addToast("error", `删除失败: ${e}`); }
  }, [addToast]);

  const handleUninstallCommand = useCallback(async (cli: string, name: string, path?: string) => {
    try {
      const msg = await invoke<string>("uninstall_command", { cli, name, path: path ?? null });
      addToast("success", msg);
      await withLoading(setSkillDataLoading, scanMultiProject, scanProjectPathsRef,
        setSkillData, setAgentData, () => {});
      setSelectedSkillItem(null);
    } catch (e) { addToast("error", `删除失败: ${e}`); }
  }, [addToast]);

  return {
    handleTogglePlugin, handleToggleStandaloneSkill, handleToggleAgent, handleDeleteAgent,
    handleBatchToggle, handleBatchUninstall, handleToggleCommand,
    handleUninstallPlugin, handleUninstallStandaloneSkill, handleUninstallCommand,
  };
}
