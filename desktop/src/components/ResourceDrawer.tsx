import React, { useState, useEffect, useCallback, useRef, useMemo } from "react";
import { useResizeHandle } from "../hooks/useResizeHandle";
import { Puzzle, Terminal, X } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ResourceList } from "./ResourceList";
import { ResourceDetail } from "./ResourceDetail";
import type {
  SelectedItem,
  ResourceScanData,
  AgentManagerData,
  PluginUpdateInfo,
  BatchToggleItem,
} from "./ResourceList";
import type { CliKind } from "../lib/types";
import { HookList, type HookManagerData } from "./HookList";
import { HookDetail, type HookEntry } from "./HookDetail";
import { HookWizard } from "./HookWizard";
import { HookStore } from "./HookStore";
import { ToastViewport } from "./ToastViewport";
import { useToasts } from "../hooks/useToasts";
import { useOverwriteConfirm, describeOverwrite } from "../hooks/useOverwriteConfirm";
import { useProjects } from "../hooks/useProjects";
import type { ProjectInfo } from "../lib/types";
import { getResourceData, getResourceType, getSubdir, buildDestDir, type ResourceData } from "../lib/resource-transfer";

/* ──────────────────── Props ──────────────────── */

interface ResourceDrawerProps {
  open: boolean;
  onClose: () => void;
  onOpenMarketplace?: () => void;
}

/** Scan user-level skills + agents only (no project resources in Resource drawer). */
async function scanResources(): Promise<{ skills: ResourceScanData; agents: AgentManagerData }> {
  const [skills, agents] = await Promise.all([
    invoke<ResourceScanData>("scan_skills", { projectPath: null }),
    invoke<AgentManagerData>("scan_agents", { projectPath: null }),
  ]);
  return {
    skills: {
      plugins: skills.plugins.filter((p) => p.source !== "project" && !p.id.includes(":project-")),
      standaloneSkills: skills.standaloneSkills.filter((s) => !s.id.includes(":project-") && !s.projectName),
      systemSkills: skills.systemSkills,
      commands: (skills.commands || []).filter((c) => !c.id.includes(":project-") && !c.projectName),
    },
    agents: {
      agents: agents.agents.filter((a) => a.source !== "project" && !a.projectName),
    },
  };
}

type ResourceTab = "resources" | "hooks";

