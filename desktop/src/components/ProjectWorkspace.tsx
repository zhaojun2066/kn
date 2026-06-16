import React, { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { ProjectInfo, SessionInfo, ProfileSummary, ProjectOverviewData } from "../lib/types";
import { ProjectOverview } from "./ProjectOverview";
import { SessionList } from "./SessionList";
import { FileTree, type FileTreeNode } from "./FileTree";
import { FileContentBlock } from "./common/FileContentBlock";
import { ConfirmDialog } from "./ConfirmDialog";
import { useOverwriteConfirm, describeOverwrite } from "../hooks/useOverwriteConfirm";
import {
  ResourceList,
  type ResourceScanData,
  type AgentManagerData,
  type SelectedItem,
  type BatchToggleItem,
  type PluginUpdateInfo,
} from "./ResourceList";
import type { CliKind } from "../lib/types";
import { ResourceDetail } from "./ResourceDetail";
import type { LocalCliUsageRow } from "./LocalCliUsage";
import { getResourceData, getResourceType, getSubdir, buildDestDir, type ResourceData } from "../lib/resource-transfer";
import { HookList, type HookManagerData } from "./HookList";
import { HookDetail, type HookEntry } from "./HookDetail";
import { HookWizard } from "./HookWizard";
import { HookStore } from "./HookStore";
import { MarketplaceBrowser } from "./MarketplaceBrowser";
import { CliBadge } from "./common/CliBadge";

type ProjectTab = "overview" | "sessions" | "resource" | "hooks" | "files";

const TABS: { key: ProjectTab; label: string }[] = [
  { key: "overview", label: "Overview" },
  { key: "sessions", label: "Sessions" },
  { key: "resource", label: "Resource" },
  { key: "hooks", label: "Hooks" },
  { key: "files", label: "Files" },
];

interface ProjectWorkspaceProps {
  project: ProjectInfo;
  sessions: SessionInfo[];
  sessionsLoading: boolean;
  cliUsageRows: LocalCliUsageRow[];
  profiles: ProfileSummary[];
  onRunProfile: (profileName: string, cliType: string) => void;
  onSplitProfile?: (profileName: string, cliType: string) => void;
  onSetDefaultProfile: (profileName: string) => void | Promise<void>;
  onScanSessions: (projectPath: string) => void;
  onResumeSession: (session: SessionInfo) => void;
  // Toast integration for resource operations
  addToast: (type: "error" | "success", message: string) => void;
  setToasts: React.Dispatch<React.SetStateAction<any[]>>;
  toastIdRef: React.MutableRefObject<number>;
  // Project management
  projects?: ProjectInfo[];
  onAddProject?: () => void;
  onOpenMarketplace?: () => void;
}

/** Scan project-level skills + agents for a specific project path. */
async function scanProjectResources(projectPath: string): Promise<{ skills: ResourceScanData; agents: AgentManagerData }> {
  const [skills, agents] = await Promise.all([
    invoke<ResourceScanData>("scan_skills", { projectPath }),
    invoke<AgentManagerData>("scan_agents", { projectPath }),
  ]);
  return {
    skills: {
      ...skills,
      plugins: skills.plugins.map((plugin) => ({
        ...plugin,
        inherited: plugin.source !== "project",
      })),
      standaloneSkills: skills.standaloneSkills.map((skill) => ({
        ...skill,
        inherited: !skill.id.includes(":project-") && !skill.projectName,
      })),
      commands: (skills.commands || []).map((command) => ({
        ...command,
        inherited: !command.id.includes(":project-") && !command.projectName,
      })),
    },
    agents: {
      agents: agents.agents.map((agent) => ({
        ...agent,
        inherited: agent.source !== "project" && !agent.projectName,
      })),
    },
  };
}

function markProjectHookInheritance(data: HookManagerData): HookManagerData {
  return {
    hooks: data.hooks.map((hook) => ({
      ...hook,
      inherited: hook.source !== "project" && !hook.projectName,
    })),
  };
}

export function ProjectWorkspace({
  project,
  sessions,
  sessionsLoading,
  cliUsageRows,
  profiles,
  onRunProfile,
  onSplitProfile,
  onSetDefaultProfile,
  onScanSessions,
  onResumeSession,
  addToast,
  setToasts,
  toastIdRef,
  projects = [],
  onAddProject = () => {},
  onOpenMarketplace,
}: ProjectWorkspaceProps) {

  const [activeTab, setActiveTab] = useState<ProjectTab>("overview");
  // Tracks the project path that was active during the last data load,
  // so we can distinguish "tab switch" (immediate load) from "project
  // change while on the same tab" (debounced 300ms to skip intermediates
  // during rapid arrow-key navigation).
  const settledPathRef = useRef(project.path);
  const dataDebounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // ── Header: Run picker + Default profile picker ──
  const [showHeaderRunPicker, setShowHeaderRunPicker] = useState(false);
  const [showHeaderDefaultPicker, setShowHeaderDefaultPicker] = useState(false);
  const [headerFocusedIdx, setHeaderFocusedIdx] = useState(0);
  const headerRunRef = useRef<HTMLDivElement>(null);
  const headerDefaultRef = useRef<HTMLDivElement>(null);

  const defaultProfile = project.defaultProfile;
  const defaultProfileObj = profiles.find((p) => p.name === defaultProfile);

  // Close header pickers on outside click
  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (headerRunRef.current && !headerRunRef.current.contains(e.target as Node)) {
        setShowHeaderRunPicker(false);
      }
      if (headerDefaultRef.current && !headerDefaultRef.current.contains(e.target as Node)) {
        setShowHeaderDefaultPicker(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, []);

  const handleHeaderRunDefault = useCallback((e: React.MouseEvent) => {
    if (defaultProfile && defaultProfileObj) {
      if (e.altKey && onSplitProfile) {
        onSplitProfile(defaultProfile, defaultProfileObj.cli_type || "claude");
      } else {
        onRunProfile(defaultProfile, defaultProfileObj.cli_type || "claude");
      }
    } else {
      setShowHeaderRunPicker((v) => !v);
      setShowHeaderDefaultPicker(false);
    }
  }, [defaultProfile, defaultProfileObj, onRunProfile, onSplitProfile]);

  const handleHeaderSelectProfile = useCallback((profile: ProfileSummary) => {
    onRunProfile(profile.name, profile.cli_type || "claude");
    setShowHeaderRunPicker(false);
  }, [onRunProfile]);

  const handleHeaderSelectDefault = useCallback((profile: ProfileSummary) => {
    onSetDefaultProfile(profile.name);
    setShowHeaderDefaultPicker(false);
  }, [onSetDefaultProfile]);

  const headerPickerProfiles = showHeaderRunPicker || showHeaderDefaultPicker ? profiles : [];
  const handleHeaderPickerKeyDown = useCallback((e: React.KeyboardEvent, mode: "run" | "default") => {
    if (e.key === "ArrowDown") { e.preventDefault(); setHeaderFocusedIdx((i) => Math.min(i + 1, profiles.length - 1)); }
    else if (e.key === "ArrowUp") { e.preventDefault(); setHeaderFocusedIdx((i) => Math.max(i - 1, 0)); }
    else if (e.key === "Enter") { e.preventDefault();
      if (profiles[headerFocusedIdx]) {
        if (mode === "run") handleHeaderSelectProfile(profiles[headerFocusedIdx]);
        else handleHeaderSelectDefault(profiles[headerFocusedIdx]);
      }
    }
    else if (e.key === "Escape") { setShowHeaderRunPicker(false); setShowHeaderDefaultPicker(false); }
  }, [profiles, headerFocusedIdx, handleHeaderSelectProfile, handleHeaderSelectDefault]);

  // ── Resource state (project-level, self-contained) ──
  const [resourceData, setResourceData] = useState<ResourceScanData | null>(null);
  const [agentData, setAgentData] = useState<AgentManagerData | null>(null);
  const [resourceLoading, setResourceLoading] = useState(false);
  const [selectedResource, setSelectedResource] = useState<SelectedItem | null>(null);
  const [checkingUpdates, setCheckingUpdates] = useState(false);
  const [updateInfos, setUpdateInfos] = useState<PluginUpdateInfo[]>([]);
  const checkRequestRef = useRef(false);
  const checkSilentRef = useRef(false);
  // Overwrite confirmation (shared hook for Resources)
  const { requestOverwrite, overwriteDialog } = useOverwriteConfirm();

  // Hook overwrite confirmation (separate from resource overwrite)
  const [hookOverwriteConfirm, setHookOverwriteConfirm] = useState<{
    hookName: string;
    targetLabel: string;
    onConfirm: () => void;
  } | null>(null);

  // ── Hooks state (project-level, self-contained) ──
  const [hookData, setHookData] = useState<HookManagerData | null>(null);
  const [hookLoading, setHookLoading] = useState(false);
  const [selectedHook, setSelectedHook] = useState<HookEntry | null>(null);
  const [hookWizardOpen, setHookWizardOpen] = useState(false);
  const [hookStoreOpen, setHookStoreOpen] = useState(false);
  const [marketplaceOpen, setMarketplaceOpen] = useState(false);
  const homeDirRef = useRef<string>("");

  // ── Overview state ──
  const [overviewData, setOverviewData] = useState<ProjectOverviewData | null>(null);
  const [overviewLoading, setOverviewLoading] = useState(false);


  // ── Sync selectedResource after data changes (toggle/delete) ──
  const syncSelection = useCallback((data: ResourceScanData, prev: SelectedItem | null) => {
    if (!prev) return null;
    if (prev.type === "plugin") {
      const found = data.plugins.find((p) => p.id === (prev.data as unknown as ResourceData).id);
      return found ? { type: "plugin" as const, data: found } : null;
    }
    if (prev.type === "standalone") {
      const found = data.standaloneSkills.find((s) => s.id === (prev.data as unknown as ResourceData).id);
      return found ? { type: "standalone" as const, data: found } : null;
    }
    if (prev.type === "system") {
      const found = data.systemSkills.find((s) => s.id === (prev.data as unknown as ResourceData).id);
      return found ? { type: "system" as const, data: found } : null;
    }
    if (prev.type === "command") {
      const found = data.commands.find((c) => c.id === (prev.data as unknown as ResourceData).id);
      return found ? { type: "command" as const, data: found } : null;
    }
    // agent / plugin-skill / plugin-agent / plugin-command — not in ResourceScanData
    return prev;
  }, []);

  const syncAgentSelection = useCallback((agents: AgentManagerData, prev: SelectedItem | null) => {
    if (!prev || prev.type !== "agent") return prev;
    const found = agents.agents.find((a) => a.id === (prev.data as unknown as ResourceData).id);
    return found ? { type: "agent" as const, data: found } : null;
  }, []);

  // Keep selectedResource in sync with fresh data after toggle/delete
  useEffect(() => {
    if (resourceData) {
      setSelectedResource((prev) => syncSelection(resourceData, prev));
    }
  }, [resourceData, syncSelection]);

  useEffect(() => {
    if (agentData) {
      setSelectedResource((prev) => syncAgentSelection(agentData, prev));
    }
  }, [agentData, syncAgentSelection]);

  // ── Resource data loading ──
  const loadResourceData = useCallback(async () => {
    if (!project?.path) return;
    setResourceData(null);          // clear stale data immediately
    setAgentData(null);
    setResourceLoading(true);
    try {
      const { skills, agents } = await scanProjectResources(project.path);
      setResourceData(skills);
      setAgentData(agents);
    } catch {
      setResourceData(null);
      setAgentData(null);
    } finally {
      setResourceLoading(false);
    }
  }, [project?.path]);

  // Load resource data: immediate on tab switch, debounced 300ms on
  // project change (so rapid arrow-key nav skips intermediate projects).
  useEffect(() => {
    if (activeTab !== "resource") {
      setSelectedResource(null);
      return;
    }
    // Update check runs once per activation
    checkRequestRef.current = true;
    checkSilentRef.current = true;
    setCheckingUpdates(true);
    setUpdateInfos([]);
    invoke("check_updates").catch(() => { setCheckingUpdates(false); checkRequestRef.current = false; });

    const pathChanged = settledPathRef.current !== project.path;
    settledPathRef.current = project.path;

    if (pathChanged) {
      // Immediately clear stale data from previous project
      setResourceData(null);
      setAgentData(null);
      setResourceLoading(true);
      if (dataDebounceRef.current) clearTimeout(dataDebounceRef.current);
      dataDebounceRef.current = setTimeout(loadResourceData, 300);
    } else {
      loadResourceData();
    }
    return () => {
      if (dataDebounceRef.current) clearTimeout(dataDebounceRef.current);
    };
  }, [activeTab, project?.path, loadResourceData]);

  // ── Hooks data loading ──
  const loadHookData = useCallback(async () => {
    if (!project?.path) return;
    setHookData(null);              // clear stale data immediately
    setHookLoading(true);
    try {
      const result = await invoke<HookManagerData>("scan_hooks", { projectPath: project.path });
      setHookData(markProjectHookInheritance(result));
    } catch {
      setHookData(null);
    } finally {
      setHookLoading(false);
    }
  }, [project?.path]);

  // ── Overview data loading ──
  const loadOverviewData = useCallback(async () => {
    if (!project?.path) return;
    setOverviewData(null);          // clear stale data immediately
    setOverviewLoading(true);
    try {
      const data = await invoke<ProjectOverviewData>("get_project_overview", {
        projectPath: project.path,
      });
      setOverviewData(data);
    } catch {
      setOverviewData(null);
    } finally {
      setOverviewLoading(false);
    }
  }, [project?.path]);

  const refreshHooks = useCallback(async () => {
    if (!project?.path) return;
    try {
      const result = markProjectHookInheritance(await invoke<HookManagerData>("scan_hooks", { projectPath: project.path }));
      setHookData(result);
      setSelectedHook((prev) => {
        if (!prev) return null;
        return result.hooks.find((h) => h.id === prev.id) || null;
      });
    } catch { /* ignore */ }
  }, [project?.path]);

  // Load hooks when switching to hooks tab (immediate) or when the project
  // changes while already on the tab (debounced 300ms).
  // Load hooks: immediate on tab switch, debounced 300ms on project change.
  // When project changes, immediately clear stale data so the user sees a loading
  // skeleton instead of the previous project's hooks during the debounce window.
  useEffect(() => {
    if (activeTab !== "hooks") {
      setSelectedHook(null);
      return;
    }
    const pathChanged = settledPathRef.current !== project.path;
    settledPathRef.current = project.path;

    if (pathChanged) {
      setHookData(null);
      setHookLoading(true);
      if (dataDebounceRef.current) clearTimeout(dataDebounceRef.current);
      dataDebounceRef.current = setTimeout(loadHookData, 300);
    } else {
      loadHookData();
    }
    return () => {
      if (dataDebounceRef.current) clearTimeout(dataDebounceRef.current);
    };
  }, [activeTab, project?.path, loadHookData]);

  // Cache home directory
  useEffect(() => {
    invoke<string>("get_home_dir")
      .then((dir) => { homeDirRef.current = dir; })
      .catch(() => {});
  }, []);

  // ── Event listeners ──
  useEffect(() => {
    const unlisten = listen<PluginUpdateInfo[]>("update-check-complete", (event) => {
      setUpdateInfos(event.payload);
      setCheckingUpdates(false);
      if (checkRequestRef.current && !checkSilentRef.current) {
        const count = event.payload.filter((u) => u.hasUpdate).length;
        if (count > 0) addToast("success", `发现 ${count} 个可用更新`);
        else addToast("success", "所有插件均为最新版本");
      }
      checkRequestRef.current = false;
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    const unlisten = listen<{ pluginId: string; success: boolean; message: string }>(
      "update-plugin-complete",
      async (event) => {
        const { success, message } = event.payload;
        if (success) {
          addToast("success", message);
          if (project?.path) {
            const { skills, agents } = await scanProjectResources(project.path);
            setResourceData(skills);
            setAgentData(agents);
          }
        } else {
          addToast("error", message);
        }
      },
    );
    return () => { unlisten.then((fn) => fn()); };
  }, [project?.path]); // eslint-disable-line react-hooks/exhaustive-deps

  // ── Resource handlers ──
  const rescan = useCallback(async () => {
    if (!project?.path) return;
    const { skills, agents } = await scanProjectResources(project.path);
    setResourceData(skills);
    setAgentData(agents);
  }, [project?.path]);

  const handleMarketplaceInstalled = useCallback(async () => {
    await rescan();
  }, [rescan]);

  const handleTogglePlugin = useCallback(async (cli: string, pluginId: string, enabled: boolean) => {
    try {
      const plugin = resourceData?.plugins.find((p) => p.id === pluginId);
      await invoke("toggle_plugin", {
        cli,
        pluginId,
        enabled,
        projectPath: plugin?.source === "project" ? project?.path ?? null : null,
      });
      await rescan();
    } catch (e) {
      addToast("error", `操作失败: ${e}`);
    }
  }, [addToast, project?.path, rescan, resourceData?.plugins]);

  const handleToggleStandaloneSkill = useCallback(async (cli: string, skillId: string, enabled: boolean, path?: string) => {
    try {
      await invoke("toggle_standalone_skill", { cli, skillId, enabled, path: path ?? null });
      await rescan();
    } catch (e) {
      addToast("error", `操作失败: ${e}`);
    }
  }, [addToast, rescan]);

  const handleToggleAgent = useCallback(async (cli: string, name: string, enabled: boolean, path?: string) => {
    try {
      await invoke("toggle_agent", { cli, name, enabled, path: path ?? null });
      await rescan();
    } catch (e) {
      addToast("error", `操作失败: ${e}`);
    }
  }, [addToast, rescan]);

  const handleDeleteAgent = useCallback(async (cli: string, name: string, path?: string) => {
    try {
      await invoke("delete_agent", { cli, name, path: path ?? null });
      await rescan();
      setSelectedResource(null);
      addToast("success", `已删除 Agent "${name}"`);
    } catch (e) {
      addToast("error", `删除失败: ${e}`);
    }
  }, [addToast, rescan]);

  const handleBatchToggle = useCallback(async (items: BatchToggleItem[], enabled: boolean) => {
    try {
      for (const item of items) {
        if (item.id.includes(":plugin:")) {
          const plugin = resourceData?.plugins.find((p) => p.id === item.id);
          await invoke("toggle_plugin", {
            cli: item.cli,
            pluginId: item.id,
            enabled,
            projectPath: plugin?.source === "project" ? project?.path ?? null : null,
          });
        } else if (item.id.includes(":agent:")) {
          const name = item.id.split(":").pop() || item.id;
          await invoke("toggle_agent", { cli: item.cli, name, enabled, path: item.path ?? null });
        } else if (item.id.includes(":command:") || item.id.includes("-command:")) {
          const name = item.id.split(":").pop() || item.id;
          await invoke("toggle_command", { cli: item.cli, name, enabled, path: item.path ?? null });
        } else {
          await invoke("toggle_standalone_skill", { cli: item.cli, skillId: item.id, enabled, path: item.path ?? null });
        }
      }
      await rescan();
    } catch (e) {
      addToast("error", `批量操作失败: ${e}`);
    }
  }, [addToast, project?.path, rescan, resourceData?.plugins]);

  const handleBatchUninstall = useCallback(async (items: BatchToggleItem[]) => {
    try {
      for (const item of items) {
        const isCommand = item.id.includes(":command:") || item.id.includes("-command:");
        if (item.id.includes(":plugin:")) {
          await invoke("uninstall_plugin", { cli: item.cli, pluginId: item.id, projectPath: project?.path ?? null });
        } else if (item.id.includes(":agent:")) {
          const name = item.id.split(":").pop() || item.id;
          await invoke("delete_agent", { cli: item.cli, name, path: item.path ?? null });
        } else if (isCommand) {
          const name = item.id.split(":").pop() || item.id;
          await invoke("uninstall_command", { cli: item.cli, name, path: item.path ?? null });
        } else {
          const name = item.id.split(":").pop() || item.id;
          await invoke("uninstall_standalone_skill", { cli: item.cli, skillId: item.id, skillPath: item.path ?? null, skillName: name });
        }
      }
      await rescan();
      setSelectedResource(null);
      addToast("success", `已删除 ${items.length} 项`);
    } catch (e) {
      addToast("error", `批量删除失败: ${e}`);
    }
  }, [addToast, rescan]);

  const handleCheckUpdates = useCallback(async () => {
    if (checkingUpdates) return;
    checkRequestRef.current = true;
    checkSilentRef.current = false;
    setCheckingUpdates(true);
    setUpdateInfos([]);
    try {
      await invoke("check_updates");
    } catch (e) {
      addToast("error", `检查更新失败: ${e}`);
      setCheckingUpdates(false);
      checkRequestRef.current = false;
    }
  }, [addToast, checkingUpdates]);

  const handleCancelCheckUpdates = useCallback(async () => {
    try { await invoke("cancel_check_updates"); } catch { /* ignore */ }
  }, []);

  const handleUpdatePlugin = useCallback(async (cli: string, pluginId: string) => {
    try {
      await invoke("update_plugin", { cli, pluginId });
      addToast("success", "正在后台更新...");
    } catch (e) {
      addToast("error", `更新失败: ${e}`);
    }
  }, [addToast]);

  const handleUninstallPlugin = useCallback(async (cli: CliKind, pluginId: string) => {
    try {
      await invoke("uninstall_plugin", { cli, pluginId, projectPath: project?.path ?? null });
      await rescan();
      setSelectedResource(null);
      addToast("success", "已卸载插件");
    } catch (e) {
      addToast("error", `卸载失败: ${e}`);
    }
  }, [addToast, rescan, project?.path]);

  const handleUninstallStandaloneSkill = useCallback(async (cli: CliKind, skillId: string, path?: string, _name?: string) => {
    try {
      const name = skillId.split(":").pop() || skillId;
      await invoke("uninstall_standalone_skill", { cli, skillId, skillPath: path ?? null, skillName: name });
      await rescan();
      setSelectedResource(null);
      addToast("success", "已删除");
    } catch (e) {
      addToast("error", `删除失败: ${e}`);
    }
  }, [addToast, rescan]);

  const handleToggleCommand = useCallback(async (cli: CliKind, name: string, enabled: boolean, path?: string) => {
    try {
      await invoke("toggle_command", { cli, name, enabled, path: path ?? null });
      await rescan();
    } catch (e) {
      addToast("error", `操作失败: ${e}`);
    }
  }, [addToast, rescan]);

  const handleUninstallCommand = useCallback(async (cli: CliKind, name: string, path?: string) => {
    try {
      await invoke("uninstall_command", { cli, name, path: path ?? null });
      await rescan();
      setSelectedResource(null);
      addToast("success", "已删除命令");
    } catch (e) {
      addToast("error", `删除失败: ${e}`);
    }
  }, [addToast, rescan]);

  // ── Move / Copy (project-level) ──

  const doMove = useCallback(async (
    item: SelectedItem, toScope: "user" | "project",
    targetProject?: ProjectInfo, overwrite = false,
  ) => {
    const data = getResourceData(item);
    const resourceType = getResourceType(item);
    const subdir = getSubdir(resourceType);
    const srcPath = data.path as string;
    const fromScope = data.id.includes(":project-") ? "project" : "user";
    const destProject = toScope === "project" ? targetProject : undefined;

    if (toScope === "project" && !destProject) throw new Error("请先选择目标项目");

    const destDir = await buildDestDir(srcPath, data.cli, toScope, subdir, destProject);

    return invoke<any>("move_skill_file", {
      sourcePath: data.path, destDir,
      resourceName: data.name, resourceType,
      fromScope, toScope,
      overwrite,
    });
  }, []);

  const handleMoveResource = useCallback(async (item: SelectedItem, toScope: "user" | "project", targetProject?: ProjectInfo) => {
    try {
      let undoInfo;
      try {
        undoInfo = await doMove(item, toScope, targetProject, false);
      } catch (e) {
        const msg = String(e);
        if (msg.includes("同名资源")) {
          const data = getResourceData(item);
          const confirmed = await requestOverwrite(describeOverwrite(data.name));
          if (!confirmed) return;
          undoInfo = await doMove(item, toScope, targetProject, true);
        } else {
          throw e;
        }
      }

      await rescan();

      const name = getResourceData(item).name;
      setToasts((prev) => {
        const id = ++toastIdRef.current;
        return [...prev, {
          id,
          type: "success" as const,
          message: `已移动 "${name}" → ${toScope === "project" ? "项目级" : "用户级"}`,
          undoAction: async () => {
            await invoke("undo_move_skill", {
              backupPath: undoInfo.backupPath,
              originalPath: undoInfo.originalPath,
              destPath: undoInfo.destPath,
              contentFingerprint: undoInfo.contentFingerprint,
            });
            await rescan();
          },
        } as any];
      });
    } catch (e) {
      const msg = String(e).slice(0, 120);
      if (msg !== "用户取消") addToast("error", `移动失败: ${msg}`);
    }
  }, [addToast, setToasts, toastIdRef, rescan, doMove, requestOverwrite]);

  const handleCopyResource = useCallback(async (item: SelectedItem, toScope: "user" | "project", targetProject?: ProjectInfo) => {
    try {
      const data = getResourceData(item);
      const srcPath = data.path as string;
      const resourceType = getResourceType(item);
      const subdir = getSubdir(resourceType);
      const destProject = toScope === "project" ? targetProject : undefined;

      if (toScope === "project" && !destProject) { addToast("error", "请先选择目标项目"); return; }

      const destDir = await buildDestDir(srcPath, data.cli, toScope, subdir, destProject);

      const doCopy = async (overwrite: boolean) => {
        await invoke("copy_skill_file", { sourcePath: data.path, destDir, resourceName: data.name, overwrite });
      };

      try {
        await doCopy(false);
      } catch (e) {
        const msg = String(e);
        if (msg.includes("同名资源")) {
          const confirmed = await requestOverwrite(describeOverwrite(data.name));
          if (!confirmed) return;
          await doCopy(true);
        } else {
          throw e;
        }
      }

      await rescan();
      addToast("success", `已复制 "${data.name}" → ${toScope === "project" ? "项目级" : "用户级"}`);
    } catch (e) {
      const msg = String(e).slice(0, 120);
      if (msg !== "用户取消") addToast("error", `复制失败: ${msg}`);
    }
  }, [addToast, rescan, requestOverwrite]);

  const handleBatchMove = useCallback(async (items: SelectedItem[], toScope: "user" | "project", targetProject?: ProjectInfo) => {
    let ok = 0;
    let failed = 0;
    for (const item of items) {
      try {
        const data = getResourceData(item);
        const resourceType = getResourceType(item);
        const subdir = getSubdir(resourceType);
        const srcPath = data.path as string;
        const fromScope = data.id.includes(":project-") ? "project" : "user";
        const destProject = toScope === "project" ? targetProject : undefined;

        if (toScope === "project" && !destProject) throw new Error("no project");

        const destDir = await buildDestDir(srcPath, data.cli, toScope, subdir, destProject);

        try {
          await invoke("move_skill_file", {
            sourcePath: data.path, destDir, resourceName: data.name,
            resourceType, fromScope, toScope, overwrite: false,
          });
        } catch (e) {
          const msg = String(e);
          if (msg.includes("同名资源")) {
            const confirmed = await requestOverwrite(describeOverwrite(data.name));
            if (!confirmed) { failed++; continue; }
            await invoke("move_skill_file", {
              sourcePath: data.path, destDir, resourceName: data.name,
              resourceType, fromScope, toScope, overwrite: true,
            });
          } else {
            throw e;
          }
        }
        ok++;
      } catch { failed++; }
    }
    await rescan();
    if (failed === 0) addToast("success", `已移动 ${ok} 项 → ${toScope === "project" ? "项目级" : "用户级"}`);
    else addToast("error", `移动完成: ${ok} 成功, ${failed} 失败`);
  }, [addToast, rescan, requestOverwrite]);

  const handleBatchCopy = useCallback(async (items: SelectedItem[], toScope: "user" | "project", targetProject?: ProjectInfo) => {
    let ok = 0;
    let failed = 0;
    for (const item of items) {
      try {
        const data = getResourceData(item);
        const srcPath = data.path as string;
        const resourceType = getResourceType(item);
        const subdir = getSubdir(resourceType);
        const destProject = toScope === "project" ? targetProject : undefined;

        if (toScope === "project" && !destProject) throw new Error("no project");

        const destDir = await buildDestDir(srcPath, data.cli, toScope, subdir, destProject);

        try {
          await invoke("copy_skill_file", { sourcePath: data.path, destDir, resourceName: data.name, overwrite: false });
        } catch (e) {
          const msg = String(e);
          if (msg.includes("同名资源")) {
            const confirmed = await requestOverwrite(describeOverwrite(data.name));
            if (!confirmed) { failed++; continue; }
            await invoke("copy_skill_file", { sourcePath: data.path, destDir, resourceName: data.name, overwrite: true });
          } else {
            throw e;
          }
        }
        ok++;
      } catch { failed++; }
    }
    await rescan();
    if (failed === 0) addToast("success", `已复制 ${ok} 项 → ${toScope === "project" ? "项目级" : "用户级"}`);
    else addToast("error", `复制完成: ${ok} 成功, ${failed} 失败`);
  }, [addToast, rescan, requestOverwrite]);

  // ── Hook operation helpers ──

  /** Build target config path for hook move/copy operations */
  const getHookTargetPath = useCallback((cli: string, toScope: "user" | "project", targetProjectPath?: string): string => {
    const configFile = cli === "codex" ? "config.toml" : "settings.json";
    if (toScope === "user") {
      const cliDirUser = cli === "qoder" ? ".qoder-cn" : cli === "codex" ? ".codex" : ".claude";
      return `${homeDirRef.current}/${cliDirUser}/${configFile}`;
    }
    if (targetProjectPath) {
      const cliDirProject = cli === "qoder" ? ".qoder" : cli === "codex" ? ".codex" : ".claude";
      return `${targetProjectPath.replace(/\/+$/, "")}/${cliDirProject}/${configFile}`;
    }
    return "";
  }, []);

  /** Scan the target path to check if a hook with same cli+eventType+command already exists */
  const checkDuplicateAtTarget = useCallback(async (hook: HookEntry, toScope: "user" | "project", targetProject?: ProjectInfo): Promise<boolean> => {
    try {
      const scanPath = toScope === "user" ? null : (targetProject?.path ?? null);
      if (toScope === "project" && !scanPath) return false;
      const result = await invoke<HookManagerData>("scan_hooks", { projectPath: scanPath });
      return result.hooks.some((h) =>
        h.cli === hook.cli &&
        h.eventType === hook.eventType &&
        h.command === hook.command
      );
    } catch {
      return false; // If scan fails, proceed anyway
    }
  }, []);

  // ── Hook operation handlers ──

  const handleToggleHook = useCallback(async (hook: HookEntry, enabled: boolean) => {
    try {
      await invoke("toggle_hook", {
        cli: hook.cli,
        eventType: hook.eventType,
        groupIdx: hook.groupIdx,
        hookIdx: hook.hookIdx,
        enabled,
        path: hook.path,
      });
      refreshHooks();
    } catch (e) {
      addToast("error", `操作失败: ${String(e).slice(0, 120)}`);
    }
  }, [addToast, refreshHooks]);

  const handleDeleteHook = useCallback(async (hook: HookEntry) => {
    try {
      await invoke("delete_hook", {
        cli: hook.cli,
        eventType: hook.eventType,
        groupIdx: hook.groupIdx,
        hookIdx: hook.hookIdx,
        path: hook.path,
      });
      refreshHooks();
      addToast("success", "已删除 Hook");
    } catch (e) {
      addToast("error", `删除失败: ${String(e).slice(0, 120)}`);
    }
  }, [addToast, refreshHooks]);

  const handleAddHook = useCallback(() => {
    setHookWizardOpen(true);
  }, []);

  const handleOpenHookStore = useCallback(() => {
    setHookStoreOpen(true);
  }, []);

  /** Execute the actual move after (optional) overwrite check */
  const doMoveHook = useCallback(async (hook: HookEntry, toScope: "user" | "project", toPath: string, name: string, targetLabel: string) => {
    try {
      await invoke("move_hook_entry", {
        cli: hook.cli, eventType: hook.eventType,
        groupIdx: hook.groupIdx, hookIdx: hook.hookIdx,
        fromPath: hook.path, toPath,
        fromScope: "project", toScope,
      });
      refreshHooks();
      addToast("success", `已移动 "${name}" → ${targetLabel}`);
    } catch (e) {
      addToast("error", `移动 Hook 失败: ${String(e).slice(0, 120)}`);
    }
  }, [addToast, refreshHooks]);

  /** Execute the actual copy after (optional) overwrite check */
  const doCopyHook = useCallback(async (hook: HookEntry, toScope: "user" | "project", toPath: string, name: string, targetLabel: string) => {
    try {
      await invoke("copy_hook_entry", {
        cli: hook.cli, eventType: hook.eventType,
        groupIdx: hook.groupIdx, hookIdx: hook.hookIdx,
        fromPath: hook.path, toPath,
      });
      refreshHooks();
      addToast("success", `已复制 "${name}" → ${targetLabel}`);
    } catch (e) {
      addToast("error", `复制 Hook 失败: ${String(e).slice(0, 120)}`);
    }
  }, [addToast, refreshHooks]);

  const handleMoveHook = useCallback(async (hook: HookEntry, toScope: "user" | "project", targetProject?: ProjectInfo) => {
    const toPath = getHookTargetPath(hook.cli, toScope, targetProject?.path);
    if (!toPath) { addToast("error", "无法确定目标路径"); return; }
    const name = hook.name || hook.matcher || hook.eventType;
    const targetLabel = toScope === "user" ? "用户级" : "项目级";

    // Scan target to check if an identical hook already exists
    const hasDuplicate = await checkDuplicateAtTarget(hook, toScope, targetProject);
    if (hasDuplicate) {
      setHookOverwriteConfirm({
        hookName: name,
        targetLabel,
        onConfirm: () => {
          setHookOverwriteConfirm(null);
          doMoveHook(hook, toScope, toPath, name, targetLabel);
        },
      });
      return;
    }

    await doMoveHook(hook, toScope, toPath, name, targetLabel);
  }, [addToast, getHookTargetPath, checkDuplicateAtTarget, doMoveHook]);

  const handleCopyHook = useCallback(async (hook: HookEntry, toScope: "user" | "project", targetProject?: ProjectInfo) => {
    const toPath = getHookTargetPath(hook.cli, toScope, targetProject?.path);
    if (!toPath) { addToast("error", "无法确定目标路径"); return; }
    const name = hook.name || hook.matcher || hook.eventType;
    const targetLabel = toScope === "user" ? "用户级" : "项目级";

    // Scan target to check if an identical hook already exists
    const hasDuplicate = await checkDuplicateAtTarget(hook, toScope, targetProject);
    if (hasDuplicate) {
      setHookOverwriteConfirm({
        hookName: name,
        targetLabel,
        onConfirm: () => {
          setHookOverwriteConfirm(null);
          doCopyHook(hook, toScope, toPath, name, targetLabel);
        },
      });
      return;
    }

    await doCopyHook(hook, toScope, toPath, name, targetLabel);
  }, [addToast, getHookTargetPath, checkDuplicateAtTarget, doCopyHook]);

  const handleBatchMoveHooks = useCallback(async (hooks: HookEntry[], toScope: "user" | "project", targetProject?: ProjectInfo) => {
    let ok = 0;
    let failed = 0;
    // Process in reverse to avoid index drift
    const sorted = [...hooks].sort((a, b) => {
      if (a.path !== b.path) return a.path.localeCompare(b.path);
      if (a.groupIdx !== b.groupIdx) return b.groupIdx - a.groupIdx;
      return b.hookIdx - a.hookIdx;
    });
    for (const hook of sorted) {
      try {
        const toPath = getHookTargetPath(hook.cli, toScope, targetProject?.path);
        if (!toPath) { failed++; continue; }
        await invoke("move_hook_entry", {
          cli: hook.cli, eventType: hook.eventType,
          groupIdx: hook.groupIdx, hookIdx: hook.hookIdx,
          fromPath: hook.path, toPath,
          fromScope: "project", toScope,
        });
        ok++;
      } catch { failed++; }
    }
    refreshHooks();
    if (failed === 0) addToast("success", `已移动 ${ok} 个 Hook → ${toScope === "project" ? "项目级" : "用户级"}`);
    else addToast("error", `移动完成: ${ok} 成功, ${failed} 失败`);
  }, [addToast, refreshHooks, getHookTargetPath]);

  const handleBatchCopyHooks = useCallback(async (hooks: HookEntry[], toScope: "user" | "project", targetProject?: ProjectInfo) => {
    let ok = 0;
    let failed = 0;
    for (const hook of hooks) {
      try {
        const toPath = getHookTargetPath(hook.cli, toScope, targetProject?.path);
        if (!toPath) { failed++; continue; }
        await invoke("copy_hook_entry", {
          cli: hook.cli, eventType: hook.eventType,
          groupIdx: hook.groupIdx, hookIdx: hook.hookIdx,
          fromPath: hook.path, toPath,
        });
        ok++;
      } catch { failed++; }
    }
    refreshHooks();
    if (failed === 0) addToast("success", `已复制 ${ok} 个 Hook → ${toScope === "project" ? "项目级" : "用户级"}`);
    else addToast("error", `复制完成: ${ok} 成功, ${failed} 失败`);
  }, [addToast, refreshHooks, getHookTargetPath]);

  // ── File tree state ──
  const [selectedFile, setSelectedFile] = useState<FileTreeNode | null>(null);
  const [fileContent, setFileContent] = useState<string>("");
  const [treeWidth, setTreeWidth] = useState(240);

  // Compute selectedId for ResourceList highlighting
  const selectedId = (() => {
    if (!selectedResource) return null;
    const item = selectedResource;
    if (item.type === "plugin-skill" || item.type === "plugin-agent" ||
        item.type === "command" || item.type === "plugin-command") return null;
    if (item.type === "plugin") return item.data.id;
    if (item.type === "standalone" || item.type === "system") return item.data.id;
    if (item.type === "agent") return item.data.id;
    return null;
  })();

  // Load overview data: immediate on tab switch, debounced 300ms on project change.
  // When project changes, immediately clear stale data so the user sees a loading
  // skeleton instead of the previous project's overview during the debounce window.
  useEffect(() => {
    if (activeTab !== "overview") return;
    const pathChanged = settledPathRef.current !== project.path;
    settledPathRef.current = project.path;

    if (pathChanged) {
      setOverviewData(null);
      setOverviewLoading(true);
      if (dataDebounceRef.current) clearTimeout(dataDebounceRef.current);
      dataDebounceRef.current = setTimeout(loadOverviewData, 300);
    } else {
      loadOverviewData();
    }
    return () => {
      if (dataDebounceRef.current) clearTimeout(dataDebounceRef.current);
    };
  }, [activeTab, project.path, loadOverviewData]);

  // Load sessions: debounced 300ms on project change to avoid race conditions
  // during rapid arrow-key navigation. scanSessions is async (Tauri IPC) and
  // internally clears stale data when project path changes + has 30s cache.
  useEffect(() => {
    if (activeTab !== "sessions") return;
    const pathChanged = settledPathRef.current !== project.path;
    settledPathRef.current = project.path;

    if (pathChanged) {
      if (dataDebounceRef.current) clearTimeout(dataDebounceRef.current);
      dataDebounceRef.current = setTimeout(() => onScanSessions(project.path), 300);
    } else {
      onScanSessions(project.path);
    }
    return () => {
      if (dataDebounceRef.current) clearTimeout(dataDebounceRef.current);
    };
  }, [activeTab, project.path, onScanSessions]);

  // Reset file state when project changes
  useEffect(() => {
    setSelectedFile(null);
    setFileContent("");
  }, [project.path]);

  // Load file content when selected file changes
  useEffect(() => {
    if (!selectedFile || selectedFile.is_dir) {
      setFileContent("");
      return;
    }
    invoke<string>("read_file", { path: selectedFile.path })
      .then(setFileContent)
      .catch(() => setFileContent(""));
  }, [selectedFile]);

  // File tree resize — horizontal drag
  const handleTreeResize = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    const startX = e.clientX;
    const startWidth = treeWidth;
    const onMouseMove = (ev: MouseEvent) => {
      const delta = ev.clientX - startX;
      setTreeWidth(Math.max(160, Math.min(600, startWidth + delta)));
    };
    const onMouseUp = () => {
      document.removeEventListener("mousemove", onMouseMove);
      document.removeEventListener("mouseup", onMouseUp);
    };
    document.addEventListener("mousemove", onMouseMove);
    document.addEventListener("mouseup", onMouseUp);
  }, [treeWidth]);

  return (
    <div className="flex-1 flex flex-col min-h-0 bg-[var(--app-bg)]">
      <div className="h-[44px] shrink-0 flex items-center gap-2 px-3 border-b border-app-border">
        {/* Left: project info — takes remaining space, truncates */}
        <div className="flex-1 min-w-0 flex items-center gap-2">
          <span className="text-sm font-mono text-app-text truncate">{project.name}</span>
          <span className="text-2xs font-mono text-app-text-muted truncate">{project.path}</span>
        </div>

        {/* Right: controls */}
        <div className="shrink-0 flex items-center gap-2">
          {/* Separator */}
          <div className="w-px h-5 bg-app-border" />

          {/* Default profile */}
          <div ref={headerDefaultRef} className="relative">
            <button
              onClick={() => { setShowHeaderDefaultPicker((v) => !v); setShowHeaderRunPicker(false); }}
              className="h-7 w-[180px] flex items-center gap-2 px-2 border border-app-border
                bg-app-sidebar text-xs font-mono hover:bg-[var(--app-hover)] transition-colors"
              title={defaultProfile ? `默认 Profile: ${defaultProfile}` : "设置默认 Profile"}
            >
              <span className="text-app-text-muted shrink-0">默认</span>
              {defaultProfile ? (
                <span className="text-app-accent truncate flex-1 text-left">{defaultProfile}</span>
              ) : (
                <span className="text-app-text-dim truncate flex-1 text-left">未设置</span>
              )}
              <span className="text-app-text-dim shrink-0">▾</span>
            </button>
            {showHeaderDefaultPicker && profiles.length > 0 && (
              <div
                className="absolute right-0 top-full mt-1 w-52 bg-app-sidebar border border-app-border
                  shadow-lg z-30 max-h-60 overflow-y-auto"
                onKeyDown={(e) => handleHeaderPickerKeyDown(e, "default")}
              >
                {profiles.map((p, i) => (
                  <button
                    key={p.name}
                    onClick={() => handleHeaderSelectDefault(p)}
                    className={`w-full flex items-center gap-1.5 px-2.5 py-1 text-left text-2xs font-mono
                      transition-colors duration-fast
                      ${i === headerFocusedIdx ? "bg-[var(--app-accent)]/10 text-app-text" : "text-app-text-dim hover:bg-[var(--app-hover)]"}
                      ${p.name === defaultProfile ? "bg-[var(--app-accent)]/5" : ""}`}
                  >
                    <CliBadge cli={p.cli_type || "claude"} />
                    <span className="flex-1 truncate">{p.name}</span>
                  </button>
                ))}
              </div>
            )}
          </div>

          {/* Run split button */}
          <div className="flex items-stretch h-7">
            <div className="relative group/run">
              <button
                onClick={handleHeaderRunDefault}
                className="h-7 flex items-center gap-1.5 px-3 text-xs font-mono
                  bg-app-accent text-[var(--app-bg)] hover:opacity-90 transition-opacity"
                title={defaultProfile ? `Run with ${defaultProfile}` : "Select profile"}
              >
                <span>▶</span>
                <span>Run</span>
              </button>
              <div className="absolute top-full left-1/2 -translate-x-1/2 mt-1.5 px-2 py-1
                bg-[var(--app-panel)] text-[var(--app-text)] text-2xs
                border border-[var(--app-border)] shadow-dialog
                whitespace-nowrap pointer-events-none
                opacity-0 group-hover/run:opacity-100
                transition-opacity duration-150 delay-700
                group-hover/run:delay-700">
                <span>在终端中运行</span>
                <span className="text-[var(--app-text-muted)] ml-1">{navigator.userAgent.includes("Mac") ? "⌥+Click" : "Alt+Click"} 分屏运行</span>
              </div>
            </div>
            <div ref={headerRunRef} className="relative">
              <button
                onClick={() => { setShowHeaderRunPicker((v) => !v); setShowHeaderDefaultPicker(false); }}
                className="h-7 px-1.5 text-xs font-mono
                  bg-app-accent text-[var(--app-bg)] hover:opacity-90 transition-opacity
                  border-l border-[var(--app-bg)]/20"
              >
                ▾
              </button>
              {showHeaderRunPicker && profiles.length > 0 && (
                <div
                  className="absolute right-0 top-full mt-1 w-52 bg-app-sidebar border border-app-border
                    shadow-lg z-30 max-h-60 overflow-y-auto"
                  onKeyDown={(e) => handleHeaderPickerKeyDown(e, "run")}
                >
                  {profiles.map((p, i) => (
                    <button
                      key={p.name}
                      onClick={() => handleHeaderSelectProfile(p)}
                      className={`w-full flex items-center gap-1.5 px-2.5 py-1 text-left text-2xs font-mono
                        transition-colors duration-fast
                        ${i === headerFocusedIdx ? "bg-[var(--app-accent)]/10 text-app-text" : "text-app-text-dim hover:bg-[var(--app-hover)]"}
                        ${p.name === defaultProfile ? "bg-[var(--app-accent)]/5" : ""}`}
                    >
                      <CliBadge cli={p.cli_type || "claude"} />
                      <span className="flex-1 truncate">{p.name}</span>
                    </button>
                  ))}
                </div>
              )}
            </div>
          </div>
        </div>
      </div>
      <div className="flex shrink-0 border-b border-app-border px-2 overflow-x-auto" role="tablist">
        {TABS.map((tab) => (
          <button
            key={tab.key}
            role="tab"
            aria-selected={activeTab === tab.key}
            onClick={() => setActiveTab(tab.key)}
            className={`px-3 py-2 text-2xs font-mono border-b whitespace-nowrap ${
              activeTab === tab.key
                ? "text-app-accent border-app-accent"
                : "text-app-text-muted border-transparent hover:text-app-text"
            }`}
          >
            {tab.label}
          </button>
        ))}
      </div>
      {activeTab === "overview" && (
        <ProjectOverview
          project={project}
          overviewData={overviewData}
          overviewLoading={overviewLoading}
          profiles={profiles}
          onResumeSession={onResumeSession}
          onRunProfile={onRunProfile}
          onSplitProfile={onSplitProfile}
          onSetDefaultProfile={onSetDefaultProfile}
        />
      )}
      {activeTab === "sessions" && (
        <div className="flex-1 min-h-0">
          <SessionList
            sessions={sessions}
            loading={sessionsLoading}
            onResume={onResumeSession}
          />
        </div>
      )}
      {activeTab !== "overview" && activeTab !== "sessions" && activeTab !== "files" && activeTab !== "resource" && activeTab !== "hooks" && (
        <div className="flex-1 flex items-center justify-center text-xs font-mono text-app-text-muted">
          {/* fallback for unknown tabs */}
        </div>
      )}
      {activeTab === "hooks" && (
        <div className="flex-1 flex min-h-0">
          {/* Left: HookList */}
          <div className="w-[300px] shrink-0 border-r border-[var(--app-border)]">
            <HookList
              data={hookData}
              loading={hookLoading}
              selectedId={selectedHook?.id ?? null}
              onSelect={setSelectedHook}
              onAddHook={handleAddHook}
              onOpenStore={handleOpenHookStore}
              onRefresh={refreshHooks}
              onToggleHook={handleToggleHook}
              onDeleteHook={handleDeleteHook}
              onMoveHook={handleMoveHook}
              onCopyHook={handleCopyHook}
              onBatchMoveHooks={handleBatchMoveHooks}
              onBatchCopyHooks={handleBatchCopyHooks}
              activeScope="project"
              onScopeChange={() => {}}
              projects={projects}
              activeProject={project}
              onProjectChange={() => {}}
              onAddProject={onAddProject}
              hideScopeTabs
            />
          </div>
          {/* Right: HookDetail */}
          <div className="flex-1 min-h-0 overflow-y-auto flex flex-col">
            <HookDetail
              hook={selectedHook}
              onRefresh={refreshHooks}
              scope="project"
            />
          </div>
        </div>
      )}
      {activeTab === "resource" && (
        <div className="flex-1 flex min-h-0">
          {/* Left: ResourceList */}
          <div className="w-[300px] shrink-0 border-r border-[var(--app-border)]">
            <ResourceList
              data={resourceData}
              agentData={agentData}
              loading={resourceLoading}
              selectedId={selectedId}
              onSelect={setSelectedResource}
              onTogglePlugin={handleTogglePlugin}
              onToggleStandaloneSkill={handleToggleStandaloneSkill}
              onBatchToggle={handleBatchToggle}
              onBatchUninstall={handleBatchUninstall}
              onDeleteAgent={handleDeleteAgent}
              checkingUpdates={checkingUpdates}
              updateInfos={updateInfos}
              onCheckUpdates={handleCheckUpdates}
              onCancelCheckUpdates={handleCancelCheckUpdates}
              onOpenMarketplace={() => setMarketplaceOpen(true)}
              activeScope="project"
              onScopeChange={() => {}}
              projects={projects}
              activeProject={project}
              onProjectChange={() => {}}
              baselineUserCount={0}
              baselineProjectCount={0}
              onAddProject={onAddProject}
              onMoveResource={handleMoveResource}
              onCopyResource={handleCopyResource}
              onBatchMove={handleBatchMove}
              onBatchCopy={handleBatchCopy}
              onToast={addToast}
              hideProjectFeatures
            />
          </div>
          {/* Right: ResourceDetail panel */}
          <div className="flex-1 overflow-y-auto flex flex-col bg-[var(--app-bg)]">
            <ResourceDetail
              item={selectedResource}
              data={resourceData}
              graphData={null}
              onTogglePlugin={handleTogglePlugin}
              onToggleStandaloneSkill={handleToggleStandaloneSkill}
              updateInfos={updateInfos}
              onUpdatePlugin={handleUpdatePlugin}
              onUninstallPlugin={handleUninstallPlugin}
              onUninstallStandaloneSkill={handleUninstallStandaloneSkill}
              onToggleAgent={handleToggleAgent}
              onDeleteAgent={handleDeleteAgent}
              onToggleCommand={handleToggleCommand}
              onUninstallCommand={handleUninstallCommand}
              onSelect={setSelectedResource}
              scope="project"
            />
          </div>
        </div>
      )}
      {activeTab === "files" && (
        <div className="flex-1 flex overflow-hidden min-h-0">
          <div
            className="shrink-0 border-r border-[var(--app-border)] overflow-y-auto"
            style={{ width: treeWidth }}
          >
            <FileTree
              key={project.path}
              rootPath={project.path}
              onSelect={setSelectedFile}
              activePath={selectedFile?.path}
            />
          </div>
          {/* Resize handle */}
          <div
            className="w-[5px] shrink-0 cursor-col-resize hover:bg-[var(--app-accent)]/20 transition-colors duration-fast group/resize flex items-center justify-center"
            onMouseDown={handleTreeResize}
          >
            <div className="w-px h-full bg-[var(--app-border)] group-hover/resize:bg-[var(--app-accent)]/50" />
          </div>
          <div className="flex-1 overflow-y-auto">
            {selectedFile && !selectedFile.is_dir ? (
              <FileContentBlock
                content={fileContent}
                filePath={selectedFile.path}
              />
            ) : (
              <div className="flex items-center justify-center h-full text-xs text-[var(--app-text-muted)] font-mono">
                选择文件以预览
              </div>
            )}
          </div>
        </div>
      )}
      {/* HookWizard modal */}
      <HookWizard
        open={hookWizardOpen}
        onClose={() => setHookWizardOpen(false)}
        onCreated={() => {
          setHookWizardOpen(false);
          refreshHooks();
        }}
      />

      {/* HookStore modal */}
      <HookStore
        open={hookStoreOpen}
        onClose={() => setHookStoreOpen(false)}
        onInstalled={() => {
          setHookStoreOpen(false);
          refreshHooks();
        }}
      />

      {/* MarketplaceBrowser — project-scoped */}
      <MarketplaceBrowser
        open={marketplaceOpen}
        onClose={() => setMarketplaceOpen(false)}
        onInstalled={handleMarketplaceInstalled}
        projectPath={project.path}
        projectName={project.name}
      />

      {/* Overwrite confirmation dialog (shared hook) */}
      {overwriteDialog}

      {/* Hook overwrite confirmation dialog */}
      {hookOverwriteConfirm && (
        <ConfirmDialog
          open={true}
          title="覆盖确认"
          message={`${hookOverwriteConfirm.targetLabel}已存在 "${hookOverwriteConfirm.hookName}"，是否覆盖？\n\n覆盖会用当前 Hook 替换目标位置的同配置 Hook。`}
          confirmLabel="覆盖"
          onConfirm={() => hookOverwriteConfirm.onConfirm()}
          onCancel={() => setHookOverwriteConfirm(null)}
        />
      )}
    </div>
  );
}
