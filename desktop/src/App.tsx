import React, { useEffect, useState, useCallback, useRef } from "react";
import { Toolbar } from "./components/Toolbar";
import { Sidebar } from "./components/Sidebar";
import { MainPanel } from "./components/MainPanel";
import { TerminalPanel } from "./components/TerminalPanel";
import { ProfileDialog } from "./components/ProfileDialog";
import { ConfirmDialog } from "./components/ConfirmDialog";
import { NameDialog } from "./components/NameDialog";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { ShortcutsPanel } from "./components/ShortcutsPanel";
import { ImportPreview } from "./components/ImportPreview";
import { ScanPreview, ScanProfile } from "./components/ScanPreview";
import { useProfiles } from "./hooks/useProfiles";
import { useTerminal } from "./hooks/useTerminal";
import { Command } from "@tauri-apps/plugin-shell";
import { open as tauriOpen, save as tauriSave } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { X, AlertCircle, CheckCircle2 } from "lucide-react";

type Toast = { id: number; type: "error" | "success"; message: string };

export function App() {
  const ctx = useProfiles();
  const terminal = useTerminal();
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
  const [terminalMaximized, setTerminalMaximized] = useState(false);

  // Load profiles on mount
  useEffect(() => { ctx.loadProfiles(); }, []);

  // Ensure shell wrapper is installed (ai command)
  useEffect(() => {
    invoke("ensure_shell_rc").catch(() => {});
  }, []);

  // Auto-open terminal for pop-out windows (?terminal=1)
  useEffect(() => {
    if (window.location.search.includes("terminal=1")) {
      terminal.open();
    }
  }, []);
  // Terminal error → toast
  useEffect(() => {
    terminal.setErrorCallback((msg: string) => addToast("error", msg));
    return () => terminal.setErrorCallback(() => {});
  }, []);

  // Fetch app version
  useEffect(() => {
    invoke<string>("get_app_version").then((v) => setAppVersion(v)).catch(() => {});
  }, []);

  // Environment check + install prompt

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
      // Toggle terminal with Ctrl+`
      if (e.ctrlKey && e.key === "`") {
        e.preventDefault();
        terminal.toggle();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [ctx.selectedName, terminal.toggle, showAddDialog, showDeleteConfirm, showNameDialog, showShortcuts]);

  const isDefault = ctx.selectedProfile?.is_default ?? false;

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

      await terminal.runInTerminal(cmd, selected);
    } else {
      // Non-ai commands: create new tab too
      await terminal.runInNewTab(cmd, "", cmd.slice(0, 30));
    }
  }, [terminal]);

  // Export profile to JSON file
  const handleExport = useCallback(async () => {
    if (!ctx.selectedProfile) return;
    const data = { name: ctx.selectedProfile.name, desc: ctx.selectedProfile.desc, env: ctx.selectedProfile.env, tags: ctx.selectedProfile.tags };
    try {
      const path = await tauriSave({ defaultPath: `${data.name}.json`, filters: [{ name: "JSON", extensions: ["json"] }] });
      if (!path) return;
      const json = JSON.stringify(data, null, 2);
      // Use Rust backend to write file reliably
      await invoke("write_file", { path, content: json });
      addToast("success", `已导出到 ${path}`);
    } catch (e) { addToast("error", `导出失败: ${e}`); }
  }, [ctx.selectedProfile]);

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
        const cliMap: Record<string, string> = { claude: "claude", codex: "codex" };
        if (cliMap[item.cli_type]) {
          await ctx.setEnvVar(item.name, "_KN_CLI_TYPE", cliMap[item.cli_type]);
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
  const handleCheckUpdate = useCallback(async () => {
    try {
      const config: { update_url?: string } = await invoke("read_app_config");
      if (!config.update_url) {
        addToast("error", "未配置更新地址。请编辑 update/update.json"); return;
      }
      const currentVersion: string = await invoke("get_app_version");

      // Fetch manifest via Rust (no shell scope needed)
      let manifest: any;
      try {
        const text = (await invoke("fetch_url", { url: config.update_url })) as string;
        if (!text.trim()) throw new Error("空响应");
        manifest = JSON.parse(text);
      } catch (e: any) {
        addToast("error", `无法获取更新清单: ${e}\n${config.update_url}`); return;
      }
      if (!manifest.version || !manifest.platforms) {
        addToast("error", "更新清单格式无效"); return;
      }

      // Compare versions
      if (manifest.version <= currentVersion) {
        addToast("success", `已是最新版本 (${currentVersion})`); return;
      }

      // Get platform-specific info from Rust
      const platformInfo: { os: string; arch: string } = await invoke("get_platform_info");
      const platform = `${platformInfo.os === "macos" ? "darwin" : platformInfo.os}-${platformInfo.arch}`;
      const plat = manifest.platforms[platform] || Object.values(manifest.platforms)[0] as any;
      if (!plat?.url) { addToast("error", `无此平台的更新包 (${platform})`); return; }

      addToast("success", `发现新版本 ${manifest.version}，正在下载...`);

      // Download to temp (Rust side, no shell scope)
      const tmpDir: string = await invoke("temp_dir");
      // Extract extension from URL so macOS can recognize the file type
      const pathPart = (plat.url as string).split('?')[0];
      const ext = pathPart.split('.').pop() || 'dmg';
      const tmpPath = `${tmpDir}/ai-profile-manager-update-${Date.now()}.${ext}`;
      try {
        await invoke("download_file", { url: plat.url, path: tmpPath });
      } catch (e: any) {
        addToast("error", `下载失败: ${e}`); return;
      }

      // Verify SHA256
      if (plat.sha256) {
        const ok = (await invoke("verify_sha256", { path: tmpPath, expected: plat.sha256 })) as boolean;
        if (!ok) {
          addToast("error", "SHA256 校验失败，文件可能损坏"); return;
        }
      }

      addToast("success", `已下载 ${manifest.version}，正在打开安装包...`);
      await invoke("open_file", { path: tmpPath });
    } catch (e) {
      addToast("error", `检查更新失败: ${e}`);
    }
  }, []);

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
      await ctx.loadProfiles(); // refresh sidebar counts
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
      // Check for name conflict
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
      await ctx.loadProfiles();
      ctx.selectProfile(newName);
      addToast("success", `已重命名为 "${newName}"`);
    });
    setShowNameDialog(true);
  }, [ctx]);

  // Resize handle for terminal panel
  const handleResizeStart = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    const startX = e.clientX;
    const startWidth = terminal.width;

    const onMouseMove = (ev: MouseEvent) => {
      const delta = startX - ev.clientX;
      terminal.setTerminalWidth(startWidth + delta);
    };

    const onMouseUp = () => {
      document.removeEventListener("mousemove", onMouseMove);
      document.removeEventListener("mouseup", onMouseUp);
    };

    document.addEventListener("mousemove", onMouseMove);
    document.addEventListener("mouseup", onMouseUp);
  }, [terminal]);

  return (
    <ErrorBoundary>
    <div className="h-screen flex flex-col bg-app-bg">
      {/* Toolbar */}
      <Toolbar
        selectedName={ctx.selectedName}
        isDefault={isDefault}
        onAdd={() => setShowAddDialog(true)}
        onRemove={() => setShowDeleteConfirm(true)}
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
        onToggleTerminal={terminal.toggle}
        onToggleWelcome={() => { setShowWelcome(!showWelcome); if (ctx.selectedName) ctx.deselect(); }}
        onRefresh={() => ctx.loadProfiles()}
        onExport={handleExport}
        onImport={handleImport}
        onCopyProfile={handleCopyProfile}
        hasSelection={!!ctx.selectedName}
        onCheckUpdate={handleCheckUpdate}
      />

      {/* Main content */}
      <div className="flex-1 flex overflow-hidden">
        {!terminalMaximized && (
          <>
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
            />

            <MainPanel
              profile={ctx.selectedProfile}
              hasProfiles={ctx.profiles.length > 0}
              showWelcome={showWelcome}
              allTags={Array.from(new Set(ctx.profiles.flatMap((p) => p.tags || []))).sort()}
              history={terminal.history}
              onSetEnv={async (key, value) => {
                if (ctx.selectedName) await ctx.setEnvVar(ctx.selectedName, key, value);
              }}
              onDeleteEnv={async (key) => {
                if (ctx.selectedName) await ctx.unsetEnvVar(ctx.selectedName, key);
              }}
              onPasteCommand={handlePasteCommand}
              onRenameProfile={handleRenameProfile}
              onResumeSession={(r) => terminal.resumeSession(r)}
              onNewSessionFromHistory={(r) => terminal.newSessionFromHistory(r)}
              onSetTags={async (name, tags) => {
                if (tags) await ctx.setEnvVar(name, "_KN_TAGS", tags);
                else await ctx.unsetEnvVar(name, "_KN_TAGS");
                await ctx.loadProfiles(); // refresh sidebar list + tag filter
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
          </>
        )}

        {/* Resize handle + Terminal panel */}
        {terminal.isOpen && (
          <>
            {!terminalMaximized && (
              <div
                className="w-[6px] shrink-0 cursor-col-resize hover:bg-app-accent/20
                  transition-colors duration-fast group/resize relative flex items-center justify-center"
                onMouseDown={handleResizeStart}
              >
                <div className="w-px h-full bg-app-border group-hover/resize:bg-app-accent/50" />
              </div>
            )}
            <TerminalPanel
              width={terminalMaximized ? undefined : terminal.width}
              maximized={terminalMaximized}
              onToggleMaximize={() => setTerminalMaximized((v) => !v)}
              tabs={terminal.tabs}
              activeTabId={terminal.activeTabId}
              history={terminal.history}
              onAttachTerminal={terminal.attachTerminal}
              onClose={terminal.close}
              onSwitchTab={terminal.switchTab}
              onCloseTab={terminal.closeTab}
              onNewTab={() => terminal.newEmptyTab()}
              onSetWorkDir={terminal.setWorkDir}
              onTerminalReady={(tabId) => terminal.handleTerminalReady(tabId)}
              onTerminalResize={(tabId, cols, rows) => terminal.handleTerminalResize(tabId, cols, rows)}
              fontSize={terminal.fontSize}
              onSetFontSize={(s) => terminal.setFontSize(s)}
              terminalVersion={terminal.terminalVersion}
              onResumeSession={(r) => terminal.resumeSession(r)}
              onNewSessionFromHistory={(r) => terminal.newSessionFromHistory(r)}
              onDeleteHistory={(id) => terminal.deleteHistory(id)}
              onClearHistory={() => terminal.clearHistory()}
            />
          </>
        )}
      </div>

      {/* Status bar */}
      <div className="flex items-center h-[22px] px-3 bg-app-statusbar border-t border-app-border select-none shrink-0">
        <span className="text-2xs text-app-text-muted font-mono">
          {ctx.loading ? "..." : ctx.profiles.length > 0 ? `[${ctx.profiles.length} 个 profile]` : "[就绪]"}
        </span>
        <span className="flex-1" />
        {terminal.isOpen && (
          <span className="text-2xs text-app-accent font-mono mr-3">
            终端已连接
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
        allTags={Array.from(new Set(ctx.profiles.flatMap((p) => p.tags || []))).sort()}
        onAdd={async (name, desc, env) => {
          await ctx.addProfile(name, desc);
          for (const [k, v] of Object.entries(env)) {
            await ctx.setEnvVar(name, k, v);
          }
          await ctx.loadProfiles(); // refresh sidebar with tags + CLI type
          ctx.selectProfile(name);
          addToast("success", `Profile "${name}" 创建成功`);
        }}
      />

      {showShortcuts && <ShortcutsPanel onClose={() => setShowShortcuts(false)} />}

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
            setShowDeleteConfirm(false);
            addToast("success", `Profile "${name}" 已删除`);
          }}
          onCancel={() => setShowDeleteConfirm(false)}
        />
      )}

    </div>
    </ErrorBoundary>
  );
}
