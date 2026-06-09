import React, { useEffect, useState, useCallback, useRef, useMemo } from "react";
import { Toolbar } from "./components/Toolbar";
import { Sidebar } from "./components/Sidebar";
import { MainPanel } from "./components/MainPanel";
import { TerminalPanel } from "./components/TerminalPanel";
import { ProfileDialog } from "./components/ProfileDialog";
import { ConfirmDialog } from "./components/ConfirmDialog";
import { NameDialog } from "./components/NameDialog";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { ShortcutsPanel } from "./components/ShortcutsPanel";
import { QuickSwitcher } from "./components/QuickSwitcher";
import { UsagePanel } from "./components/UsagePanel";
import { ActivityBar, type ActivityKey } from "./components/ActivityBar";
import { SkillManager, type SkillManagerData, type SelectedItem, type BatchToggleItem } from "./components/SkillManager";
import type { PluginUpdateInfo } from "./components/SkillManager";
import type { AgentManagerData } from "./components/SkillManager";
import { SkillDetail } from "./components/SkillDetail";
import { DependencyGraph, type DependencyGraphData } from "./components/DependencyGraph";
import { HookList, type HookManagerData } from "./components/HookList";
import { HookDetail, type HookEntry } from "./components/HookDetail";
import { HookWizard } from "./components/HookWizard";
import { HookStore } from "./components/HookStore";
import { MarketplaceBrowser } from "./components/MarketplaceBrowser";
import { AboutDialog } from "./components/AboutDialog";
import { SettingsDialog } from "./components/SettingsDialog";
import { UpdateDialog } from "./components/UpdateDialog";
import { ImportPreview } from "./components/ImportPreview";
import { ScanPreview, ScanProfile } from "./components/ScanPreview";
import { formatShortcut } from "./utils/shortcut";
import { useFontScale } from "./hooks/useFontScale";
import { useProfiles } from "./hooks/useProfiles";
import { useTerminal } from "./hooks/useTerminal";
import { useUsage } from "./hooks/useUsage";
import { useTheme } from "./hooks/useTheme";
import { useProjects } from "./hooks/useProjects";
import type { ScopeTab } from "./lib/types";
import { basename } from "./lib/path-utils";
import { Command } from "@tauri-apps/plugin-shell";
import { open as tauriOpen, save as tauriSave } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { X, AlertCircle, CheckCircle2 } from "lucide-react";

type Toast = { id: number; type: "error" | "success"; message: string; undoAction?: () => void; undoLabel?: string };

interface EnvCheckItem { name: string; label: string; status: "ok" | "warn" | "missing"; detail: string; install_cmd?: string; }
type EnvCheckResult = { items: EnvCheckItem[]; all_ok: boolean } | null;

