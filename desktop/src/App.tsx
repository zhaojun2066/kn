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
import { UsagePanel } from "./components/UsagePanel";
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
import { Command } from "@tauri-apps/plugin-shell";
import { open as tauriOpen, save as tauriSave } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { X, AlertCircle, CheckCircle2 } from "lucide-react";

type Toast = { id: number; type: "error" | "success"; message: string };

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
  const usage = useUsage();
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
      if ((e.metaKey || e.ctrlKey) && e.key === "k") {
        e.preventDefault();
        setShowShortcuts((prev) => !prev);
      }
      if ((e.metaKey || e.ctrlKey) && e.key === "n") {
        e.preventDefault();
        setShowAddDialog(true);
      }
      if (e.key === "Escape") {
        if (showAddDialog) setShowAddDialog(false);
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
  }, [ctx.selectedName, bottomTerminal.toggle, showAddDialog, showDeleteConfirm, showNameDialog, showShortcuts]);

  const isDefault = ctx.selectedProfile?.is_default ?? false;

  const handleInstallTool = useCallback(async (cmd: string) => {
    // Open bottom terminal and auto-execute the install command
    bottomTerminal.runInTerminal(cmd, "");
  }, [bottomTerminal]);

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
      } catch { /* individual profile import failed, continue with others */ }
    }
    await ctx.loadProfiles();
    setScanData(null);
    if (imported > 0) addToast("success", `已导入 ${imported}/${items.length} 个 profile`);
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

      let manifest: any;
      try {
        const text = (await invoke("fetch_url", { url: config.update_url })) as string;
        if (!text.trim()) throw new Error("空响应");
        manifest = JSON.parse(text);
      } catch (e: any) {
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
      const plat = manifest.platforms[platform] || Object.values(manifest.platforms)[0] as any;
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
        selectedName={ctx.selectedName}
        isDefault={isDefault}
        onAdd={() => setShowAddDialog(true)}
        onSetDefault={(name) => ctx.setDefault(name)}
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
        sidebarVisible={sidebarVisible}
        onToggleSidebar={() => setSidebarVisible((v) => !v)}
        terminalVisible={bottomTerminal.isOpen}
        rightTerminalVisible={rightTerminal.isOpen}
        onToggleRightTerminal={() => rightTerminal.isOpen ? rightTerminal.hide() : rightTerminal.open()}
        onToggleTerminal={bottomTerminal.toggle}
        onToggleWelcome={() => { setShowWelcome(!showWelcome); if (ctx.selectedName) ctx.deselect(); }}
        onRefresh={() => ctx.loadProfiles()}
        onImport={handleImport}
        onCopyProfile={handleCopyProfile}
        hasSelection={!!ctx.selectedName}
        onCheckUpdate={handleCheckUpdate}
        onBackup={handleBackup}
        onRestore={handleRestore}
        backupExists={backupExists}
        envCheck={envCheck}
        onInstallTool={handleInstallTool}
        onRefreshEnvCheck={refreshEnvCheck}
        onAbout={() => setShowAbout(true)}
        onSettings={() => setShowSettings(true)}
      />

      {/* Main content — VS Code-style: Sidebar | (MainPanel + BottomTerminal) | RightTerminal */}
      <div className="flex-1 flex flex-col overflow-hidden">
        <div className="flex-1 flex overflow-hidden min-h-0">
          {/* Sidebar — hidden when either panel is maximized, or toggled off */}
          {sidebarVisible && !rightMaximized && !bottomMaximized && (
            <Sidebar
              profiles={ctx.filteredProfiles}
              selectedName={ctx.selectedName}
              searchQuery={ctx.searchQuery}
              onSelect={(name) => ctx.selectProfile(name)}
              onSearch={(query) => ctx.search(query)}
              onCopy={handleCopyProfile}
              onRename={handleRenameProfile}
              onDelete={(name) => { ctx.selectProfile(name); setShowDeleteConfirm(true); }}
              onSetDefault={(name) => ctx.setDefault(name)}
              usageCounts={usageCounts}
              onBatchDelete={handleBatchDelete}
              onBatchExport={handleBatchExport}
            />
          )}

          {/* Middle column: MainPanel + Bottom terminal — hidden when right panel is maximized */}
          {!rightMaximized && (
          <div className="flex-1 flex flex-col overflow-hidden min-h-0">
            {/* MainPanel — hidden when either panel is maximized */}
            {!rightMaximized && !bottomMaximized && (
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

            {/* Bottom terminal (VS Code-style panel) */}
            {bottomTerminal.isOpen && !bottomMaximized && (
              <div
                className="h-[6px] shrink-0 cursor-row-resize hover:bg-app-accent/20
                  transition-colors duration-fast group/resize flex items-center justify-center"
                onMouseDown={handleBottomResize}
              >
                <div className="h-px w-full bg-app-border group-hover/resize:bg-app-accent/50" />
              </div>
            )}
            {bottomTerminal.isOpen && (
              <TerminalPanel
                mode="bottom"
                size={bottomMaximized ? undefined : bottomTerminal.size}
                maximized={bottomMaximized}
                onToggleMaximize={() => { setBottomMaximized((v) => !v); setRightMaximized(false); }}
                {...buildTerminalProps(bottomTerminal)}
              />
            )}
          </div>
          )}

          {/* Right terminal (profile「运行」) */}
          {rightTerminal.isOpen && !rightMaximized && !bottomMaximized && (
            <div
              className="w-[6px] shrink-0 cursor-col-resize hover:bg-app-accent/20
                transition-colors duration-fast group/resize flex items-center justify-center"
              onMouseDown={handleRightResize}
            >
              <div className="w-px h-full bg-app-border group-hover/resize:bg-app-accent/50" />
            </div>
          )}
          {rightTerminal.isOpen && !bottomMaximized && (
            <TerminalPanel
              mode="right"
              size={rightMaximized ? undefined : rightTerminal.size}
              maximized={rightMaximized}
              onToggleMaximize={() => { setRightMaximized((v) => !v); setBottomMaximized(false); }}
              {...buildTerminalProps(rightTerminal)}
            />
          )}
        </div>
      </div>

      {/* Status bar */}
      <div className="flex items-center h-[22px] px-3 bg-app-statusbar border-t border-app-border select-none shrink-0">
        <span className="text-2xs text-app-text-muted font-mono">
          {ctx.loading ? "..." : ctx.profiles.length > 0 ? `[${ctx.profiles.length} 个 profile]` : "[就绪]"}
        </span>
        <span className="flex-1" />
        {isAnyTerminalOpen && (
          <span className="text-2xs text-app-accent font-mono mr-3">
            终端已连接
          </span>
        )}
        {usage.todayTokens > 0 && (
          <span
            className="text-2xs text-app-amber font-mono mr-3 cursor-pointer hover:text-app-amber-glow transition-colors"
            onClick={() => setShowUsage(true)}
            title="查看 Token 用量"
          >
            ◉ {usage.todayTokens >= 1000 ? `${(usage.todayTokens / 1000).toFixed(1)}K` : usage.todayTokens} 今天
          </span>
        )}
        {usage.todayTokens === 0 && !usage.loading && (
          <span
            className="text-2xs text-app-text-dim font-mono mr-3 cursor-pointer hover:text-app-text-muted transition-colors"
            onClick={() => setShowUsage(true)}
            title="查看 Token 用量"
          >
            ◉ 用量
          </span>
        )}
        <span className="text-2xs text-app-text-muted font-mono">
          {ctx.selectedName && (
            <>
              <span className="text-app-text-dim">{ctx.selectedName}</span>
              {ctx.defaultProfile === ctx.selectedName && (
                <span className="text-app-accent ml-2">(default)</span>
              )}
            </>
          )}
          {!ctx.selectedName && "-- 未选择 --"}
        </span>
        {appVersion && (
          <span className="text-2xs text-app-text-dim font-mono ml-3 pl-3 border-l border-app-border">
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

      <UsagePanel open={showUsage} onClose={() => setShowUsage(false)} />

      <AboutDialog open={showAbout} onClose={() => setShowAbout(false)} />

      <SettingsDialog open={showSettings} onClose={() => setShowSettings(false)} />

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

      {/* Batch delete confirmation */}
      <ConfirmDialog
        open={showBatchDeleteConfirm}
        title="批量删除 Profile"
        message={`确定要永久删除选中的 ${batchDeleteNamesRef.current.length} 个 profile 吗？此操作不可撤销。`}
        confirmLabel="删除"
        onConfirm={executeBatchDelete}
        onCancel={() => { setShowBatchDeleteConfirm(false); batchDeleteNamesRef.current = []; }}
      />

    </div>
    </ErrorBoundary>
  );
}