export function ResourceDrawer({ open, onClose, onOpenMarketplace }: ResourceDrawerProps) {
  const { toasts, addToast, setToasts, toastIdRef, dismissToast } = useToasts();
  const { projects, addProject } = useProjects();

  /* ── Tab state ── */
  const [activeTab, setActiveTab] = useState<ResourceTab>("resources");

  /* ── Resources state ── */
  const [skillData, setSkillData] = useState<ResourceScanData | null>(null);
  const [agentData, setAgentData] = useState<AgentManagerData | null>(null);
  const [loading, setLoading] = useState(false);
  const [selectedItem, setSelectedItem] = useState<SelectedItem | null>(null);
  const [checkingUpdates, setCheckingUpdates] = useState(false);
  const [updateInfos, setUpdateInfos] = useState<PluginUpdateInfo[]>([]);

  /* ── Hooks state ── */
  const [hookData, setHookData] = useState<HookManagerData | null>(null);
  const [hookLoading, setHookLoading] = useState(false);
  const [selectedHook, setSelectedHook] = useState<HookEntry | null>(null);
  const [hookWizardOpen, setHookWizardOpen] = useState(false);
  const [hookStoreOpen, setHookStoreOpen] = useState(false);

  /* ── Resizable width ── */
  const maxWidth = useMemo(() => Math.round(window.innerWidth * 0.92), []);
  const { size: drawerWidth, handleProps } = useResizeHandle({
    direction: "horizontal",
    minSize: 480,
    maxSize: maxWidth,
    defaultSize: 1080,
    storageKey: "kn-resource-drawer-width",
  });

  // Track whether update check was initiated by THIS component — only then show toast.
  const checkRequestRef = useRef(false);
  const checkSilentRef = useRef(false);
  // Overwrite confirmation (shared hook)
  const { requestOverwrite, overwriteDialog } = useOverwriteConfirm();

  /* ── Sync selectedItem with fresh data after toggle/delete/move ── */
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

  useEffect(() => {
    if (skillData) {
      setSelectedItem((prev) => syncSelection(skillData, prev));
    }
  }, [skillData, syncSelection]);

  useEffect(() => {
    if (agentData) {
      setSelectedItem((prev) => syncAgentSelection(agentData, prev));
    }
  }, [agentData, syncAgentSelection]);

  /* ── Data loading ── */
  const loadData = useCallback(async () => {
    setLoading(true);
    setHookLoading(true);
    try {
      const [{ skills, agents }, hookResult] = await Promise.all([
        scanResources(),
        invoke<HookManagerData>("scan_hooks", { projectPath: null }).catch(() => null),
      ]);
      setSkillData(skills);
      setAgentData(agents);
      setHookData(hookResult);
    } catch (e) {
      setSkillData(null);
      setAgentData(null);
      setHookData(null);
    } finally {
      setLoading(false);
      setHookLoading(false);
    }
  }, []);

  const refreshHooks = useCallback(async () => {
    try {
      const result = await invoke<HookManagerData>("scan_hooks", { projectPath: null });
      setHookData(result);
      setSelectedHook((prev) => {
        if (!prev) return null;
        return result.hooks.find((h) => h.id === prev.id) || null;
      });
    } catch { /* ignore */ }
  }, []);

  // Load when drawer opens, clear when closed
  useEffect(() => {
    if (open) {
      loadData();
      // Reset to resources tab on open
      setActiveTab("resources");
      // Also trigger update check on open (silent)
      checkRequestRef.current = true;
      checkSilentRef.current = true;
      setCheckingUpdates(true);
      setUpdateInfos([]);
      invoke("check_updates").catch(() => { setCheckingUpdates(false); checkRequestRef.current = false; });
    } else {
      setSelectedItem(null);
      setSelectedHook(null);
    }
  }, [open, loadData]);

  // Listen for update-check-complete event
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
  }, []);

  // Listen for update-plugin-complete event
  useEffect(() => {
    const unlisten = listen<{ pluginId: string; success: boolean; message: string }>(
      "update-plugin-complete",
      async (event) => {
        const { success, message } = event.payload;
        if (success) {
          addToast("success", message);
          const { skills, agents } = await scanResources();
          setSkillData(skills);
          setAgentData(agents);
        } else {
          addToast("error", message);
        }
      },
    );
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  /* ── Handler functions ── */

  const handleTogglePlugin = useCallback(
    async (cli: string, pluginId: string, enabled: boolean) => {
      try {
        await invoke("toggle_plugin", { cli, pluginId, enabled });
        setLoading(true);
        const { skills, agents } = await scanResources();
        setSkillData(skills);
        setAgentData(agents);
        setLoading(false);
      } catch (e) {
        addToast("error", `操作失败: ${e}`);
      }
    },
    [addToast],
  );

  const handleToggleStandaloneSkill = useCallback(
    async (cli: string, skillId: string, enabled: boolean, path?: string) => {
      try {
        await invoke("toggle_standalone_skill", { cli, skillId, enabled, path: path ?? null });
        setLoading(true);
        const { skills, agents } = await scanResources();
        setSkillData(skills);
        setAgentData(agents);
        setLoading(false);
      } catch (e) {
        addToast("error", `操作失败: ${e}`);
      }
    },
    [addToast],
  );

  const handleToggleAgent = useCallback(
    async (cli: string, name: string, enabled: boolean, path?: string) => {
      try {
        await invoke("toggle_agent", { cli, name, enabled, path: path ?? null });
        setLoading(true);
        const { skills, agents } = await scanResources();
        setSkillData(skills);
        setAgentData(agents);
        setLoading(false);
      } catch (e) {
        addToast("error", `操作失败: ${e}`);
      }
    },
    [addToast],
  );

  const handleDeleteAgent = useCallback(
    async (cli: string, name: string, path?: string) => {
      try {
        await invoke("delete_agent", { cli, name, path: path ?? null });
        setLoading(true);
        const { skills, agents } = await scanResources();
        setSkillData(skills);
        setAgentData(agents);
        setSelectedItem(null);
        setLoading(false);
        addToast("success", `已删除 Agent "${name}"`);
      } catch (e) {
        addToast("error", `删除失败: ${e}`);
      }
    },
    [addToast],
  );

  const handleBatchToggle = useCallback(
    async (items: BatchToggleItem[], enabled: boolean) => {
      try {
        for (const item of items) {
          if (item.id.includes(":plugin:")) {
            await invoke("toggle_plugin", { cli: item.cli, pluginId: item.id, enabled });
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
        setLoading(true);
        const { skills, agents } = await scanResources();
        setSkillData(skills);
        setAgentData(agents);
        setLoading(false);
      } catch (e) {
        addToast("error", `批量操作失败: ${e}`);
      }
    },
    [addToast],
  );

  const handleBatchUninstall = useCallback(
    async (items: BatchToggleItem[]) => {
      try {
        for (const item of items) {
          const isCommand = item.id.includes(":command:") || item.id.includes("-command:");
          if (item.id.includes(":plugin:")) {
            await invoke("uninstall_plugin", { cli: item.cli, pluginId: item.id });
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
        setLoading(true);
        const { skills, agents } = await scanResources();
        setSkillData(skills);
        setAgentData(agents);
        setSelectedItem(null);
        setLoading(false);
        addToast("success", `已删除 ${items.length} 项`);
      } catch (e) {
        addToast("error", `批量删除失败: ${e}`);
      }
    },
    [addToast],
  );

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
    try {
      await invoke("cancel_check_updates");
    } catch { /* ignore */ }
  }, []);

  const handleUpdatePlugin = useCallback(async (cli: string, pluginId: string) => {
    try {
      await invoke("update_plugin", { cli, pluginId });
      addToast("success", "正在后台更新...");
    } catch (e) {
      addToast("error", `更新失败: ${e}`);
    }
  }, [addToast]);

  const handleUninstallPlugin = useCallback(
    async (cli: CliKind, pluginId: string) => {
      try {
        await invoke("uninstall_plugin", { cli, pluginId });
        const { skills, agents } = await scanResources();
        setSkillData(skills);
        setAgentData(agents);
        setSelectedItem(null);
        addToast("success", "已卸载插件");
      } catch (e) {
        addToast("error", `卸载失败: ${e}`);
      }
    },
    [addToast],
  );

  const handleUninstallStandaloneSkill = useCallback(
    async (cli: CliKind, skillId: string, path?: string, _name?: string) => {
      try {
        const name = skillId.split(":").pop() || skillId;
        await invoke("uninstall_standalone_skill", { cli, skillId, skillPath: path ?? null, skillName: name });
        const { skills, agents } = await scanResources();
        setSkillData(skills);
        setAgentData(agents);
        setSelectedItem(null);
        addToast("success", "已删除");
      } catch (e) {
        addToast("error", `删除失败: ${e}`);
      }
    },
    [addToast],
  );

  const handleToggleCommand = useCallback(
    async (cli: CliKind, name: string, enabled: boolean, path?: string) => {
      try {
        await invoke("toggle_command", { cli, name, enabled, path: path ?? null });
        setLoading(true);
        const { skills } = await scanResources();
        setSkillData(skills);
        setLoading(false);
      } catch (e) {
        addToast("error", `操作失败: ${e}`);
      }
    },
    [addToast],
  );

  const handleUninstallCommand = useCallback(
    async (cli: CliKind, name: string, path?: string) => {
      try {
        await invoke("uninstall_command", { cli, name, path: path ?? null });
        const { skills } = await scanResources();
        setSkillData(skills);
        setSelectedItem(null);
        addToast("success", "已删除命令");
      } catch (e) {
        addToast("error", `删除失败: ${e}`);
      }
    },
    [addToast],
  );

  /* ── Move / Copy ── */

  const scanAndSet = useCallback(async () => {
    const { skills, agents } = await scanResources();
    setSkillData(skills);
    setAgentData(agents);
  }, []);

  const handleMoveResource = useCallback(
    async (item: SelectedItem, toScope: "user" | "project", targetProject?: ProjectInfo) => {
      try {
        const data = getResourceData(item);
        const resourceType = getResourceType(item);
        const subdir = getSubdir(resourceType);
        const srcPath = data.path as string;
        const fromScope = data.id.includes(":project-") ? "project" : "user";
        const project = toScope === "project" ? targetProject : undefined;

        if (toScope === "project" && !project) { addToast("error", "请先选择目标项目"); return; }

        const destDir = await buildDestDir(srcPath, data.cli, toScope, subdir, project);

        let undoInfo;
        try {
          undoInfo = await invoke<any>("move_skill_file", {
            sourcePath: data.path, destDir, resourceName: data.name,
            resourceType, fromScope, toScope, overwrite: false,
          });
        } catch (e) {
          const msg = String(e);
          if (msg.includes("同名资源")) {
            const confirmed = await requestOverwrite(describeOverwrite(data.name));
            if (!confirmed) return;
            undoInfo = await invoke<any>("move_skill_file", {
              sourcePath: data.path, destDir, resourceName: data.name,
              resourceType, fromScope, toScope, overwrite: true,
            });
          } else {
            throw e;
          }
        }

        await scanAndSet();

        setToasts((prev) => {
          const id = ++toastIdRef.current;
          return [...prev, {
            id,
            type: "success" as const,
            message: `已移动 "${data.name}" → ${toScope === "project" ? "项目级" : "用户级"}`,
            undoAction: async () => {
              await invoke("undo_move_skill", {
                backupPath: undoInfo.backupPath,
                originalPath: undoInfo.originalPath,
                destPath: undoInfo.destPath,
                contentFingerprint: undoInfo.contentFingerprint,
              });
              await scanAndSet();
            },
          } as any];
        });
      } catch (e) {
        const msg = String(e).slice(0, 120);
        if (msg !== "用户取消") addToast("error", `移动失败: ${msg}`);
      }
    },
    [addToast, setToasts, toastIdRef, requestOverwrite, scanAndSet],
  );

  const handleCopyResource = useCallback(
    async (item: SelectedItem, toScope: "user" | "project", targetProject?: ProjectInfo) => {
      try {
        const data = getResourceData(item);
        const srcPath = data.path as string;
        const resourceType = getResourceType(item);
        const subdir = getSubdir(resourceType);
        const project = toScope === "project" ? targetProject : undefined;

        if (toScope === "project" && !project) { addToast("error", "请先选择目标项目"); return; }

        const destDir = await buildDestDir(srcPath, data.cli, toScope, subdir, project);

        try {
          await invoke("copy_skill_file", { sourcePath: data.path, destDir, resourceName: data.name, overwrite: false });
        } catch (e) {
          const msg = String(e);
          if (msg.includes("同名资源")) {
            const confirmed = await requestOverwrite(describeOverwrite(data.name));
            if (!confirmed) return;
            await invoke("copy_skill_file", { sourcePath: data.path, destDir, resourceName: data.name, overwrite: true });
          } else {
            throw e;
          }
        }

        await scanAndSet();
        addToast("success", `已复制 "${data.name}" → ${toScope === "project" ? "项目级" : "用户级"}`);
      } catch (e) {
        const msg = String(e).slice(0, 120);
        if (msg !== "用户取消") addToast("error", `复制失败: ${msg}`);
      }
    },
    [addToast, requestOverwrite, scanAndSet],
  );

  const handleBatchMove = useCallback(
    async (items: SelectedItem[], toScope: "user" | "project", targetProject?: ProjectInfo) => {
      let ok = 0;
      let failed = 0;
      for (const item of items) {
        try {
          const data = getResourceData(item);
          const resourceType = getResourceType(item);
          const subdir = getSubdir(resourceType);
          const srcPath = data.path as string;
          const fromScope = data.id.includes(":project-") ? "project" : "user";
          const project = toScope === "project" ? targetProject : undefined;

          if (toScope === "project" && !project) throw new Error("no project");

          const destDir = await buildDestDir(srcPath, data.cli, toScope, subdir, project);

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
      await scanAndSet();
      if (failed === 0) addToast("success", `已移动 ${ok} 项 → ${toScope === "project" ? "项目级" : "用户级"}`);
      else addToast("error", `移动完成: ${ok} 成功, ${failed} 失败`);
    },
    [addToast, requestOverwrite, scanAndSet],
  );

  const handleBatchCopy = useCallback(
    async (items: SelectedItem[], toScope: "user" | "project", targetProject?: ProjectInfo) => {
      let ok = 0;
      let failed = 0;
      for (const item of items) {
        try {
          const data = getResourceData(item);
          const srcPath = data.path as string;
          const resourceType = getResourceType(item);
          const subdir = getSubdir(resourceType);
          const project = toScope === "project" ? targetProject : undefined;

          if (toScope === "project" && !project) throw new Error("no project");

          const destDir = await buildDestDir(srcPath, data.cli, toScope, subdir, project);

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
      await scanAndSet();
      if (failed === 0) addToast("success", `已复制 ${ok} 项 → ${toScope === "project" ? "项目级" : "用户级"}`);
      else addToast("error", `复制完成: ${ok} 成功, ${failed} 失败`);
    },
    [addToast, requestOverwrite, scanAndSet],
  );

  const handleAddProject = useCallback(async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selectedPath = await open({
        directory: true,
        multiple: false,
        title: "选择项目目录",
      });
      if (!selectedPath) return;
      const { basename } = await import("../lib/path-utils");
      const name = basename(selectedPath as string) || (selectedPath as string);
      await addProject(name, selectedPath as string);
      addToast("success", `项目 "${name}" 已注册`);
    } catch (e) {
      addToast("error", `注册项目失败: ${String(e).slice(0, 120)}`);
    }
  }, [addProject, addToast]);

  const handleOpenMarketplace = useCallback(() => {
    onOpenMarketplace?.();
    // Don't close the drawer — keep it open behind the modal so the user
    // can see installed plugins immediately after closing the marketplace.
  }, [onOpenMarketplace]);

  /* ── Hook operation handlers ── */

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
      addToast("success", `已删除 Hook`);
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

  const handleMoveHook = useCallback(async (hook: HookEntry, toScope: "user" | "project", targetProject?: ProjectInfo) => {
    // In ResourceDrawer, only user→project moves are supported (no project hooks shown)
    if (toScope !== "project" || !targetProject) return;
    try {
      const configFile = hook.cli === "codex" ? "config.toml" : "settings.json";
      const cliDirProject = hook.cli === "qoder" ? ".qoder" : hook.cli === "codex" ? ".codex" : ".claude";
      const toPath = `${targetProject.path.replace(/\/+$/, "")}/${cliDirProject}/${configFile}`;
      await invoke("move_hook_entry", {
        cli: hook.cli, eventType: hook.eventType,
        groupIdx: hook.groupIdx, hookIdx: hook.hookIdx,
        fromPath: hook.path, toPath,
        fromScope: "user", toScope: "project",
      });
      refreshHooks();
      addToast("success", `已移动 "${hook.name || hook.eventType}" → 项目级`);
    } catch (e) {
      addToast("error", `移动 Hook 失败: ${String(e).slice(0, 120)}`);
    }
  }, [addToast, refreshHooks]);

  const handleCopyHook = useCallback(async (hook: HookEntry, toScope: "user" | "project", targetProject?: ProjectInfo) => {
    if (toScope !== "project" || !targetProject) return;
    try {
      const configFile = hook.cli === "codex" ? "config.toml" : "settings.json";
      const cliDirProject = hook.cli === "qoder" ? ".qoder" : hook.cli === "codex" ? ".codex" : ".claude";
      const toPath = `${targetProject.path.replace(/\/+$/, "")}/${cliDirProject}/${configFile}`;
      await invoke("copy_hook_entry", {
        cli: hook.cli, eventType: hook.eventType,
        groupIdx: hook.groupIdx, hookIdx: hook.hookIdx,
        fromPath: hook.path, toPath,
      });
      refreshHooks();
      addToast("success", `已复制 "${hook.name || hook.eventType}" → 项目级`);
    } catch (e) {
      addToast("error", `复制 Hook 失败: ${String(e).slice(0, 120)}`);
    }
  }, [addToast, refreshHooks]);

  /* ── Render ── */
  if (!open) return null;

  return (
    <div className="fixed inset-0 z-40 flex justify-end">
      <button
        aria-label="关闭资源管理遮罩"
        className="absolute inset-0 bg-black/45"
        onClick={onClose}
      />
      {/* Resize handle — outside section so internal content can't block it */}
      <div
        {...handleProps}
        className={`absolute top-0 bottom-0 w-2 z-30 hover:bg-app-accent/15 transition-colors ${handleProps.className}`}
        style={{ right: `${drawerWidth}px` }}
      >
        <div className="absolute right-0 top-0 bottom-0 w-px bg-app-border" />
      </div>
      <section
        className="relative z-10 h-full bg-app-bg border-l border-app-border shadow-dialog flex flex-col shrink-0"
        style={{ width: `${drawerWidth}px` }}
      >
        {/* Header */}
        <div className="h-[44px] shrink-0 flex items-center gap-3 px-4 border-b border-app-border bg-app-toolbar">
          <div className="text-sm font-mono text-app-text font-semibold">扩展</div>
          <button
            aria-label="关闭资源管理"
            onClick={onClose}
            className="ml-auto text-app-text-muted hover:text-app-text p-1 hover:bg-[var(--app-hover)] transition-colors"
          >
            <X size={14} />
          </button>
        </div>

        {/* Tab bar */}
        <div className="h-[36px] shrink-0 flex items-center border-b border-app-border bg-app-toolbar px-2" role="tablist">
          <button
            role="tab"
            aria-selected={activeTab === "resources"}
            onClick={() => { setActiveTab("resources"); setSelectedHook(null); }}
            className={`px-3 py-1 text-xs font-mono transition-colors relative flex items-center gap-1.5
              ${activeTab === "resources"
                ? "text-[var(--app-text)]"
                : "text-[var(--app-text-muted)] hover:text-[var(--app-text)]"
              }`}
          >
            <Puzzle size={12} className={activeTab === "resources" ? "text-[var(--app-accent)]" : ""} />
            Resource
            {activeTab === "resources" && (
              <div className="absolute bottom-0 left-0 right-0 h-[2px] bg-[var(--app-accent)]" />
            )}
          </button>
          <button
            role="tab"
            aria-selected={activeTab === "hooks"}
            onClick={() => { setActiveTab("hooks"); setSelectedItem(null); }}
            className={`px-3 py-1 text-xs font-mono transition-colors relative flex items-center gap-1.5
              ${activeTab === "hooks"
                ? "text-[var(--app-text)]"
                : "text-[var(--app-text-muted)] hover:text-[var(--app-text)]"
              }`}
          >
            <Terminal size={12} className={activeTab === "hooks" ? "text-[var(--app-accent)]" : ""} />
            Hooks
            {activeTab === "hooks" && (
              <div className="absolute bottom-0 left-0 right-0 h-[2px] bg-[var(--app-accent)]" />
            )}
          </button>
        </div>

        {/* Tab content */}
        {activeTab === "resources" ? (
        /* Two-column layout: ResourceList + ResourceDetail */
        <div className="flex-1 min-h-0 flex">
          {/* Left: Skill list */}
          <div className="w-[300px] shrink-0 flex flex-col border-r border-[var(--app-border)]">
            <ResourceList
              data={skillData}
              agentData={agentData}
              loading={loading}
              selectedId={
                selectedItem &&
                selectedItem.type !== "plugin-skill" &&
                selectedItem.type !== "plugin-agent" &&
                selectedItem.type !== "command" &&
                selectedItem.type !== "plugin-command"
                  ? (selectedItem.data as any).id ?? null
                  : null
              }
              onSelect={setSelectedItem}
              onTogglePlugin={handleTogglePlugin}
              onToggleStandaloneSkill={handleToggleStandaloneSkill}
              onBatchToggle={handleBatchToggle}
              onBatchUninstall={handleBatchUninstall}
              onDeleteAgent={handleDeleteAgent}
              checkingUpdates={checkingUpdates}
              updateInfos={updateInfos}
              onCheckUpdates={handleCheckUpdates}
              onCancelCheckUpdates={handleCancelCheckUpdates}
              onOpenMarketplace={handleOpenMarketplace}
              onToast={addToast}
              hideProjectFeatures
              activeScope="user"
              onScopeChange={() => {}}
              projects={projects}
              activeProject={null}
              onProjectChange={() => {}}
              onAddProject={handleAddProject}
              onMoveResource={handleMoveResource}
              onCopyResource={handleCopyResource}
              onBatchMove={handleBatchMove}
              onBatchCopy={handleBatchCopy}
            />
          </div>

          {/* Right: Detail panel */}
          <div className="flex-1 min-h-0 overflow-y-auto flex flex-col">
            <ResourceDetail
              item={selectedItem}
              data={skillData}
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
              onSelect={(item) => setSelectedItem(item)}
              scope="user"
            />
          </div>
        </div>
        ) : (
        /* Hooks panel: HookList + HookDetail */
        <div className="flex-1 min-h-0 flex">
          {/* Left: Hook list */}
          <div className="w-[300px] shrink-0 flex flex-col border-r border-[var(--app-border)]">
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
              activeScope="user"
              onScopeChange={() => {}}
              projects={projects}
              activeProject={null}
              onProjectChange={() => {}}
              onAddProject={handleAddProject}
              hideScopeTabs
            />
          </div>

          {/* Right: Hook detail */}
          <div className="flex-1 min-h-0 overflow-y-auto flex flex-col">
            <HookDetail
              hook={selectedHook}
              onRefresh={refreshHooks}
              scope="user"
            />
          </div>
        </div>
        )}
      </section>

      {/* HookWizard modal */}
      <HookWizard
        open={hookWizardOpen}
        onClose={() => setHookWizardOpen(false)}
        onCreated={(_name: string, _description: string) => {
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

      {/* Overwrite confirmation dialog (shared hook) */}
      {overwriteDialog}

      {/* Toast notifications */}
      <ToastViewport toasts={toasts} onDismiss={dismissToast} />
    </div>
  );
}
