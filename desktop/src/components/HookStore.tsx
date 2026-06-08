import { useState, useEffect, useMemo } from "react";
import { X, Terminal, Download, Check, Shield, Brush, Bot, Monitor, Bell, Clock, Search, Trash2 } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";

/* ─────────────────── Types ─────────────────── */

export interface StoreHook {
  id: string;
  name: string;
  description: string;
  category: string;
  eventType: string;
  matcher: string;
  hookType: string;
  scriptExt: string;
  compatibleClis: string[];
  platforms: string[];
  tags?: string[];
  installed?: string[];
}

export interface HookStoreData {
  hooks: StoreHook[];
}

/* ─────────────────── Constants ─────────────────── */

const CATEGORY_ICONS: Record<string, React.ReactNode> = {
  security:  <Shield size={13} />,
  quality:   <Brush size={13} />,
  automation:<Bot size={13} />,
  session:   <Monitor size={13} />,
  notification: <Bell size={13} />,
};

const CATEGORY_LABELS: Record<string, string> = {
  security: "安全防护",
  quality: "代码质量",
  automation: "自动化",
  session: "会话管理",
  notification: "通知提醒",
};

const CATEGORY_ORDER = ["security", "quality", "automation", "session", "notification"];

import { CLI_HEX_COLORS } from "../lib/cli-constants";
const CLI_COLORS: Record<string, string> = CLI_HEX_COLORS;

const CLI_LABELS: Record<string, string> = {
  claude: "Claude",
  codex: "Codex",
  qoder: "Qoder",
};

const PLATFORM_LABELS: Record<string, string> = {
  unix: "macOS / Linux",
  windows: "Windows",
};

// Detect current platform. navigator.platform is coarse but sufficient:
// Windows → "Win32", macOS → "MacIntel"/"MacPPC", Linux → "Linux"
const currentPlatform: string = (() => {
  const p = (navigator as any).userAgentData?.platform ?? navigator.platform ?? "";
  if (/Win/i.test(p)) return "windows";
  return "unix"; // macOS, Linux, others default to unix
})();

/* ─────────────────── Props ─────────────────── */

interface HookStoreProps {
  open: boolean;
  onClose: () => void;
  onInstalled: () => void;
}

/* ─────────────────── Main ─────────────────── */