export function App() {
  const ctx = useProfiles();
  const rightTerminal = useTerminal("right");     // profile「运行」→ 右侧面板
  const bottomTerminal = useTerminal("bottom");   // 工具栏按钮 → 底部面板 (VS Code 风格)
  useFontScale();                                 // 初始化全局 UI 字体缩放
  const [showAddDialog, setShowAddDialog] = useState(false);
  const [showDeleteConfirm, setShowDeleteConfirm] = useState(false);
  const [showWelcome, setShowWelcome] = useState(false);
  const [showShortcuts, setShowShortcuts] = useState(false);
  const [showQuickSwitcher, setShowQuickSwitcher] = useState(false);
  const [quickSwitcherMode, setQuickSwitcherMode] = useState<"profile" | "history">("profile");
  const [importData, setImportData] = useState<{ name: string; desc?: string; env: Record<string, string> } | null>(null);
  const [scanData, setScanData] = useState<ScanProfile[] | null>(null);
  const [toasts, setToasts] = useState<Toast[]>([]);
  const toastIdRef = useRef(0);
  const [showNameDialog, setShowNameDialog] = useState(false);
  const [nameDialogTitle, setNameDialogTitle] = useState("");
  const [nameDialogInitial, setNameDialogInitial] = useState("");
  const [nameDialogOnConfirm, setNameDialogOnConfirm] = useState<((name: string) => Promise<void>) | null>(null);
  const [appVersion, setAppVersion] = useState("");
  const [rightMaximized, setRightMaximized] = useState(false);
  const [bottomMaximized, setBottomMaximized] = useState(false);
  const [sidebarVisible, setSidebarVisible] = useState(true);
  const [backupExists, setBackupExists] = useState(false);
  const [envCheck, setEnvCheck] = useState<EnvCheckResult>(null);
  const [showAbout, setShowAbout] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [showBatchDeleteConfirm, setShowBatchDeleteConfirm] = useState(false);
  const [showUsage, setShowUsage] = useState(false);
  const [activeActivity, setActiveActivity] = useState<ActivityKey>("profile");
  const [skillData, setSkillData] = useState<SkillManagerData | null>(null);
  const [agentData, setAgentData] = useState<AgentManagerData | null>(null);
  // Baseline counts from a full scan (user + all projects), unaffected by activeProject filter
  const [baselineCounts, setBaselineCounts] = useState<{ user: number; project: number }>({ user: 0, project: 0 });
  const [showGraph, setShowGraph] = useState(false);

  // Hook overwrite confirmation — shown before move/copy when target already has the same hook
  type OverwriteConfirm = {
    hookName: string;
    targetLabel: string;
    onConfirm: () => void;
  };
  const [overwriteConfirm, setOverwriteConfirm] = useState<OverwriteConfirm | null>(null);
  const [graphData, setGraphData] = useState<DependencyGraphData | null>(null);
  const [hookData, setHookData] = useState<HookManagerData | null>(null);
  const [hookDataLoading, setHookDataLoading] = useState(false);
  const [selectedHook, setSelectedHook] = useState<HookEntry | null>(null);
  const [activeScope, setActiveScope] = useState<ScopeTab>("user");
  const { projects, activeProject, setActiveProject, addProject } = useProjects();

  // Compute scan paths: null = user only, or a list of project paths to scan
  const scanProjectPaths = useMemo<(string | null)[]>(() => {
    if (activeScope === "user") return [null];
    if (activeScope === "project") {
      if (activeProject) return [activeProject.path];
      // "全部项目" — scan all registered projects
      if (projects.length > 0) return projects.map(p => p.path);
      return [];
    }
    // "all" scope
    if (activeProject) return [null, activeProject.path]; // user + specific project
    if (projects.length > 0) return [null, ...projects.map(p => p.path)]; // user + all projects
    return [null]; // user only, no projects registered
  }, [activeScope, activeProject, projects]);

  // Hook scan paths — always include user-level + ALL projects so tab badge
  // counts are correct regardless of which scope tab is active.
  // Unlike scanProjectPaths (which changes per scope), this is stable and
  // ensures projectCount never drops to 0 when viewing the "user" tab.
  const hookScanPaths = useMemo<(string | null)[]>(() => {
    if (projects.length === 0) return [null];
    return [null, ...projects.map(p => p.path)];
  }, [projects]);

  // Backward-compatible single project path (for callbacks that need one path)
  const scanProjectPath = useMemo(() => {
    if (scanProjectPaths.length === 0) return null;
    if (scanProjectPaths.length === 1) return scanProjectPaths[0];
    return null; // multi-project mode — individual re-scans use user-level
  }, [scanProjectPaths]);

  // Keep a ref so callbacks always read current value
  const scanProjectPathRef = useRef(scanProjectPath);
  scanProjectPathRef.current = scanProjectPath;
  const scanProjectPathsRef = useRef(scanProjectPaths);
  scanProjectPathsRef.current = scanProjectPaths;

  // Refresh baseline counts from a full scan (user + all projects), insensitive to activeProject
  const refreshBaselineCounts = useCallback(async () => {
    try {
      const allPaths: (string | null)[] = [null, ...projects.map(p => p.path)];
      const results = await Promise.all(
        allPaths.map(p => invoke<SkillManagerData>("scan_skills", { projectPath: p }))
      );
      // results[0] = user-level scan (null path); rest = per-project scans
      // Project scans also include user-level items, so only count user items from the null scan
      const userCount = results[0]?.standaloneSkills.filter(s => !s.id.includes(":project-")).length ?? 0;
      let projectCount = 0;
      for (const r of results) {
        projectCount += r.standaloneSkills.filter(s => s.id.includes(":project-")).length;
      }
      setBaselineCounts({ user: userCount, project: projectCount });
    } catch { /* ignore */ }
  }, [projects]);

  // Initial baseline counts & refresh after skillData mutations
  useEffect(() => { refreshBaselineCounts(); }, [refreshBaselineCounts, skillData]);

  const [hookWizardOpen, setHookWizardOpen] = useState(false);
  const [hookStoreOpen, setHookStoreOpen] = useState(false);
  const [skillDataLoading, setSkillDataLoading] = useState(false);
  const [selectedSkillItem, setSelectedSkillItem] = useState<SelectedItem | null>(null);
  const [checkingUpdates, setCheckingUpdates] = useState(false);
  const [updateInfos, setUpdateInfos] = useState<PluginUpdateInfo[]>([]);
  const [marketplaceOpen, setMarketplaceOpen] = useState(false);
  const usage = useUsage();
  const { colorScheme } = useTheme();
  const batchDeleteNamesRef = useRef<string[]>([]);
  // Platform info (fetched once, used for download path construction)
  const platformRef = useRef<{ os: string; arch: string }>({ os: "macos", arch: "x86_64" });
  // Update dialog: store manifest + platform data when new version found
  const [updateDialog, setUpdateDialog] = useState<{ version: string; notes: string; url: string; sha256: string } | null>(null);
  const [downloadState, setDownloadState] = useState<{ phase: "idle" | "downloading" | "verifying"; progress: number; error: string | null }>({
    phase: "idle", progress: 0, error: null,
  });

  // Load profiles + platform info on mount
  useEffect(() => { ctx.loadProfiles(); }, []);
  useEffect(() => {
    invoke<{ os: string; arch: string }>("get_platform_info").then((info) => { platformRef.current = info; }).catch(() => {});
  }, []);

  // Keep terminals aware of valid profile names (for history restore validation)
  useEffect(() => {
    const names = ctx.profiles.map((p) => p.name);
    rightTerminal.setValidProfileNames(names);
    bottomTerminal.setValidProfileNames(names);
  }, [ctx.profiles]);

  // Environment check — refresh on mount, on panel open, on dialog open
  const refreshEnvCheck = useCallback(() => {
    invoke<EnvCheckResult>("check_environment").then(setEnvCheck).catch(() => {});
  }, []);
  useEffect(() => { refreshEnvCheck(); }, [refreshEnvCheck]);

  // Re-check tools when ProfileDialog opens (user may have installed since last open)
  useEffect(() => {
    if (showAddDialog) refreshEnvCheck();
  }, [showAddDialog, refreshEnvCheck]);

  // Check backup status on mount + after profile changes
  useEffect(() => {
    invoke<boolean>("config_backup_exists").then(setBackupExists).catch(() => {});
  }, [ctx.profiles]);

  // Merge per-profile run counts from both terminal panels
  const usageCounts = useMemo(() => {
    const merged: Record<string, number> = { ...bottomTerminal.usageCounts };
    for (const [k, v] of Object.entries(rightTerminal.usageCounts)) {
      merged[k] = (merged[k] || 0) + v;
    }
    return merged;
  }, [rightTerminal.usageCounts, bottomTerminal.usageCounts]);

  // Ensure shell wrapper and usage hooks are installed
  useEffect(() => {
    invoke("ensure_shell_rc").catch(() => {});
    invoke("ensure_usage_hooks").catch(() => {});
  }, []);

  /** Scan skills + agents across multiple project paths, merge, and return. */
  const scanMultiProject = useCallback(async (paths: (string | null)[]) => {
    if (paths.length === 0) {
      const empty: SkillManagerData = { plugins: [], standaloneSkills: [], systemSkills: [], commands: [] };
      return { skills: empty, agents: { agents: [] as AgentManagerData["agents"] } };
    }
    // Single path — simple case
    if (paths.length === 1) {
      const [skills, agents] = await Promise.all([
        invoke<SkillManagerData>("scan_skills", { projectPath: paths[0] }),
        invoke<AgentManagerData>("scan_agents", { projectPath: paths[0] }),
      ]);
      return { skills, agents };
    }
    // Multiple paths: determine if user-level is included (null in the array)
    const hasUserLevel = paths[0] === null;
    const projectPaths: string[] = hasUserLevel
      ? (paths.slice(1) as string[])
      : (paths as string[]);

    // Baseline: user-level scan (if included), or empty for project-only scans
    const [baseSkills, baseAgents] = hasUserLevel
      ? await Promise.all([
          invoke<SkillManagerData>("scan_skills", { projectPath: null }),
          invoke<AgentManagerData>("scan_agents", { projectPath: null }),
        ])
      : [
          { plugins: [] as any[], standaloneSkills: [] as any[], systemSkills: [] as any[], commands: [] as any[] } as SkillManagerData,
          { agents: [] as any[] } as AgentManagerData,
        ];

    const mergedSkills: SkillManagerData = {
      plugins: [...baseSkills.plugins],
      standaloneSkills: [...baseSkills.standaloneSkills],
      systemSkills: [...baseSkills.systemSkills],
      commands: [...(baseSkills.commands || [])],
    };
    const mergedAgents: AgentManagerData = { agents: [...baseAgents.agents] };

    // Scan each project and merge only project-scoped items
    const projResults = await Promise.all(projectPaths.map(pp =>
      Promise.all([
        invoke<SkillManagerData>("scan_skills", { projectPath: pp }),
        invoke<AgentManagerData>("scan_agents", { projectPath: pp }),
      ])
    ));
    for (const [pSkills, pAgents] of projResults) {
      mergedSkills.standaloneSkills.push(...pSkills.standaloneSkills.filter(s => s.id.includes(":project-")));
      mergedSkills.commands.push(...(pSkills.commands || []).filter(c => c.id.includes(":project-")));
      mergedAgents.agents.push(...pAgents.agents.filter(a => a.source === "project"));
    }
    return { skills: mergedSkills, agents: mergedAgents };
  }, []);

  // Load skill/plugin data when switching to skills view
  useEffect(() => {
    if (activeActivity !== "skills") {
      setSelectedSkillItem(null);
      return;
    }
    setSkillDataLoading(true);
    scanMultiProject(scanProjectPaths)
      .then(async ({ skills, agents }) => {
        setSkillData(skills);
        setAgentData(agents);
        setSelectedSkillItem(null);
        // Refresh baseline counts in background (don't block)
        refreshBaselineCounts();
        // Pre-load dependency graph
        try {
          const graph = await invoke<DependencyGraphData>("build_dependency_graph", {
            skillsData: skills,
            agentsData: agents,
          });
          setGraphData(graph);
        } catch (e) {
          console.error("Failed to pre-load dependency graph:", e);
          addToast("error", `依赖图预加载失败: ${String(e).slice(0, 120)}`);
        }
      })
      .catch(() => { setSkillData(null); setAgentData(null); })
      .finally(() => setSkillDataLoading(false));
  }, [activeActivity, scanProjectPaths, scanMultiProject]);

  // ── Hook multi-project scanning ───────────────────────────
  // Like scanMultiProject for skills, this scans all relevant project paths
  // and merges results, so project-level hooks are visible when a project is selected.
  const scanMultiProjectHooks = useCallback(async (paths: (string | null)[]) => {
    if (paths.length === 0) {
      return { hooks: [] as HookEntry[] };
    }
    // Single path — simple case (user-level or single project)
    if (paths.length === 1) {
      const data = await invoke<HookManagerData>("scan_hooks", { projectPath: paths[0] });
      return data;
    }
    // Multiple paths: user-level baseline + per-project scans
    const hasUserLevel = paths[0] === null;
    const projectPaths: string[] = hasUserLevel
      ? (paths.slice(1) as string[])
      : (paths as string[]);

    // Baseline scan (user-level)
    const baseData: HookManagerData = hasUserLevel
      ? await invoke<HookManagerData>("scan_hooks", { projectPath: null })
      : { hooks: [] };

    const merged: HookEntry[] = [...baseData.hooks];

    // Per-project scans — only keep project-scoped hooks (source === "project")
    const projResults = await Promise.all(
      projectPaths.map(pp => invoke<HookManagerData>("scan_hooks", { projectPath: pp }))
    );
    for (const pr of projResults) {
      merged.push(...pr.hooks.filter(h => h.source === "project"));
    }
    return { hooks: merged } as HookManagerData;
  }, []);

  // Load hook data when switching to hooks view, or when scope/project changes
  const refreshHooks = useCallback(() => {
    return scanMultiProjectHooks(hookScanPaths)
      .then((data) => {
        setHookData(data);
        setSelectedHook((prev) => {
          if (!prev) return null;
          return data.hooks.find((h) => h.id === prev.id) || null;
        });
        return data;
      })
      .catch(() => {});
  }, [scanMultiProjectHooks, hookScanPaths]);

  useEffect(() => {
    if (activeActivity !== "hooks") {
      setSelectedHook(null);
      return;
    }
    setHookDataLoading(true);
    scanMultiProjectHooks(hookScanPaths)
      .then((data) => {
        setHookData(data);
        setSelectedHook(null);
      })
      .catch((e) => {
        console.error("Failed to scan hooks:", e);
        setHookData(null);
      })
      .finally(() => setHookDataLoading(false));
  }, [activeActivity, scanMultiProjectHooks, hookScanPaths]);

  // Batch hook operations
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
      console.error("Toggle hook failed:", e);
    }
  }, [refreshHooks]);

  // Auto-open terminal for pop-out windows (?terminal=1)
  useEffect(() => {
    if (window.location.search.includes("terminal=1")) {
      bottomTerminal.open();
    }
  }, []);

  // Listen for onboarding wizard dismiss event
  useEffect(() => {
    const handler = () => setShowWelcome(false);
    window.addEventListener("kn-dismiss-welcome", handler);
    return () => window.removeEventListener("kn-dismiss-welcome", handler);
  }, []);
  // Terminal error → toast (both panels)
  useEffect(() => {
    rightTerminal.setErrorCallback((msg: string) => addToast("error", msg));
    return () => rightTerminal.setErrorCallback(() => {});
  }, []);
  useEffect(() => {
    bottomTerminal.setErrorCallback((msg: string) => addToast("error", msg));
    return () => bottomTerminal.setErrorCallback(() => {});
  }, []);

  // Fetch app version
  useEffect(() => {
    invoke<string>("get_app_version").then((v) => setAppVersion(v)).catch(() => {});
  }, []);

  // Toast management
  const addToast = useCallback((type: "error" | "success", message: string) => {
    const id = ++toastIdRef.current;
    setToasts((prev) => [...prev, { id, type, message }]);
    setTimeout(() => setToasts((prev) => prev.filter((t) => t.id !== id)), type === "error" ? 12000 : 4000);
  }, []);

  const dismissToast = (id: number) => setToasts((prev) => prev.filter((t) => t.id !== id));

  // ── Hook Delete ───────────────────────────────────────────

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
    } catch (e) {
      addToast("error", `删除 Hook 失败: ${String(e).slice(0, 120)}`);
    }
  }, [refreshHooks, addToast]);

  // ── Hook Move / Copy ──

  // Keep a ref to hookData for use in callbacks that shouldn't re-create on data change
  const hookDataRef = useRef(hookData);
  hookDataRef.current = hookData;

  /** Compute the target config file path for a hook move/copy. */
  const getHookTargetPath = useCallback((cli: string, toScope: "user" | "project", sourcePath: string, targetProjectPath?: string): string => {
    const configFile = cli === "codex" ? "config.toml" : "settings.json";
    // Qoder: user-level = .qoder-cn, project-level = .qoder (different names!)
    const cliDirUser = cli === "qoder" ? ".qoder-cn" : cli === "codex" ? ".codex" : ".claude";
    const cliDirProject = cli === "qoder" ? ".qoder" : cli === "codex" ? ".codex" : ".claude";

    // Project scope: use target project path directly
    if (toScope === "project" && targetProjectPath) {
      return `${targetProjectPath}/${cliDirProject}/${configFile}`;
    }

    // User scope: find home directory from an existing user-level hook of the SAME CLI
    const data = hookDataRef.current;
    const userHook = data?.hooks?.find(
      (h) => h.cli === cli && h.source !== "project"
    );
    if (userHook) {
      const base = userHook.path.split(`/${cliDirUser}/`)[0];
      if (base && base !== userHook.path) {
        return `${base}/${cliDirUser}/${configFile}`;
      }
    }
    // Fallback: derive from sourcePath (works when source is already user-level)
    if (sourcePath.includes(`/${cliDirUser}/`)) {
      const base = sourcePath.split(`/${cliDirUser}/`)[0];
      return `${base}/${cliDirUser}/${configFile}`;
    }
    return `${cliDirUser}/${configFile}`;
  }, []);

  // Check if a hook with the same CLI + eventType + command already exists at the target path
  const findDuplicateAtTarget = useCallback((hook: HookEntry, toPath: string): HookEntry | undefined => {
    return hookData?.hooks.find((h) =>
      h.cli === hook.cli &&
      h.eventType === hook.eventType &&
      h.command === hook.command &&
      h.path === toPath
    );
  }, [hookData]);

  const handleMoveHook = useCallback(async (hook: HookEntry, toScope: "user" | "project", targetProject?: import("./lib/types").ProjectInfo) => {
    const fromScope = hook.source === "project" ? "project" : "user";
    const toPath = getHookTargetPath(hook.cli, toScope, hook.path, targetProject?.path);
    const name = hook.name || hook.matcher || hook.eventType;
    const targetLabel = toScope === "user" ? "用户级" : "项目级";

    // Check if target already has an identical hook
    const dup = findDuplicateAtTarget(hook, toPath);
    if (dup) {
      setOverwriteConfirm({
        hookName: name,
        targetLabel,
        onConfirm: async () => {
          setOverwriteConfirm(null);
          try {
            await invoke<any>("move_hook_entry", {
              cli: hook.cli, eventType: hook.eventType,
              groupIdx: hook.groupIdx, hookIdx: hook.hookIdx,
              fromPath: hook.path, toPath, fromScope, toScope,
            });
            refreshHooks();
            addToast("success", `已覆盖 "${name}" → ${targetLabel}`);
          } catch (e) {
            addToast("error", `移动 Hook 失败: ${String(e).slice(0, 120)}`);
          }
        },
      });
      return;
    }

    try {
      const undoInfo = await invoke<any>("move_hook_entry", {
        cli: hook.cli,
        eventType: hook.eventType,
        groupIdx: hook.groupIdx,
        hookIdx: hook.hookIdx,
        fromPath: hook.path,
        toPath,
        fromScope,
        toScope,
      });

      refreshHooks();

      setToasts((prev) => {
        const id = ++toastIdRef.current;
        return [...prev, {
          id,
          type: "success" as const,
          message: `已移动 "${name}" → ${targetLabel}`,
          undoAction: async () => {
            await invoke("undo_move_hook", undoInfo);
            refreshHooks();
          },
        } as any];
      });
    } catch (e) {
      addToast("error", `移动 Hook 失败: ${String(e).slice(0, 120)}`);
    }
  }, [getHookTargetPath, findDuplicateAtTarget, refreshHooks, addToast]);

  const handleCopyHook = useCallback(async (hook: HookEntry, toScope: "user" | "project", targetProject?: import("./lib/types").ProjectInfo) => {
    const toPath = getHookTargetPath(hook.cli, toScope, hook.path, targetProject?.path);
    const name = hook.name || hook.matcher || hook.eventType;
    const targetLabel = toScope === "user" ? "用户级" : "项目级";

    // Check if target already has an identical hook
    const dup = findDuplicateAtTarget(hook, toPath);
    if (dup) {
      setOverwriteConfirm({
        hookName: name,
        targetLabel,
        onConfirm: async () => {
          setOverwriteConfirm(null);
          try {
            await invoke("copy_hook_entry", {
              cli: hook.cli, eventType: hook.eventType,
              groupIdx: hook.groupIdx, hookIdx: hook.hookIdx,
              fromPath: hook.path, toPath,
            });
            refreshHooks();
            addToast("success", `已覆盖复制 "${name}" → ${targetLabel}`);
          } catch (e) {
            addToast("error", `复制 Hook 失败: ${String(e).slice(0, 120)}`);
          }
        },
      });
      return;
    }

    try {
      await invoke("copy_hook_entry", {
        cli: hook.cli,
        eventType: hook.eventType,
        groupIdx: hook.groupIdx,
        hookIdx: hook.hookIdx,
        fromPath: hook.path,
        toPath,
      });

      await refreshHooks();
      addToast("success", `已复制 "${name}" → ${targetLabel}`);
    } catch (e) {
      addToast("error", `复制 Hook 失败: ${String(e).slice(0, 120)}`);
    }
  }, [getHookTargetPath, findDuplicateAtTarget, refreshHooks, addToast]);

  const handleBatchMoveHooks = useCallback(async (hooks: HookEntry[], toScope: "user" | "project", targetProject?: import("./lib/types").ProjectInfo) => {
    let ok = 0;
    let failed = 0;
    // Process in reverse order (descending groupIdx, descending hookIdx) within each
    // event_type to prevent index drift — removing an earlier hook shifts later indices.
    const sorted = [...hooks].sort((a, b) => {
      if (a.eventType !== b.eventType) return a.eventType.localeCompare(b.eventType);
      if (a.groupIdx !== b.groupIdx) return b.groupIdx - a.groupIdx; // descending
      return b.hookIdx - a.hookIdx; // descending
    });
    for (const hook of sorted) {
      try {
        const fromScope = hook.source === "project" ? "project" : "user";
        const toPath = getHookTargetPath(hook.cli, toScope, hook.path, targetProject?.path);
        await invoke("move_hook_entry", {
          cli: hook.cli,
          eventType: hook.eventType,
          groupIdx: hook.groupIdx,
          hookIdx: hook.hookIdx,
          fromPath: hook.path,
          toPath,
          fromScope,
          toScope,
        });
        ok++;
      } catch (e) {
        failed++;
        console.error(`Batch move hook failed:`, e);
      }
    }
    refreshHooks();
    if (failed === 0) {
      addToast("success", `已移动 ${ok} 项 Hook → ${toScope === "user" ? "用户级" : "项目级"}`);
    } else {
      addToast("error", `移动完成: ${ok} 成功, ${failed} 失败`);
    }
  }, [getHookTargetPath, refreshHooks, addToast]);

  const handleBatchCopyHooks = useCallback(async (hooks: HookEntry[], toScope: "user" | "project", targetProject?: import("./lib/types").ProjectInfo) => {
    let ok = 0;
    let failed = 0;
    for (const hook of hooks) {
      try {
        const toPath = getHookTargetPath(hook.cli, toScope, hook.path, targetProject?.path);

        // Direct copy — reads source snapshot, writes to target, never touches source file
        await invoke("copy_hook_entry", {
          cli: hook.cli,
          eventType: hook.eventType,
          groupIdx: hook.groupIdx,
          hookIdx: hook.hookIdx,
          fromPath: hook.path,
          toPath,
        });

        ok++;
      } catch (e) {
        failed++;
        console.error(`Batch copy hook failed:`, e);
      }
    }
    refreshHooks();
    if (failed === 0) {
      addToast("success", `已复制 ${ok} 项 Hook → ${toScope === "user" ? "用户级" : "项目级"}`);
    } else {
      addToast("error", `复制完成: ${ok} 成功, ${failed} 失败`);
    }
  }, [getHookTargetPath, refreshHooks, addToast]);

  // ── Project management ──
  const handleAddProject = useCallback(async () => {
    try {
      const selectedPath = await tauriOpen({
        directory: true,
        multiple: false,
        title: "选择项目目录",
      });
      if (!selectedPath) return;
      const name = basename(selectedPath) || selectedPath;
      await addProject(name, selectedPath);
      addToast("success", `项目 "${name}" 已注册`);
    } catch (e) {
      addToast("error", `注册项目失败: ${String(e).slice(0, 120)}`);
    }
  }, [addProject, addToast]);

  // ── Move / Copy resource ──

  // ── Resource helpers (extracted from duplicated move/copy/batch logic) ──

  /** Common fields present on all SelectedItem data variants. */
  interface ResourceData {
    id: string;
    name: string;
    path: string;
    cli?: string;
    projectName?: string;
  }

  /** Extract common fields from any SelectedItem with defensive fallbacks. */
  function getResourceData(item: SelectedItem): ResourceData {
    const data = item.data as Record<string, unknown>;
    return {
      id: (data.id as string) || "",
      name: (data.name as string) || "",
      path: (data.path as string) || "",
      cli: data.cli as string | undefined,
      projectName: data.projectName as string | undefined,
    };
  }

  /** Subdirectory inside the CLI config folder for each resource type. */
  type ResourceType = "skill" | "agent" | "command";

  function getResourceType(item: SelectedItem): ResourceType {
    switch (item.type) {
      case "standalone": case "system": case "plugin-skill": return "skill";
      case "agent": case "plugin-agent": return "agent";
      default: return "command";
    }
  }

  function getSubdir(rt: ResourceType): string {
    return rt === "skill" ? "skills" : rt === "agent" ? "agents" : "commands";
  }

  /** Detect CLI type from a file path (e.g. "/.codex/" → "codex").
   *  Normalizes Windows backslashes to forward slashes before matching. */
  function detectCliFromPath(srcPath: string): string {
    const normalized = srcPath.replace(/\\/g, "/");
    if (normalized.includes("/.codex/")) return "codex";
    if (normalized.includes("/.qoder-cn/") || normalized.includes("/.qoder/")) return "qoder";
    return "claude";
  }

  /** CLI-appropriate config directory name for user-level scope. */
  function userConfigDir(cli: string): string {
    if (cli === "codex" || cli === "cx") return ".codex";
    if (cli === "qoder") return ".qoder-cn";
    return ".claude";
  }

  /** CLI-appropriate config directory name for project-level scope. */
  function projectConfigDir(cli: string): string {
    if (cli === "codex" || cli === "cx") return ".codex";
    if (cli === "qoder") return ".qoder"; // project-level uses .qoder (not .qoder-cn)
    return ".claude";
  }

  /** Build destination directory for a move/copy operation. */
  async function buildDestDir(
    srcPath: string,
    cliFromData: string | undefined,
    toScope: "user" | "project",
    subdir: string,
    project?: import("./lib/types").ProjectInfo,
  ): Promise<string> {
    if (toScope === "user") {
      const homeDir = await invoke<string>("get_home_dir");
      const cli = detectCliFromPath(srcPath);
      return `${homeDir}/${userConfigDir(cli)}/${subdir}`;
    }
    // project scope
    if (!project) throw new Error("no project");
    const cli = cliFromData || "claude";
    return `${project.path}/${projectConfigDir(cli)}/${subdir}`;
  }
  // ── end resource helpers ──

  /** @deprecated Use getSubdir(getResourceType(item)) instead. */
  const subdirForType = (rt: string): string => getSubdir(rt as ResourceType);

  const handleMoveResource = useCallback(async (item: SelectedItem, toScope: "user" | "project", targetProject?: import("./lib/types").ProjectInfo) => {
    try {
      const data = getResourceData(item);
      const isProject = data.projectName !== undefined || data.id?.includes(":project-");
      const fromScope = isProject ? "project" : "user";
      const resourceType = getResourceType(item);
      const subdir = getSubdir(resourceType);
      const srcPath = data.path as string;
      const project = toScope === "project" ? (targetProject ?? activeProject ?? undefined) : undefined;

      if (toScope === "project" && !project) { addToast("error", "请先选择目标项目"); return; }

      const destDir = await buildDestDir(srcPath, data.cli, toScope, subdir, project);

      const undoInfo = await invoke<any>("move_skill_file", {
        sourcePath: data.path,
        destDir,
        resourceName: data.name,
        resourceType,
        fromScope,
        toScope,
      });

      // Re-scan with full project paths (like scanMultiProject) so project tabs stay populated
      const { skills: reSkills, agents: reAgents } = await scanMultiProject(scanProjectPathsRef.current);
      setSkillData(reSkills);
      setAgentData(reAgents);

      // Show undo toast
      setToasts((prev) => {
        const id = ++toastIdRef.current;
        return [...prev, {
          id,
          type: "success",
          message: `已移动 "${data.name}" → ${toScope === "user" ? "用户级" : "项目级"}`,
          undoAction: async () => {
            await invoke("undo_move_skill", {
              backupPath: undoInfo.backupPath,
              originalPath: undoInfo.originalPath,
              destPath: undoInfo.destPath,
              contentFingerprint: undoInfo.contentFingerprint,
            });
            const { skills: s, agents: a } = await scanMultiProject(scanProjectPathsRef.current);
            setSkillData(s);
            setAgentData(a);
          },
        }];
      });
    } catch (e) {
      addToast("error", `移动失败: ${String(e).slice(0, 120)}`);
    }
  }, [activeProject, scanProjectPath, addToast, scanMultiProject]);

  const handleCopyResource = useCallback(async (item: SelectedItem, toScope: "user" | "project", targetProject?: import("./lib/types").ProjectInfo) => {
    try {
      const data = getResourceData(item);
      const srcPath = data.path as string;
      const resourceType = getResourceType(item);
      const subdir = getSubdir(resourceType);
      const project = toScope === "project" ? (targetProject ?? activeProject ?? undefined) : undefined;

      if (toScope === "project" && !project) { addToast("error", "请先选择目标项目"); return; }

      const destDir = await buildDestDir(srcPath, data.cli, toScope, subdir, project);

      await invoke("copy_skill_file", {
        sourcePath: data.path,
        destDir,
        resourceName: data.name,
      });

      const { skills: reSkills, agents: reAgents } = await scanMultiProject(scanProjectPathsRef.current);
      setSkillData(reSkills);
      setAgentData(reAgents);

      addToast("success", `已复制 "${data.name}" → ${toScope === "user" ? "用户级" : "项目级"}`);
    } catch (e) {
      addToast("error", `复制失败: ${String(e).slice(0, 120)}`);
    }
  }, [activeProject, scanProjectPath, addToast, scanMultiProject]);

  // ── Batch Move / Copy ──
  const handleBatchMove = useCallback(async (items: SelectedItem[], toScope: "user" | "project", targetProject?: import("./lib/types").ProjectInfo) => {
    const total = items.length;
    let ok = 0;
    let failed = 0;
    for (const item of items) {
      try {
        const data = getResourceData(item);
        const isProject = data.projectName !== undefined || data.id?.includes(":project-");
        const fromScope = isProject ? "project" : "user";
        const resourceType = getResourceType(item);
        const subdir = getSubdir(resourceType);
        const srcPath = data.path as string;
        const project = toScope === "project" ? (targetProject ?? activeProject ?? undefined) : undefined;

        if (toScope === "project" && !project) throw new Error("no project");

        const destDir = await buildDestDir(srcPath, data.cli, toScope, subdir, project);

        await invoke("move_skill_file", {
          sourcePath: data.path,
          destDir,
          resourceName: data.name,
          resourceType,
          fromScope,
          toScope,
        });
        ok++;
      } catch (e) {
        failed++;
        console.error(`Batch move failed for ${getResourceData(item).name}:`, e);
      }
    }
    // Re-scan with full project paths so project tabs stay populated
    const { skills: reSkills, agents: reAgents } = await scanMultiProject(scanProjectPathsRef.current);
    setSkillData(reSkills);
    setAgentData(reAgents);

    if (failed === 0) {
      addToast("success", `已移动 ${ok} 项 → ${toScope === "user" ? "用户级" : "项目级"}`);
    } else {
      addToast("error", `移动完成: ${ok} 成功, ${failed} 失败`);
    }
  }, [activeProject, scanProjectPath, addToast, scanMultiProject]);

  const handleBatchCopy = useCallback(async (items: SelectedItem[], toScope: "user" | "project", targetProject?: import("./lib/types").ProjectInfo) => {
    const total = items.length;
    let ok = 0;
    let failed = 0;
    for (const item of items) {
      try {
        const data = getResourceData(item);
        const srcPath = data.path as string;
        const resourceType = getResourceType(item);
        const subdir = getSubdir(resourceType);
        const project = toScope === "project" ? (targetProject ?? activeProject ?? undefined) : undefined;

        if (toScope === "project" && !project) throw new Error("no project");

        const destDir = await buildDestDir(srcPath, data.cli, toScope, subdir, project);

        await invoke("copy_skill_file", {
          sourcePath: data.path,
          destDir,
          resourceName: data.name,
        });
        ok++;
      } catch (e) {
        failed++;
        console.error(`Batch copy failed for ${getResourceData(item).name}:`, e);
      }
    }
    // Re-scan with full project paths so project tabs stay populated
    const { skills: reSkills, agents: reAgents } = await scanMultiProject(scanProjectPathsRef.current);
    setSkillData(reSkills);
    setAgentData(reAgents);

    if (failed === 0) {
      addToast("success", `已复制 ${ok} 项 → ${toScope === "user" ? "用户级" : "项目级"}`);
    } else {
      addToast("error", `复制完成: ${ok} 成功, ${failed} 失败`);
    }
  }, [activeProject, scanProjectPath, addToast, scanMultiProject]);

  // After re-scanning, sync selectedSkillItem to the matching item in fresh data
  const syncSelection = useCallback((data: SkillManagerData, prev: SelectedItem | null) => {
    if (!prev) return null;
    if (prev.type === "plugin") {
      const found = data.plugins.find((p) => p.id === (prev!.data as unknown as ResourceData).id);
      return found ? { type: "plugin" as const, data: found } : null;
    }
    if (prev.type === "standalone") {
      const found = data.standaloneSkills.find((s) => s.id === (prev!.data as unknown as ResourceData).id);
      return found ? { type: "standalone" as const, data: found } : null;
    }
    if (prev.type === "system") {
      const found = data.systemSkills.find((s) => s.id === (prev!.data as unknown as ResourceData).id);
      return found ? { type: "system" as const, data: found } : null;
    }
    if (prev.type === "command") {
      const found = data.commands.find((c) => c.id === (prev!.data as unknown as ResourceData).id);
      return found ? { type: "command" as const, data: found } : null;
    }
    return null;
  }, []);

  // Agent selection sync (separate since agent data comes from agentData, not skillData)
  const syncAgentSelection = useCallback((agents: AgentManagerData, prev: SelectedItem | null) => {
    if (!prev || prev.type !== "agent") return null;
    const found = agents.agents.find((a) => a.id === (prev!.data as unknown as ResourceData).id);
    return found ? { type: "agent" as const, data: found } : null;
  }, []);

  // Toggle dependency graph view (data pre-loaded when entering skills view)
  const loadGraph = useCallback(() => {
    if (!graphData) {
      // graphData not loaded yet — fetch it first
      if (!skillData || !agentData) return;
      invoke<DependencyGraphData>("build_dependency_graph", {
        skillsData: skillData,
        agentsData: agentData,
      }).then((g) => {
        setGraphData(g);
        setShowGraph(true);
      }).catch((e) => {
        console.error("Failed to build dependency graph:", e);
        addToast("error", `依赖图加载失败: ${String(e).slice(0, 120)}`);
      });
      return;
    }
    setShowGraph((prev) => !prev);
  }, [graphData, skillData, agentData, addToast]);

  // Keep graphData in sync when skills/agents change (e.g. after toggle)
  useEffect(() => {
    if (!skillData || !agentData) return;
    invoke<DependencyGraphData>("build_dependency_graph", {
      skillsData: skillData,
      agentsData: agentData,
    }).then(setGraphData).catch((e) => {
      console.error("Failed to sync dependency graph:", e);
    });
  }, [skillData, agentData]);

  // Show detail for a graph node
  const showNodeDetail = useCallback((nodeId: string) => {
    const parts = nodeId.split(":");
    const kind = parts[1];
    if (kind === "agent" && agentData) {
      const agent = agentData.agents.find((a) => a.id === nodeId);
      if (agent) {
        setSelectedSkillItem({ type: "agent", data: agent });
        setShowGraph(false);
      }
    }
  }, [agentData]);

  // Skill toggle handlers
  const handleTogglePlugin = useCallback(
    async (cli: string, pluginId: string, enabled: boolean) => {
      try {
        await invoke("toggle_plugin", { cli, pluginId, enabled });
        setSkillDataLoading(true);
        const { skills, agents } = await scanMultiProject(scanProjectPathsRef.current);
        setSkillData(skills);
        setAgentData(agents);
        setSelectedSkillItem((prev) => syncSelection(skills, prev));
        setSkillDataLoading(false);
      } catch (e) {
        addToast("error", `操作失败: ${e}`);
      }
    },
    [addToast, syncSelection, scanMultiProject],
  );

  const handleToggleStandaloneSkill = useCallback(
    async (cli: string, skillId: string, enabled: boolean, path?: string) => {
      try {
        await invoke("toggle_standalone_skill", { cli, skillId, enabled, path: path ?? null });
        setSkillDataLoading(true);
        const { skills, agents } = await scanMultiProject(scanProjectPathsRef.current);
        setSkillData(skills);
        setAgentData(agents);
        setSelectedSkillItem((prev) => syncSelection(skills, prev));
        setSkillDataLoading(false);
      } catch (e) {
        addToast("error", `操作失败: ${e}`);
      }
    },
    [addToast, syncSelection, scanMultiProject],
  );

  // Agent toggle
  const handleToggleAgent = useCallback(
    async (cli: string, name: string, enabled: boolean, path?: string) => {
      try {
        await invoke("toggle_agent", { cli, name, enabled, path: path ?? null });
        // Re-scan both agents and skills with full project paths
        setSkillDataLoading(true);
        const { skills, agents } = await scanMultiProject(scanProjectPathsRef.current);
        setSkillData(skills);
        setAgentData(agents);
        setSelectedSkillItem((prev) => {
          if (prev?.type === "agent" && prev.data.name === name) {
            const found = agents.agents.find((a) => a.name === name && a.cli === cli);
            return found ? { type: "agent", data: found } : null;
          }
          return prev;
        });
        setSkillDataLoading(false);
      } catch (e) {
        addToast("error", `操作失败: ${e}`);
      }
    },
    [addToast, scanMultiProject],
  );

  // Agent delete = disable (rename to .disabled)
  const handleDeleteAgent = useCallback(
    async (cli: string, name: string, path?: string) => {
      try {
        await invoke("delete_agent", { cli, name, path: path ?? null });
        // Re-scan with full project paths
        setSkillDataLoading(true);
        const { skills, agents } = await scanMultiProject(scanProjectPathsRef.current);
        setSkillData(skills);
        setAgentData(agents);
        setSelectedSkillItem(null);
        setSkillDataLoading(false);
        addToast("success", `已删除 Agent "${name}"`);
      } catch (e) {
        addToast("error", `删除失败: ${e}`);
      }
    },
    [addToast, scanMultiProject],
  );

  // Batch toggle: fires all toggles, then does ONE scan
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
        // Re-scan with full project paths so project tabs stay populated
        setSkillDataLoading(true);
        const { skills, agents } = await scanMultiProject(scanProjectPathsRef.current);
        setSkillData(skills);
        setAgentData(agents);
        setSelectedSkillItem((prev) => syncSelection(skills, prev));
        setSkillDataLoading(false);
      } catch (e) {
        addToast("error", `批量操作失败: ${e}`);
      }
    },
    [addToast, syncSelection, scanMultiProject],
  );

  const handleBatchUninstall = useCallback(
    async (items: BatchToggleItem[]) => {
      try {
        for (const item of items) {
          // Detect command IDs: user-level "cli:command:name" or project-level "cli:project-command:hash:name"
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
            // Extract name from ID (last segment) as fallback
            const name = item.id.split(":").pop() || item.id;
            await invoke("uninstall_standalone_skill", { cli: item.cli, skillId: item.id, skillPath: item.path ?? null, skillName: name });
          }
        }
        // Re-scan with full project paths so project tabs stay populated
        setSkillDataLoading(true);
        const { skills, agents } = await scanMultiProject(scanProjectPathsRef.current);
        setSkillData(skills);
        setAgentData(agents);
        setSelectedSkillItem(null);
        setSkillDataLoading(false);
        addToast("success", `已删除 ${items.length} 项`);
      } catch (e) {
        addToast("error", `批量删除失败: ${e}`);
      }
    },
    [addToast, scanMultiProject],
  );

  // Track whether check was user-initiated (show toast) or auto (silent)
  const checkSilentRef = useRef(false);

  // Plugin update check — fires background thread, result arrives via event
  const handleCheckUpdates = useCallback(async () => {
    if (checkingUpdates) return; // already running
    checkSilentRef.current = false; // user-initiated → show toast
    setCheckingUpdates(true);
    setUpdateInfos([]);
    try {
      await invoke("check_updates");
    } catch (e) {
      addToast("error", `检查更新失败: ${e}`);
      setCheckingUpdates(false);
    }
  }, [addToast, checkingUpdates]);

  // Listen for update-check-complete event from background thread
  useEffect(() => {
    const unlisten = listen<PluginUpdateInfo[]>("update-check-complete", (event) => {
      setUpdateInfos(event.payload);
      setCheckingUpdates(false);
      // Only toast when user-initiated, not for auto re-checks
      if (!checkSilentRef.current) {
        const count = event.payload.filter((u) => u.hasUpdate).length;
        if (count > 0) addToast("success", `发现 ${count} 个可用更新`);
        else addToast("success", "所有插件均为最新版本");
      }
    });
    return () => { unlisten.then((fn) => fn()); };
  }, [addToast]);

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

  // Listen for update-plugin-complete event from background thread
  useEffect(() => {
    const unlisten = listen<{ pluginId: string; success: boolean; message: string }>("update-plugin-complete", async (event) => {
      const { success, message } = event.payload;
      if (success) {
        addToast("success", message);
        // Re-scan with full project paths so project tabs stay populated
        const { skills, agents } = await scanMultiProject(scanProjectPathsRef.current);
        setSkillData(skills);
        setAgentData(agents);
        setSelectedSkillItem((prev) => syncSelection(skills, prev));
        // Re-check silently (no toast) — result arrives via update-check-complete event
        checkSilentRef.current = true;
        invoke("check_updates").catch(() => {});
      } else {
        addToast("error", message);
      }
    });
    return () => { unlisten.then((fn) => fn()); };
  }, [addToast, syncSelection]);

  // Plugin uninstall handler
  const handleUninstallPlugin = useCallback(
    async (cli: string, pluginId: string) => {
      try {
        const msg = await invoke<string>("uninstall_plugin", { cli, pluginId });
        addToast("success", msg);
        // Re-scan with full project paths so project tabs stay populated
        const { skills, agents } = await scanMultiProject(scanProjectPathsRef.current);
        setSkillData(skills);
        setAgentData(agents);
        setSelectedSkillItem((prev) => syncSelection(skills, prev));
      } catch (e) {
        addToast("error", `删除失败: ${e}`);
      }
    },
    [addToast, syncSelection, scanMultiProject],
  );

  const handleUninstallStandaloneSkill = useCallback(
    async (cli: string, skillId: string, path?: string, name?: string) => {
      try {
        const msg = await invoke<string>("uninstall_standalone_skill", { cli, skillId, skillPath: path ?? null, skillName: name ?? null });
        addToast("success", msg);
        // Re-scan with full project paths so project tabs stay populated
        const { skills, agents } = await scanMultiProject(scanProjectPathsRef.current);
        setSkillData(skills);
        setAgentData(agents);
        setSelectedSkillItem(null);
      } catch (e) {
        addToast("error", `删除失败: ${e}`);
      }
    },
    [addToast, scanMultiProject],
  );

  // Command toggle handler
  const handleToggleCommand = useCallback(
    async (cli: string, name: string, enabled: boolean, path?: string) => {
      try {
        await invoke("toggle_command", { cli, name, enabled, path: path ?? null });
        const { skills, agents } = await scanMultiProject(scanProjectPathsRef.current);
        setSkillData(skills);
        setAgentData(agents);
        setSelectedSkillItem((prev) => syncSelection(skills, prev));
      } catch (e) {
        addToast("error", `操作失败: ${e}`);
      }
    },
    [addToast, syncSelection, scanMultiProject],
  );

  // Command uninstall handler
  const handleUninstallCommand = useCallback(
    async (cli: string, name: string, path?: string) => {
      try {
        const msg = await invoke<string>("uninstall_command", { cli, name, path: path ?? null });
        addToast("success", msg);
        const { skills, agents } = await scanMultiProject(scanProjectPathsRef.current);
        setSkillData(skills);
        setAgentData(agents);
        setSelectedSkillItem(null);
      } catch (e) {
        addToast("error", `删除失败: ${e}`);
      }
    },
    [addToast, scanMultiProject],
  );

  // Listen for plugin-install-complete event
  useEffect(() => {
    const unlisten = listen<{ name: string; cli: string; success: boolean; message: string }>("plugin-install-complete", (event) => {
      const { success, message } = event.payload;
      if (success) {
        addToast("success", message);
        // Re-scan skills with full project paths
        scanMultiProject(scanProjectPathsRef.current).then(({ skills, agents }) => {
          setSkillData(skills);
          setAgentData(agents);
        }).catch(() => {});
      } else {
        addToast("error", message);
      }
    });
    return () => { unlisten.then((fn) => fn()); };
  }, [addToast, scanMultiProject]);

  // Listen for plugin-uninstall-complete event
  useEffect(() => {
    const unlisten = listen<{ pluginId: string; cli: string; success: boolean; message: string }>("plugin-uninstall-complete", (event) => {
      const { success, message } = event.payload;
      if (success) {
        addToast("success", message);
        // Re-scan with full project paths
        scanMultiProject(scanProjectPathsRef.current).then(({ skills, agents }) => {
          setSkillData(skills);
          setAgentData(agents);
          setSelectedSkillItem((prev) => syncSelection(skills, prev));
        }).catch(() => {});
      } else {
        addToast("error", message);
      }
    });
    return () => { unlisten.then((fn) => fn()); };
  }, [addToast, syncSelection, scanMultiProject]);

  // Refresh skills after marketplace install
  const handleMarketplaceInstalled = useCallback(async () => {
    const { skills, agents } = await scanMultiProject(scanProjectPathsRef.current);
    setSkillData(skills);
    setAgentData(agents);
  }, [scanMultiProject]);

  // Show errors as toasts (dedup: track last shown error)
  const lastErrorRef = useRef<string | null>(null);
  useEffect(() => {
    if (ctx.error && ctx.error !== lastErrorRef.current && !showAddDialog && !showNameDialog) {
      lastErrorRef.current = ctx.error;
      addToast("error", ctx.error);
    }
    if (!ctx.error) lastErrorRef.current = null;
  }, [ctx.error, showAddDialog, showNameDialog]);

  // Keyboard shortcuts
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      // Cmd+P → profile quick switcher
      if ((e.metaKey || e.ctrlKey) && !e.shiftKey && (e.key === "p" || e.key === "P")) {
        e.preventDefault();
        setQuickSwitcherMode("profile");
        setShowQuickSwitcher(true);
        return;
      }
      // Cmd+Shift+P → session history
      if ((e.metaKey || e.ctrlKey) && e.shiftKey && (e.key === "p" || e.key === "P")) {
        e.preventDefault();
        setQuickSwitcherMode("history");
        setShowQuickSwitcher(true);
        return;
      }
      if ((e.metaKey || e.ctrlKey) && e.key === "k") {
        e.preventDefault();
        setShowShortcuts((prev) => !prev);
      }
      if ((e.metaKey || e.ctrlKey) && e.key === "n") {
        e.preventDefault();
        setShowAddDialog(true);
      }
      if (e.key === "Escape") {
        if (showQuickSwitcher) setShowQuickSwitcher(false);
        else if (showAddDialog) setShowAddDialog(false);
        else if (showDeleteConfirm) setShowDeleteConfirm(false);
        else if (showNameDialog) setShowNameDialog(false);
        else if (ctx.selectedName) ctx.deselect();
      }
      if (e.key === "Backspace" && ctx.selectedName && !(e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement)) {
        setShowDeleteConfirm(true);
      }
      // Toggle bottom terminal with Ctrl+`
      if (e.ctrlKey && e.key === "`") {
        e.preventDefault();
        bottomTerminal.toggle();
      }
      // Toggle sidebar — Cmd/Ctrl+B (VS Code standard)
      if ((e.metaKey || e.ctrlKey) && !e.shiftKey && e.key === "b") {
        e.preventDefault();
        setSidebarVisible((v) => !v);
      }
      // Toggle bottom terminal — Cmd/Ctrl+J (VS Code standard)
      if ((e.metaKey || e.ctrlKey) && !e.shiftKey && e.key === "j") {
        e.preventDefault();
        bottomTerminal.toggle();
      }
      // Maximize/restore terminal — Cmd/Ctrl+Shift+M. Works when terminal panel is focused.
      if ((e.metaKey || e.ctrlKey) && e.shiftKey && (e.key === "m" || e.key === "M")) {
        e.preventDefault();
        const el = document.activeElement as HTMLElement | null;
        const panel = el?.closest("[data-panel]") as HTMLElement | null;
        if (panel) {
          if (panel.dataset.panel === "right") {
            setRightMaximized((v) => !v);
            setBottomMaximized(false);
          } else if (panel.dataset.panel === "bottom") {
            setBottomMaximized((v) => !v);
            setRightMaximized(false);
          }
        } else {
          // No terminal panel focused — show guidance toast
          addToast("success", `💡 请先点击终端面板，再使用 ${formatShortcut("mod+⇧M")} 最大化`);
        }
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [ctx.selectedName, bottomTerminal.toggle, showAddDialog, showDeleteConfirm, showNameDialog, showShortcuts, showQuickSwitcher]);

  const isDefault = ctx.selectedProfile?.is_default ?? false;

  const handleInstallTool = useCallback(async (cmd: string) => {
    // Open bottom terminal and auto-execute the install command
    bottomTerminal.runInTerminal(cmd, "");
  }, [bottomTerminal]);

  const handleQuickLaunchProfile = useCallback((name: string, command: string, workDir: string) => {
    ctx.selectProfile(name);
    rightTerminal.runInNewTab(command, workDir, command);
    setShowQuickSwitcher(false);
  }, [ctx, rightTerminal]);

  const handlePasteCommand = useCallback(async (cmd: string) => {
    const isAiCmd = /^ai\s/.test(cmd);

    if (isAiCmd) {
      let selected: string | null = null;
      try {
        selected = await tauriOpen({
          directory: true, multiple: false,
          title: "选择项目工作目录",
        }) as string | null;
      } catch { /* cancelled */ }
      if (!selected || typeof selected !== "string") return;

      await rightTerminal.runInTerminal(cmd, selected);
    } else {
      // Non-ai commands: create new tab too
      await rightTerminal.runInNewTab(cmd, "", cmd.slice(0, 30));
    }
  }, [rightTerminal]);

  // Export profile to JSON file
  const handleExport = useCallback(async () => {
    if (!ctx.selectedProfile) return;
    const data = { name: ctx.selectedProfile.name, desc: ctx.selectedProfile.desc, env: ctx.selectedProfile.env, tags: ctx.selectedProfile.tags };
    try {
      const path = await tauriSave({ defaultPath: `${data.name}.json`, filters: [{ name: "JSON", extensions: ["json"] }] });
      if (!path) return;
      const json = JSON.stringify(data, null, 2);
      await invoke("write_file", { path, content: json });
      addToast("success", `已导出到 ${path}`);
    } catch (e) { addToast("error", `导出失败: ${e}`); }
  }, [ctx.selectedProfile]);

  // Batch delete — show confirmation first
  const handleBatchDelete = useCallback((names: string[]) => {
    batchDeleteNamesRef.current = names;
    setShowBatchDeleteConfirm(true);
  }, []);

  const executeBatchDelete = useCallback(async () => {
    const names = batchDeleteNamesRef.current;
    try {
      const deleted: string[] = await invoke("batch_delete_profiles", { names });
      for (const name of deleted) {
        rightTerminal.clearProfileHistory(name);
        bottomTerminal.clearProfileHistory(name);
      }
      await ctx.loadProfiles();
      addToast("success", `已删除 ${deleted.length} 个 profile`);
    } catch (e) { addToast("error", `批量删除失败: ${e}`); }
    setShowBatchDeleteConfirm(false);
    batchDeleteNamesRef.current = [];
  }, [ctx, rightTerminal, bottomTerminal]);

  // About dialog
  const handleAbout = useCallback(() => setShowAbout(true), []);

  // Batch export
  const handleBatchExport = useCallback(async (names: string[]) => {
    try {
      if (names.length === 0) return;
      const json: string = await invoke("batch_export_profiles", { names });
      const path = await tauriSave({ defaultPath: "profiles.json", filters: [{ name: "JSON", extensions: ["json"] }] });
      if (!path) return;
      await invoke("write_file", { path, content: json });
      addToast("success", `已导出 ${names.length} 个 profile 到 ${path}`);
    } catch (e) { addToast("error", `批量导出失败: ${e}`); }
  }, []);

  // Import profile — parse file, show preview
  const handleImport = useCallback(async () => {
    try {
      const path = await tauriOpen({ multiple: false, filters: [{ name: "JSON", extensions: ["json"] }] });
      if (!path || typeof path !== "string") return;
      const text: string = await invoke("read_file", { path });
      const data = JSON.parse(text);
      if (!data.name || !data.env) { addToast("error", "无效的 profile 文件"); return; }
      setImportData({ name: data.name, desc: data.desc, env: data.env });
    } catch (e) { addToast("error", `读取文件失败: ${e}`); }
  }, []);

  // Import scanned profiles
  const handleScanImport = useCallback(async (items: ScanProfile[]) => {
    let imported = 0;
    let failed = 0;
    for (const item of items) {
      try {
        await ctx.addProfile(item.name, "");
        for (const [k, v] of Object.entries(item.env)) {
          if (v) await ctx.setEnvVar(item.name, k, v);
        }
        if (item.cli_type) {
          await ctx.setEnvVar(item.name, "_KN_CLI_TYPE", item.cli_type);
        }
        imported++;
      } catch (e) {
        failed++;
        addToast("error", `导入 "${item.name}" 失败: ${e}`);
      }
    }
    await ctx.loadProfiles();
    setScanData(null);
    if (imported > 0 && failed === 0) addToast("success", `已导入 ${imported}/${items.length} 个 profile`);
    else if (imported > 0) addToast("success", `已导入 ${imported}/${items.length} 个 profile，${failed} 个失败`);
    else addToast("error", "导入失败，请检查配置");
  }, [ctx]);

  // Confirm import
  const handleConfirmImport = useCallback(async (name: string) => {
    if (!importData) return;
    await ctx.addProfile(name, importData.desc || "");
    for (const [k, v] of Object.entries(importData.env)) {
      if (v) await ctx.setEnvVar(name, k, v);
    }
    await ctx.loadProfiles();
    ctx.selectProfile(name);
    setImportData(null);
    addToast("success", `已导入 "${name}"`);
  }, [ctx, importData]);

  // Check for updates
  const handleCheckUpdate = useCallback(async (opts?: { silent?: boolean }) => {
    try {
      const config: { update_url?: string } = await invoke("read_app_config");
      if (!config.update_url) {
        if (!opts?.silent) addToast("error", "未配置更新地址。请编辑 update/update.json");
        return;
      }
      const currentVersion: string = await invoke("get_app_version");

      interface UpdatePlatform { url: string; sha256?: string; signature?: string; }
      interface UpdateManifest { version: string; notes?: string; platforms: Record<string, UpdatePlatform>; }
      let manifest: UpdateManifest;
      try {
        const text = (await invoke("fetch_url", { url: config.update_url })) as string;
        if (!text.trim()) throw new Error("空响应");
        manifest = JSON.parse(text) as UpdateManifest;
      } catch (e: unknown) {
        if (!opts?.silent) addToast("error", `无法获取更新清单: ${e}\n${config.update_url}`);
        return;
      }
      if (!manifest.version || !manifest.platforms) {
        if (!opts?.silent) addToast("error", "更新清单格式无效");
        return;
      }

      if (manifest.version <= currentVersion) {
        if (!opts?.silent) addToast("success", `已是最新版本 (${currentVersion})`);
        return;
      }

      const platformInfo: { os: string; arch: string } = await invoke("get_platform_info");
      const platform = `${platformInfo.os === "macos" ? "darwin" : platformInfo.os}-${platformInfo.arch}`;
      const plat = manifest.platforms[platform] || Object.values(manifest.platforms)[0];
      if (!plat?.url) { addToast("error", `无此平台的更新包 (${platform})`); return; }

      // Show update dialog with release notes instead of auto-downloading
      setUpdateDialog({
        version: manifest.version,
        notes: manifest.notes || "",
        url: plat.url,
        sha256: plat.sha256 || "",
      });
    } catch (e) {
      if (!opts?.silent) addToast("error", `检查更新失败: ${e}`);
    }
  }, []);

  // Confirm update: download, verify, and open installer
  const handleConfirmUpdate = useCallback(async () => {
    if (!updateDialog) return;
    const { version, url, sha256 } = updateDialog;

    // Kick off download — dialog stays open and shows progress
    setDownloadState({ phase: "downloading", progress: 0, error: null });

    // Listen for progress events from Rust
    const unlisten = await listen<number>("download-progress", (event) => {
      setDownloadState((prev) =>
        prev.phase === "downloading" ? { ...prev, progress: event.payload } : prev
      );
    });

    try {
      const tmpDir: string = await invoke("temp_dir");
      const pathPart = url.split('?')[0];
      const urlExt = pathPart.split('.').pop() || "";
      // Platform-aware: .dmg on macOS, .exe on Windows, .deb on Linux
      const defaultExt = platformRef.current.os === "windows" ? "exe"
        : platformRef.current.os === "linux" ? "deb"
        : "dmg";
      const ext = urlExt || defaultExt;
      const sep = platformRef.current.os === "windows" ? "\\" : "/";
      const tmpPath = `${tmpDir}${sep}ai-profile-manager-update-${Date.now()}.${ext}`;
      await invoke("download_file", { url, path: tmpPath });

      // Download complete — 100%
      setDownloadState({ phase: "verifying", progress: 100, error: null });

      if (sha256) {
        const ok = (await invoke("verify_sha256", { path: tmpPath, expected: sha256 })) as boolean;
        if (!ok) {
          setDownloadState({ phase: "idle", progress: 0, error: "SHA256 校验失败，文件可能损坏" });
          return;
        }
      }

      // Done — show completion in dialog briefly, then open installer
      setDownloadState({ phase: "idle", progress: 100, error: null });
      await new Promise((r) => setTimeout(r, 800)); // brief pause so user sees "done"
      setUpdateDialog(null);
      setDownloadState({ phase: "idle", progress: 0, error: null });
      addToast("success", `已下载 ${version}，正在打开安装包...`);
      await invoke("open_file", { path: tmpPath });
    } catch (e: any) {
      setDownloadState({ phase: "idle", progress: 0, error: String(e) });
    }
    unlisten();
  }, [updateDialog]);

  // Auto-check for updates on startup (silent — only shows toast when update available)
  useEffect(() => {
    handleCheckUpdate({ silent: true });
  }, []);

  // ── Backup / Restore config ──────────────────────────────
  const handleBackup = useCallback(async () => {
    try {
      const msg: string = await invoke("backup_config");
      setBackupExists(true);
      addToast("success", msg);
    } catch (e) { addToast("error", `备份失败: ${e}`); }
  }, []);

  const handleRestore = useCallback(async () => {
    try {
      const msg: string = await invoke("restore_config_backup");
      await ctx.loadProfiles();
      setBackupExists(true);
      addToast("success", msg);
    } catch (e) { addToast("error", `恢复失败: ${e}`); }
  }, [ctx]);

  // Copy selected profile — prompt for new name
  const handleCopyProfile = useCallback(() => {
    if (!ctx.selectedName || !ctx.selectedProfile) return;
    const src = ctx.selectedProfile;
    setNameDialogTitle("复制 Profile");
    setNameDialogInitial(`${src.name}-copy`);
    setNameDialogOnConfirm(() => async (newName: string) => {
      await ctx.addProfile(newName, src.desc || `Copy of ${src.name}`);
      for (const [k, v] of Object.entries(src.env)) {
        await ctx.setEnvVar(newName, k, v);
      }
      await ctx.loadProfiles();
      ctx.selectProfile(newName);
      addToast("success", `Profile "${newName}" 已复制`);
    });
    setShowNameDialog(true);
  }, [ctx]);

  // Rename profile — create new, copy env, delete old
  const handleRenameProfile = useCallback((oldName: string) => {
    if (!ctx.selectedProfile) return;
    const src = ctx.selectedProfile;
    setNameDialogTitle("重命名 Profile");
    setNameDialogInitial(oldName);
    setNameDialogOnConfirm(() => async (newName: string) => {
      if (newName === oldName) return;
      if (ctx.profiles.some((p) => p.name === newName)) {
        addToast("error", `Profile "${newName}" 已存在`);
        return;
      }
      const result = await ctx.addProfile(newName, src.desc || "");
      if (!result.ok) {
        addToast("error", result.error || "创建失败");
        return;
      }
      for (const [k, v] of Object.entries(src.env)) {
        if (v) await ctx.setEnvVar(newName, k, v);
      }
      await ctx.removeProfile(oldName);
      // Clean up session history referencing the old profile name
      rightTerminal.clearProfileHistory(oldName);
      bottomTerminal.clearProfileHistory(oldName);
      await ctx.loadProfiles();
      ctx.selectProfile(newName);
      addToast("success", `已重命名为 "${newName}"`);
    });
    setShowNameDialog(true);
  }, [ctx]);

  // ── Resize handlers ───────────────────────────────────────

  // Right terminal — horizontal drag (adjusts width)
  const handleRightResize = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    const startX = e.clientX;
    const startSize = rightTerminal.size;

    const onMouseMove = (ev: MouseEvent) => {
      const delta = startX - ev.clientX;
      rightTerminal.setSize(startSize + delta);
    };

    const onMouseUp = () => {
      document.removeEventListener("mousemove", onMouseMove);
      document.removeEventListener("mouseup", onMouseUp);
    };

    document.addEventListener("mousemove", onMouseMove);
    document.addEventListener("mouseup", onMouseUp);
  }, [rightTerminal.size, rightTerminal.setSize]);

  // Bottom terminal — vertical drag (adjusts height)
  const handleBottomResize = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    const startY = e.clientY;
    const startSize = bottomTerminal.size;

    const onMouseMove = (ev: MouseEvent) => {
      const delta = startY - ev.clientY;
      bottomTerminal.setSize(startSize + delta);
    };

    const onMouseUp = () => {
      document.removeEventListener("mousemove", onMouseMove);
      document.removeEventListener("mouseup", onMouseUp);
    };

    document.addEventListener("mousemove", onMouseMove);
    document.addEventListener("mouseup", onMouseUp);
  }, [bottomTerminal.size, bottomTerminal.setSize]);

  // ── Helper: build TerminalPanel props ──────────────────────
  const buildTerminalProps = (tm: ReturnType<typeof useTerminal>) => ({
    tabs: tm.tabs,
    activeTabId: tm.activeTabId,
    history: tm.history,
    onAttachTerminal: tm.attachTerminal,
    onClose: tm.close,
    onSwitchTab: tm.switchTab,
    onCloseTab: tm.closeTab,
    onCloseOthers: tm.closeOthers,
    onCloseToRight: tm.closeToRight,
    onNewTab: () => tm.newEmptyTab(),
    onSetWorkDir: tm.setWorkDir,
    onTerminalReady: (tabId: string) => tm.handleTerminalReady(tabId),
    onTerminalResize: (tabId: string, cols: number, rows: number) => tm.handleTerminalResize(tabId, cols, rows),
    fontSize: tm.fontSize,
    onSetFontSize: (s: number) => tm.setFontSize(s),
    onResumeSession: (r: any) => tm.resumeSession(r),
    onNewSessionFromHistory: (r: any) => tm.newSessionFromHistory(r),
    onDeleteHistory: (id: string) => tm.deleteHistory(id),
    onClearHistory: () => tm.clearHistory(),
  });

  const isAnyTerminalOpen = rightTerminal.isOpen || bottomTerminal.isOpen;

  return (
    <ErrorBoundary>
    <div className="h-screen flex flex-col bg-app-bg">
      {/* Toolbar */}
      <Toolbar
        onToggleTerminal={bottomTerminal.toggle}
        onToggleWelcome={() => { setShowWelcome(!showWelcome); if (ctx.selectedName) ctx.deselect(); }}
        onCheckUpdate={handleCheckUpdate}
        onAbout={() => setShowAbout(true)}
        onSettings={() => setShowSettings(true)}
        sidebarVisible={sidebarVisible}
        onToggleSidebar={() => setSidebarVisible((v) => !v)}
        terminalVisible={bottomTerminal.isOpen}
        rightTerminalVisible={rightTerminal.isOpen}
        onToggleRightTerminal={() => rightTerminal.isOpen ? rightTerminal.hide() : rightTerminal.open()}
        envCheck={envCheck}
        onInstallTool={handleInstallTool}
        onRefreshEnvCheck={refreshEnvCheck}
        onQuickSwitcher={() => {
          setQuickSwitcherMode("profile");
          setShowQuickSwitcher(true);
        }}
        onQuickHistory={() => {
          setQuickSwitcherMode("history");
          setShowQuickSwitcher(true);
        }}
      />

      {/* Main content — ActivityBar | Sidebar/SkillManager | (MainPanel + BottomTerminal) | RightTerminal */}
      <div className="flex-1 flex flex-col overflow-hidden">
        <div className="flex-1 flex overflow-hidden min-h-0">
          {/* Activity Bar — VS Code-style left icon strip */}
          <ActivityBar active={activeActivity} onChange={setActiveActivity} />

          {/* Sidebar (Profile) / SkillManager — switch based on active activity */}
          {sidebarVisible && !rightMaximized && !bottomMaximized && activeActivity === "profile" && (
            <Sidebar
              profiles={ctx.filteredProfiles}
              selectedName={ctx.selectedName}
              searchQuery={ctx.searchQuery}
              onSelect={(name) => ctx.selectProfile(name)}
              onSearch={(query) => ctx.search(query)}
              onCopy={(name) => { ctx.selectProfile(name); handleCopyProfile(); }}
              onRename={(name) => { ctx.selectProfile(name); handleRenameProfile(name); }}
              onDelete={(name) => { ctx.selectProfile(name); setShowDeleteConfirm(true); }}
              onSetDefault={(name) => ctx.setDefault(name)}
              usageCounts={usageCounts}
              // ExpandableToolbar props
              isDefault={isDefault}
              hasSelection={!!ctx.selectedName}
              backupExists={backupExists}
              onAdd={() => setShowAddDialog(true)}
              onCopyProfile={handleCopyProfile}
              onInit={async () => {
                try {
                  const result: { profiles: ScanProfile[] } = await invoke("scan_system_configs");
                  if (result.profiles.length === 0) {
                    addToast("error", "未找到配置。\n检查: ~/.claude/settings.json 和 ~/.codex/config.json");
                    return;
                  }
                  setScanData(result.profiles);
                } catch (e) {
                  addToast("error", `扫描失败: ${e}`);
                }
              }}
              onImport={handleImport}
              onExport={handleExport}
              onBatchDelete={handleBatchDelete}
              onBatchExport={handleBatchExport}
              onRefresh={() => ctx.loadProfiles()}
              onBackup={handleBackup}
              onRestore={handleRestore}
            />
          )}

          {sidebarVisible && !rightMaximized && !bottomMaximized && activeActivity === "skills" && (
            <div className="w-[300px] shrink-0 flex flex-col border-r border-[var(--app-border)]">
              <SkillManager
                data={skillData}
                agentData={agentData}
                loading={skillDataLoading}
                selectedId={selectedSkillItem
                  && selectedSkillItem.type !== "plugin-skill"
                  && selectedSkillItem.type !== "plugin-agent"
                  && selectedSkillItem.type !== "command"
                  && selectedSkillItem.type !== "plugin-command"
                  ? getResourceData(selectedSkillItem!).id
                  : null}
                onSelect={setSelectedSkillItem}
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
                onOpenGraph={loadGraph}
                activeScope={activeScope}
                onScopeChange={setActiveScope}
                projects={projects}
                activeProject={activeProject}
                onProjectChange={setActiveProject}
                baselineUserCount={baselineCounts.user}
                baselineProjectCount={baselineCounts.project}
                onAddProject={handleAddProject}
                onMoveResource={handleMoveResource}
                onCopyResource={handleCopyResource}
                onBatchMove={handleBatchMove}
                onBatchCopy={handleBatchCopy}
                onToast={addToast}
              />
            </div>
          )}

          {/* Hooks sidebar */}
          {sidebarVisible && !rightMaximized && !bottomMaximized && activeActivity === "hooks" && (
            <div className="w-[300px] shrink-0 flex flex-col border-r border-[var(--app-border)]">
              <HookList
                data={hookData}
                loading={hookDataLoading}
                selectedId={selectedHook?.id ?? null}
                onSelect={setSelectedHook}
                onAddHook={() => setHookWizardOpen(true)}
                onOpenStore={() => setHookStoreOpen(true)}
                onRefresh={refreshHooks}
                onToggleHook={handleToggleHook}
                onDeleteHook={handleDeleteHook}
                onMoveHook={handleMoveHook}
                onCopyHook={handleCopyHook}
                onBatchMoveHooks={handleBatchMoveHooks}
                onBatchCopyHooks={handleBatchCopyHooks}
                activeScope={activeScope}
                onScopeChange={setActiveScope}
                projects={projects}
                activeProject={activeProject}
                onProjectChange={setActiveProject}
                onAddProject={handleAddProject}
              />
            </div>
          )}

          {/* Middle column: MainPanel + Bottom terminal — hidden when right panel is maximized */}
          <div className="flex-1 flex flex-col overflow-hidden min-h-0"
            style={rightMaximized ? { flex: "0 0 0px", overflow: "hidden", minWidth: 0 } : undefined}>
            {/* MainPanel / SkillDetail — hidden when either panel is maximized */}
            {!rightMaximized && !bottomMaximized && activeActivity === "skills" && showGraph && graphData && (
              <div className="flex-1 flex flex-col bg-[var(--app-bg)]">
                <div className="flex items-center px-3 py-1 border-b border-[var(--app-border)]">
                  <button
                    onClick={() => setShowGraph(false)}
                    className="text-2xs text-[var(--app-text-dim)] hover:text-[var(--app-text)] font-mono"
                  >
                    ← Back to list
                  </button>
                </div>
                <DependencyGraph data={graphData} onNodeClick={showNodeDetail} />
              </div>
            )}
            {!rightMaximized && !bottomMaximized && activeActivity === "skills" && !showGraph && (
              <div className="flex-1 overflow-y-auto bg-[var(--app-bg)] flex flex-col">
                <SkillDetail
                  item={selectedSkillItem}
                  data={skillData}
                  graphData={graphData}
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
                  onNodeClick={showNodeDetail}
                  onSelect={setSelectedSkillItem}
                />
              </div>
            )}
            {!rightMaximized && !bottomMaximized && activeActivity === "hooks" && (
              <div className="flex-1 overflow-y-auto bg-[var(--app-bg)]">
                <HookDetail
                  hook={selectedHook}
                  onRefresh={refreshHooks}
                />
              </div>
            )}
            {!rightMaximized && !bottomMaximized && activeActivity !== "skills" && activeActivity !== "hooks" && (
              <MainPanel
                profile={ctx.selectedProfile}
                hasProfiles={ctx.profiles.length > 0}
                showWelcome={showWelcome}
                allTags={Array.from(new Set(ctx.profiles.flatMap((p) => p.tags || []))).sort()}
                history={rightTerminal.history}
                onAdd={() => setShowAddDialog(true)}
                onSetEnv={async (key, value) => {
                  if (ctx.selectedName) await ctx.setEnvVar(ctx.selectedName, key, value);
                }}
                onDeleteEnv={async (key) => {
                  if (ctx.selectedName) await ctx.unsetEnvVar(ctx.selectedName, key);
                }}
                onPasteCommand={handlePasteCommand}
                onRenameProfile={handleRenameProfile}
                onResumeSession={(r) => rightTerminal.resumeSession(r)}
                onNewSessionFromHistory={(r) => rightTerminal.newSessionFromHistory(r)}
                onDeleteHistory={(id) => rightTerminal.deleteHistory(id)}
                onClearProfileHistory={(name) => rightTerminal.clearProfileHistory(name)}
                onSetTags={async (name, tags) => {
                  if (tags) await ctx.setEnvVar(name, "_KN_TAGS", tags);
                  else await ctx.unsetEnvVar(name, "_KN_TAGS");
                  await ctx.loadProfiles();
                  ctx.selectProfile(name);
                }}
                onInit={async () => {
                try {
                  const result: { profiles: ScanProfile[] } = await invoke("scan_system_configs");
                  if (result.profiles.length === 0) {
                    addToast("error", "未找到配置。\n检查: ~/.claude/settings.json 和 ~/.codex/config.json");
                    return;
                  }
                  setScanData(result.profiles);
                } catch (e) {
                  addToast("error", `扫描失败: ${e}`);
                }
              }}
              />
            )}

            {/* Bottom terminal (VS Code-style panel) — resize handle */}
            <div
              className="h-[6px] shrink-0 cursor-row-resize hover:bg-app-accent/20
                transition-colors duration-fast group/resize flex items-center justify-center"
              style={{ display: (bottomTerminal.isOpen && !bottomMaximized) ? undefined : "none" }}
              onMouseDown={handleBottomResize}
            >
              <div className="h-px w-full bg-app-border group-hover/resize:bg-app-accent/50" />
            </div>
            <TerminalPanel
              mode="bottom"
              visible={bottomTerminal.isOpen}
              size={bottomMaximized ? undefined : bottomTerminal.size}
              maximized={bottomMaximized}
              onToggleMaximize={() => { setBottomMaximized((v) => !v); setRightMaximized(false); }}
              {...buildTerminalProps(bottomTerminal)}
            />
          </div>

          {/* Right terminal (profile「运行」) — resize handle */}
          <div
            className="w-[6px] shrink-0 cursor-col-resize hover:bg-app-accent/20
              transition-colors duration-fast group/resize flex items-center justify-center"
            style={{ display: (rightTerminal.isOpen && !rightMaximized && !bottomMaximized) ? undefined : "none" }}
            onMouseDown={handleRightResize}
          >
            <div className="w-px h-full bg-app-border group-hover/resize:bg-app-accent/50" />
          </div>
          <TerminalPanel
            mode="right"
            visible={rightTerminal.isOpen && !bottomMaximized}
            size={rightMaximized ? undefined : rightTerminal.size}
            maximized={rightMaximized}
            onToggleMaximize={() => { setRightMaximized((v) => !v); setBottomMaximized(false); }}
            {...buildTerminalProps(rightTerminal)}
          />
        </div>
      </div>

      {/* Status bar */}
      <div className="flex items-center h-[26px] px-3 bg-app-statusbar border-t border-app-border select-none shrink-0 gap-3">
        <span className="text-2xs text-app-text-muted font-mono shrink-0">
          {ctx.loading ? "..." : ctx.profiles.length > 0 ? `${ctx.profiles.length} 个 profile` : "就绪"}
        </span>
        {/* PTY session indicator */}
        {isAnyTerminalOpen && (
          <span className="text-2xs text-app-accent font-mono shrink-0 flex items-center gap-1">
            <span className="w-1.5 h-1.5 rounded-full bg-app-accent" style={{ boxShadow: "0 0 4px var(--app-glow)" }} />
            终端
          </span>
        )}
        {!isAnyTerminalOpen && (
          <span className="text-2xs text-app-text-muted font-mono shrink-0 flex items-center gap-1 opacity-50">
            <span className="w-1.5 h-1.5 rounded-full bg-app-text-muted" />
            终端
          </span>
        )}
        <span className="flex-1" />
        <span className="text-2xs text-app-text-muted font-mono shrink-0">
          {colorScheme ? colorScheme.charAt(0).toUpperCase() + colorScheme.slice(1) : ""}
        </span>
        {usage.todayTokens > 0 && (
          <span
            className="text-2xs text-app-amber font-mono shrink-0 cursor-pointer hover:text-app-amber-glow transition-colors"
            onClick={() => setShowUsage(true)}
            title="查看 Token 用量"
          >
            ◉ {usage.todayTokens >= 1000 ? `${(usage.todayTokens / 1000).toFixed(1)}K` : usage.todayTokens}
          </span>
        )}
        {usage.todayTokens === 0 && !usage.loading && (
          <span
            className="text-2xs text-app-text-dim font-mono shrink-0 cursor-pointer hover:text-app-text-muted transition-colors"
            onClick={() => setShowUsage(true)}
            title="查看 Token 用量"
          >
            ◉ 0
          </span>
        )}
        <span className="text-2xs text-app-text-muted font-mono shrink-0">
          {ctx.selectedName ? (
            <>
              <span className="text-app-text-dim">{ctx.selectedName}</span>
              {ctx.defaultProfile === ctx.selectedName && (
                <span className="text-app-accent ml-1.5">(默认)</span>
              )}
            </>
          ) : "--"}
        </span>
        {appVersion && (
          <span className="text-2xs text-app-text-dim font-mono shrink-0 pl-3 border-l border-app-border">
            v{appVersion}
          </span>
        )}
      </div>

      {/* Toasts */}
      <div className="fixed bottom-8 right-4 z-50 flex flex-col gap-1.5 max-w-sm">
        {toasts.map((t) => (
          <div
            key={t.id}
            className={`flex items-start gap-2 px-3 py-2 shadow-lg border animate-[slideUp_150ms_ease-out] text-sm font-mono
              ${t.type === "error"
                ? "bg-app-red-bg border-app-border text-app-red shadow-lg"
                : "bg-app-green-bg border-app-border text-app-green shadow-lg"
              }`}
          >
            {t.type === "error"
              ? <AlertCircle size={14} className="shrink-0 mt-px" />
              : <CheckCircle2 size={14} className="shrink-0 mt-px" />
            }
            <span className="flex-1">{t.message}</span>
            {t.undoAction && (
              <button
                onClick={() => { t.undoAction!(); dismissToast(t.id); }}
                className="shrink-0 underline text-[var(--app-amber)] hover:text-[var(--app-amber-glow)] font-mono text-xs"
              >
                {t.undoLabel || "撤销"}
              </button>
            )}
            <button onClick={() => dismissToast(t.id)} className="shrink-0 opacity-60 hover:opacity-100">
              <X size={12} />
            </button>
          </div>
        ))}
      </div>

      {/* Loading indicator */}
      {ctx.loadingHeavy && (
        <div className="fixed top-0 left-0 right-0 h-0.5 z-50 overflow-hidden bg-app-bg">
          <div className="h-full bg-app-accent w-1/3 animate-[loading_1.5s_ease-in-out_infinite] shadow-[0_0_6px_var(--app-glow)]" />
        </div>
      )}

      {/* Dialogs */}
      <ProfileDialog
        open={showAddDialog}
        onClose={() => setShowAddDialog(false)}
        onRunCommand={handlePasteCommand}
        onInstallTool={handleInstallTool}
        allTags={Array.from(new Set(ctx.profiles.flatMap((p) => p.tags || []))).sort()}
        envCheck={envCheck}
        existingNames={ctx.profiles.map((p) => p.name)}
        onAdd={async (name, desc, env) => {
          await ctx.addProfile(name, desc);
          for (const [k, v] of Object.entries(env)) {
            await ctx.setEnvVar(name, k, v);
          }
          await ctx.loadProfiles();
          ctx.selectProfile(name);
          addToast("success", `Profile "${name}" 创建成功`);
        }}
      />

      {showShortcuts && <ShortcutsPanel onClose={() => setShowShortcuts(false)} />}

      <QuickSwitcher
        open={showQuickSwitcher}
        mode={quickSwitcherMode}
        onClose={() => setShowQuickSwitcher(false)}
        profiles={ctx.profiles}
        projects={projects}
        onLaunchProfile={handleQuickLaunchProfile}
        history={rightTerminal.history}
        onResumeSession={(record) => {
          // QuickSwitcher uses -c (resume last) for instant entry — no session picker
          const cmd = record.resumeLastCommand || record.command;
          rightTerminal.runInNewTab(cmd, record.workDir, cmd);
          setShowQuickSwitcher(false);
        }}
      />

      <UsagePanel open={showUsage} onClose={() => setShowUsage(false)} />

      <AboutDialog open={showAbout} onClose={() => setShowAbout(false)} />

      <SettingsDialog open={showSettings} onClose={() => setShowSettings(false)} />

      <HookWizard
        open={hookWizardOpen}
        onClose={() => setHookWizardOpen(false)}
        onCreated={(name, desc) => {
          scanMultiProjectHooks(hookScanPaths)
            .then(async (data) => {
              setHookData(data);
              if (data.hooks.length > 0) {
                const last = data.hooks[data.hooks.length - 1];
                setSelectedHook(last);
                // Save metadata if name or description provided
                if (name || desc) {
                  await invoke("set_hook_meta", {
                    cli: last.cli,
                    eventType: last.eventType,
                    groupIdx: last.groupIdx,
                    hookIdx: last.hookIdx,
                    name: name || last.eventType,
                    description: desc || null,
                  });
                  // Re-scan with metadata merged
                  refreshHooks();
                }
              }
            })
            .catch(() => {});
        }}
      />

      <HookStore
        open={hookStoreOpen}
        onClose={() => setHookStoreOpen(false)}
        onInstalled={() => {
          refreshHooks();
        }}
      />

      {updateDialog && (
        <UpdateDialog
          open={true}
          version={updateDialog.version}
          notes={updateDialog.notes}
          downloading={downloadState.phase === "downloading"}
          progress={downloadState.progress}
          downloadError={downloadState.error}
          onConfirm={handleConfirmUpdate}
          onCancel={() => {
            setUpdateDialog(null);
            setDownloadState({ phase: "idle", progress: 0, error: null });
          }}
        />
      )}

      <ImportPreview
        open={!!importData}
        data={importData}
        onConfirm={handleConfirmImport}
        onCancel={() => setImportData(null)}
      />

      <ScanPreview
        open={!!scanData}
        profiles={scanData || []}
        onImport={handleScanImport}
        onCancel={() => setScanData(null)}
      />

      <NameDialog
        open={showNameDialog}
        title={nameDialogTitle}
        initialName={nameDialogInitial}
        onConfirm={async (name) => {
          if (nameDialogOnConfirm) await nameDialogOnConfirm(name);
        }}
        onCancel={() => setShowNameDialog(false)}
      />

      {ctx.selectedName && (
        <ConfirmDialog
          open={showDeleteConfirm}
          title="删除 Profile"
          message={`确定要永久删除 "${ctx.selectedName}" 及其 ${Object.keys(ctx.selectedProfile?.env ?? {}).length} 个环境变量吗？`}
          onConfirm={async () => {
            const name = ctx.selectedName!;
            await ctx.removeProfile(name);
            // Clean up session history referencing this profile
            rightTerminal.clearProfileHistory(name);
            bottomTerminal.clearProfileHistory(name);
            setShowDeleteConfirm(false);
            addToast("success", `Profile "${name}" 已删除`);
          }}
          onCancel={() => setShowDeleteConfirm(false)}
        />
      )}

      {/* Marketplace Browser */}
      <MarketplaceBrowser
        open={marketplaceOpen}
        onClose={() => setMarketplaceOpen(false)}
        onInstalled={handleMarketplaceInstalled}
      />

      {/* Batch delete confirmation */}
      <ConfirmDialog
        open={showBatchDeleteConfirm}
        title="批量删除 Profile"
        message={`确定要永久删除选中的 ${batchDeleteNamesRef.current.length} 个 profile 吗？此操作不可撤销。`}
        confirmLabel="删除"
        onConfirm={executeBatchDelete}
        onCancel={() => { setShowBatchDeleteConfirm(false); batchDeleteNamesRef.current = []; }}
      />

      {/* Hook move/copy overwrite confirmation */}
      <ConfirmDialog
        open={overwriteConfirm !== null}
        title="目标已存在相同 Hook"
        message={`${overwriteConfirm?.targetLabel ?? ""}已存在 "${overwriteConfirm?.hookName ?? ""}"，是否覆盖？\n\n覆盖会用当前 Hook 替换目标位置的同配置 Hook。`}
        confirmLabel="覆盖"
        onConfirm={() => overwriteConfirm?.onConfirm()}
        onCancel={() => setOverwriteConfirm(null)}
      />

    </div>
    </ErrorBoundary>
  );
}