export function HookStore({ open, onClose, onInstalled }: HookStoreProps) {
  const [data, setData] = useState<HookStoreData | null>(null);
  const [loading, setLoading] = useState(false);
  const [installing, setInstalling] = useState<string | null>(null); // hook_id being installed
  const [selectedHook, setSelectedHook] = useState<StoreHook | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedTag, setSelectedTag] = useState<string | null>(null);
  const [uninstalling, setUninstalling] = useState<string | null>(null); // hook_id being uninstalled
  const [error, setError] = useState("");

  useEffect(() => {
    if (!open) return;
    setLoading(true);
    setSelectedHook(null);
    setSearchQuery("");
    setSelectedTag(null);
    invoke<HookStoreData>("list_store_hooks")
      .then(setData)
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }, [open]);

  // Unique tags from all hooks (sorted alphabetically)
  const allTags = useMemo(() => {
    if (!data) return [];
    const tagSet = new Set<string>();
    data.hooks.forEach((hook) => {
      hook.tags?.forEach((t) => tagSet.add(t));
    });
    return Array.from(tagSet).sort();
  }, [data]);

  // Filtered hooks based on search query + tag filter
  const filteredHooks = useMemo(() => {
    if (!data) return null;
    const q = searchQuery.trim().toLowerCase();
    let hooks = data.hooks;

    if (q) {
      hooks = hooks.filter((hook) => {
        if (hook.name.toLowerCase().includes(q)) return true;
        if (hook.description.toLowerCase().includes(q)) return true;
        if (hook.tags?.some((t) => t.toLowerCase().includes(q))) return true;
        if (hook.category.toLowerCase().includes(q)) return true;
        if (hook.id.toLowerCase().includes(q)) return true;
        if (hook.eventType.toLowerCase().includes(q)) return true;
        return false;
      });
    }

    if (selectedTag) {
      hooks = hooks.filter((hook) => hook.tags?.includes(selectedTag));
    }

    return hooks;
  }, [data, searchQuery, selectedTag]);

  const handleInstall = async (hook: StoreHook, cli: string) => {
    setInstalling(hook.id);
    setError("");
    try {
      await invoke("install_store_hook", { hookId: hook.id, cli });
      // Refresh install status
      const fresh = await invoke<HookStoreData>("list_store_hooks");
      setData(fresh);
      setSelectedHook(fresh.hooks.find((h) => h.id === hook.id) || null);
      onInstalled();
    } catch (e: any) {
      setError(typeof e === "string" ? e : String(e));
    } finally {
      setInstalling(null);
    }
  };

  const handleUninstall = async (hook: StoreHook, cli: string) => {
    setUninstalling(hook.id);
    setError("");
    try {
      await invoke("uninstall_store_hook", { hookId: hook.id, cli });
      const fresh = await invoke<HookStoreData>("list_store_hooks");
      setData(fresh);
      setSelectedHook(fresh.hooks.find((h) => h.id === hook.id) || null);
      onInstalled();
    } catch (e: any) {
      setError(typeof e === "string" ? e : String(e));
    } finally {
      setUninstalling(null);
    }
  };

  const handleClose = () => {
    setSelectedHook(null);
    setError("");
    onClose();
  };

  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm animate-[fadeIn_100ms_ease-out]"
      onClick={(e) => e.target === e.currentTarget && handleClose()}
    >
      <div className="bg-[var(--app-panel)] border border-[var(--app-border)] shadow-dialog w-[720px] max-h-[85vh] animate-[scaleIn_150ms_ease-out] flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-[var(--app-border)]">
          <div className="flex items-center gap-2">
            <Terminal size={14} className="text-[var(--app-accent)]" />
            <span className="text-sm font-mono text-[var(--app-text)]">Hook 市场</span>
            <span className="text-2xs text-[var(--app-text-muted)] font-mono">本地</span>
          </div>
          <button onClick={handleClose} className="p-1 text-[var(--app-text-muted)] hover:text-[var(--app-text)] transition-colors">
            <X size={14} />
          </button>
        </div>

        {/* Error */}
        {error && (
          <div className="mx-4 mt-2 px-3 py-2 bg-[var(--app-red-bg)] border border-[var(--app-red-bg)] text-xs text-[var(--app-red)] font-mono rounded">
            {error}
          </div>
        )}

        {/* Body */}
        <div className="flex-1 flex overflow-hidden min-h-0">
          {/* Left: Hook list grouped by category */}
          <div className="w-[340px] shrink-0 border-r border-[var(--app-border-light)] flex flex-col min-h-0">
            {/* Search bar */}
            <div className="px-2 py-2 border-b border-[var(--app-border-light)]">
              <div className="relative">
                <Search size={12} className="absolute left-2 top-1/2 -translate-y-1/2 text-[var(--app-text-muted)]" />
                <input
                  type="text"
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                  placeholder="搜索 hook..."
                  spellCheck={false}
                  className="w-full h-[26px] pl-6 pr-6 text-2xs font-mono bg-[var(--app-input)] border border-[var(--app-border-light)]
                    text-[var(--app-text)] placeholder:text-[var(--app-text-muted)]
                    hover:border-[var(--app-border)]
                    focus:border-[var(--app-accent)] focus:shadow-[0_0_0_1px_var(--app-accent),0_0_6px_var(--app-glow)]
                    outline-none"
                />
                {searchQuery && (
                  <button
                    onClick={() => setSearchQuery("")}
                    className="absolute right-1.5 top-1/2 -translate-y-1/2 p-0.5 text-[var(--app-text-muted)] hover:text-[var(--app-text)]"
                  >
                    <X size={10} />
                  </button>
                )}
              </div>
            </div>

            {/* Tag filter chips */}
            {allTags.length > 0 && (
              <div className="px-2 py-1.5 border-b border-[var(--app-border-light)] flex flex-wrap gap-1">
                {selectedTag && (
                  <button
                    onClick={() => setSelectedTag(null)}
                    className="text-2xs font-mono px-1.5 py-px border border-[var(--app-accent)] text-[var(--app-accent)] rounded hover:bg-[var(--app-accent)]/10 transition-colors"
                  >
                    ✕ 清除筛选
                  </button>
                )}
                {allTags.map((tag) => (
                  <button
                    key={tag}
                    onClick={() => setSelectedTag(selectedTag === tag ? null : tag)}
                    className={`text-2xs font-mono px-1.5 py-px border rounded transition-colors
                      ${selectedTag === tag
                        ? "border-[var(--app-accent)] bg-[var(--app-accent)]/10 text-[var(--app-accent)]"
                        : "border-[var(--app-border-light)] text-[var(--app-text-dim)] hover:border-[var(--app-border)] hover:text-[var(--app-text)]"
                      }`}
                  >
                    {tag}
                  </button>
                ))}
              </div>
            )}

            {/* Hook list */}
            <div className="flex-1 overflow-y-auto">
            {loading ? (
              <div className="p-4 text-xs text-[var(--app-text-muted)] font-mono">加载中...</div>
            ) : data && filteredHooks && filteredHooks.length === 0 ? (
              <div className="p-4 text-center">
                <p className="text-xs text-[var(--app-text-muted)] font-mono">未找到匹配的 Hook</p>
                <p className="text-2xs text-[var(--app-text-dim)] font-mono mt-1">尝试其他关键词</p>
              </div>
            ) : data ? (
              CATEGORY_ORDER.map((cat) => {
                const hooks = filteredHooks?.filter((h) => h.category === cat) || [];
                if (hooks.length === 0) return null;
                return (
                  <div key={cat}>
                    <div className="flex items-center gap-2 px-3 py-2 text-2xs font-mono text-[var(--app-text-muted)] uppercase tracking-[0.15em] border-b border-[var(--app-border-light)] bg-[var(--app-subtle)]">
                      {CATEGORY_ICONS[cat]}
                      {CATEGORY_LABELS[cat]}
                    </div>
                    {hooks.map((hook) => {
                      const allInstalled = hook.compatibleClis.length > 0 && hook.installed?.length === hook.compatibleClis.length;
                      return (
                        <button
                          key={hook.id}
                          onClick={() => { setSelectedHook(hook); setError(""); }}
                          className={`w-full text-left px-3 py-2.5 border-b border-[var(--app-border-light)] transition-colors
                            ${selectedHook?.id === hook.id
                              ? "bg-[var(--app-accent)]/10"
                              : "hover:bg-[var(--app-hover)]"
                            }`}
                        >
                          <div className="flex items-center gap-2">
                            <span className="text-xs font-mono text-[var(--app-text)] flex-1 truncate">
                              {hook.name}
                            </span>
                            {hook.tags?.map((t) => (
                              <span key={t} className="text-2xs font-mono px-1 py-px border border-[var(--app-accent)] text-[var(--app-accent)] rounded shrink-0">
                                {t.toUpperCase()}
                              </span>
                            ))}
                            {allInstalled && <Check size={12} className="text-[var(--app-green)] shrink-0" />}
                            {hook.platforms && hook.platforms.length > 0 && (
                              <span
                                className={`text-2xs font-mono px-1.5 py-px rounded shrink-0 border ${
                                  hook.platforms.includes(currentPlatform)
                                    ? "border-[var(--app-green)] text-[var(--app-green)]"
                                    : "border-[var(--app-text-muted)] text-[var(--app-text-muted)]"
                                }`}
                                title={`支持平台: ${hook.platforms.map((p) => PLATFORM_LABELS[p] || p).join("、")}`}
                              >
                                {hook.platforms.map((p) => PLATFORM_LABELS[p] || p).join("/")}
                              </span>
                            )}
                          </div>
                          {hook.installed && hook.installed.length > 0 && !allInstalled && (
                            <div className="flex items-center gap-1 mt-0.5">
                              <Check size={10} className="text-[var(--app-green)]" />
                              <span className="text-2xs text-[var(--app-text-muted)] font-mono">
                                已安装: {hook.installed.map((c) => CLI_LABELS[c] || c).join(", ")}
                              </span>
                            </div>
                          )}
                        </button>
                      );
                    })}
                  </div>
                );
              })
            ) : (
              <div className="p-4 text-xs text-[var(--app-text-muted)] font-mono">加载失败</div>
            )}
            </div>
          </div>

          {/* Right: Hook detail + install */}
          <div className="flex-1 flex flex-col overflow-y-auto">
            {!selectedHook ? (
              <div className="flex-1 flex items-center justify-center">
                <div className="text-center">
                  <Terminal size={32} className="mx-auto text-[var(--app-text-muted)] opacity-15 mb-3" />
                  <p className="text-xs text-[var(--app-text-muted)] font-mono">
                    选择一个 Hook 查看详情
                  </p>
                </div>
              </div>
            ) : (
              <div className="flex flex-col h-full">
                {/* Detail header */}
                <div className="px-5 pt-5 pb-3">
                  <div className="flex items-center gap-2 mb-1">
                    {CATEGORY_ICONS[selectedHook.category]}
                    <span className="text-xs text-[var(--app-text-muted)] font-mono">{CATEGORY_LABELS[selectedHook.category]}</span>
                  </div>
                  <h3 className="text-base font-mono text-[var(--app-text)]">{selectedHook.name}</h3>
                  <p className="text-2xs text-[var(--app-text-dim)] font-mono mt-2 leading-relaxed">
                    {selectedHook.description}
                  </p>
                </div>

                {/* Meta */}
                <div className="px-5 py-3 border-y border-[var(--app-border-light)] space-y-1">
                  <div className="flex gap-2 text-2xs font-mono">
                    <span className="text-[var(--app-text-muted)] w-14 shrink-0">事件:</span>
                    <span className="text-[var(--app-accent)]">{selectedHook.eventType}</span>
                  </div>
                  <div className="flex gap-2 text-2xs font-mono">
                    <span className="text-[var(--app-text-muted)] w-14 shrink-0">Matcher:</span>
                    <span className="text-[var(--app-text-dim)]">{selectedHook.matcher || "(无)"}</span>
                  </div>
                  <div className="flex gap-2 text-2xs font-mono">
                    <span className="text-[var(--app-text-muted)] w-14 shrink-0">Type:</span>
                    <span className="text-[var(--app-text-dim)]">{selectedHook.hookType}</span>
                  </div>
                  <div className="flex gap-2 text-2xs font-mono">
                    <span className="text-[var(--app-text-muted)] w-14 shrink-0">平台:</span>
                    <span className={selectedHook.platforms?.includes(currentPlatform) ? "text-[var(--app-green)]" : "text-[var(--app-red)]"}>
                      {selectedHook.platforms?.map((p) => PLATFORM_LABELS[p] || p).join("、") || "未知"}
                      {!selectedHook.platforms?.includes(currentPlatform) && " (不支持当前平台)"}
                    </span>
                  </div>
                </div>

                {/* Install / Uninstall buttons */}
                <div className="px-5 py-4">
                  <span className="text-2xs text-[var(--app-text-muted)] font-mono block mb-2">
                    安装到:
                  </span>

                  {/* All installed — show summary */}
                  {selectedHook.compatibleClis.every((c) => selectedHook.installed?.includes(c)) && (
                    <div className="flex items-center gap-2 px-3 py-2 border border-[var(--app-green)]/30 bg-[var(--app-green)]/5 text-xs font-mono text-[var(--app-green)]">
                      <Check size={14} />
                      <span>已安装到全部兼容 CLI</span>
                      <span className="flex-1" />
                      <button
                        onClick={() => selectedHook.compatibleClis.forEach((c) => handleUninstall(selectedHook, c))}
                        disabled={!!uninstalling}
                        className="text-2xs text-[var(--app-text-dim)] hover:text-[var(--app-red)] transition-colors"
                      >
                        全部卸载
                      </button>
                    </div>
                  )}

                  {!selectedHook.compatibleClis.every((c) => selectedHook.installed?.includes(c)) && (
                    <>
                      <div className="flex flex-wrap gap-2">
                        {selectedHook.compatibleClis.map((cli) => {
                          const installed = selectedHook.installed?.includes(cli);
                          const isInstalling = installing === selectedHook.id;
                          const isUninstalling = uninstalling === selectedHook.id;
                          const platformOk = selectedHook.platforms?.includes(currentPlatform);
                          return installed ? (
                            <button
                              key={cli}
                              onClick={() => handleUninstall(selectedHook, cli)}
                              disabled={isUninstalling}
                              className="flex items-center gap-1.5 px-3 py-1.5 border rounded text-xs font-mono transition-colors
                                border-[var(--app-green)] text-[var(--app-green)]
                                hover:border-[var(--app-red)] hover:text-[var(--app-red)] hover:bg-[var(--app-red-bg)]
                                disabled:opacity-50"
                              title="点击卸载"
                            >
                              {isUninstalling ? (
                                <Clock size={12} className="animate-spin" />
                              ) : (
                                <Trash2 size={12} />
                              )}
                              <span>{CLI_LABELS[cli]}</span>
                            </button>
                          ) : (
                            <button
                              key={cli}
                              onClick={() => platformOk && handleInstall(selectedHook, cli)}
                              disabled={isInstalling || !platformOk}
                              className="flex items-center gap-1.5 px-3 py-1.5 border rounded text-xs font-mono transition-colors
                                border-[var(--app-border)] text-[var(--app-text-dim)] hover:border-[var(--app-accent)] hover:text-[var(--app-accent)]
                                disabled:opacity-50"
                              title={platformOk ? undefined : `此 Hook 不支持当前平台（仅支持 ${selectedHook.platforms.map((p) => PLATFORM_LABELS[p] || p).join("、")}）`}
                            >
                              {isInstalling ? (
                                <Clock size={12} className="animate-spin" />
                              ) : (
                                <Download size={12} />
                              )}
                              <span style={{ color: CLI_COLORS[cli] }}>
                                {CLI_LABELS[cli]}
                              </span>
                            </button>
                          );
                        })}
                      </div>
                      {selectedHook.platforms && !selectedHook.platforms.includes(currentPlatform) && (
                        <p className="text-2xs text-[var(--app-text-muted)] mt-2">
                          ⚠ 此 Hook 不支持当前平台
                          （支持: {selectedHook.platforms.map((p) => PLATFORM_LABELS[p] || p).join("、")}）
                        </p>
                      )}
                    </>
                  )}
                </div>

                <div className="flex-1" />
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
