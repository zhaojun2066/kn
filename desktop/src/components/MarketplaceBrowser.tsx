import React, { useState, useEffect, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open as tauriOpen } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import {
  Search, Globe, Download, Check, X, Loader2,
  FolderOpen, Upload, Package, Filter, Plus, Trash2, Link,
} from "lucide-react";
import { ConfirmDialog } from "./ConfirmDialog";
import type { MarketplacePluginEntry, MarketplaceData } from "./ResourceList";
import type { CliKind } from "../lib/types";
import { SearchInput } from "./common/SearchInput";
import { basename } from "../lib/path-utils";

/* ──────────────────── Props ──────────────────── */

interface MarketplaceBrowserProps {
  open: boolean;
  onClose: () => void;
  onInstalled: () => void;
  /** When set, plugins/skills are installed at project level instead of user level. */
  projectPath?: string | null;
  /** Display name for project-scoped installs. */
  projectName?: string | null;
}

/* ──────────────────── Helpers ──────────────────── */

import { CLI_LABELS, CLI_CSS_COLORS, CLI_FILTER_OPTIONS } from "../lib/cli-constants";
const CLI_OPTIONS = CLI_FILTER_OPTIONS;
const CLI_LABEL = CLI_LABELS;
const CLI_COLOR = CLI_CSS_COLORS;

// ── Recommended marketplaces ───────────────────────────────────

interface RecommendedMarket {
  id: string;
  name: string;
  description: string;
  source: string;       // GitHub URL, owner/repo, or HTTPS Git URL
  cli: CliKind[];        // which CLIs can install from this market
  aliases?: string[];    // installed marketplace may have a different name
}

const RECOMMENDED_MARKETPLACES: RecommendedMarket[] = [
  {
    id: "superpowers",
    name: "Superpowers",
    description: "开发提效全套 skill：brainstorming、TDD、debugging、code review、planning…",
    source: "obra/superpowers-marketplace",
    cli: ["claude"],
    aliases: ["superpowers-marketplace"],
  },
  {
    id: "anthropic-agent-skills",
    name: "Anthropic Agent Skills",
    description: "Anthropic 官方 skill 合集",
    source: "https://github.com/anthropics/skills.git",
    cli: ["claude"],
    aliases: ["anthropic-agent-skills-marketplace"],
  },
  {
    id: "everything-claude-code",
    name: "Everything Claude Code",
    description: "社区最大的 Claude Code plugin 合集（60+ skills）",
    source: "https://github.com/affaan-m/everything-claude-code.git",
    cli: ["claude"],
    aliases: ["ecc"],
  },
];

/* ──────────────────── Install Result Toast ──────────────────── */

interface InstallResult {
  name?: string;
  marketplace?: string;
  installKey?: string;
  pluginId?: string;
  cli: string;
  success: boolean;
  message: string;
}

/* ──────────────────── Component ──────────────────── */

export function MarketplaceBrowser({ open, onClose, onInstalled, projectPath, projectName }: MarketplaceBrowserProps) {
  const [data, setData] = useState<MarketplaceData | null>(null);
  const [loading, setLoading] = useState(false);
  const [search, setSearch] = useState("");
  const [cliFilter, setCliFilter] = useState("all");
  const [marketplaceFilter, setMarketplaceFilter] = useState("all");
  const [installing, setInstalling] = useState<Set<string>>(new Set());
  const [skillSource, setSkillSource] = useState("");
  const [skillCli, setSkillCli] = useState<CliKind>("claude");
  const [skillInstalling, setSkillInstalling] = useState(false);
  const [skillMessage, setSkillMessage] = useState("");
  const [errorMsg, setErrorMsg] = useState("");

  // Marketplace management state
  const [showMarketMgr, setShowMarketMgr] = useState(false);
  const [newMarketSource, setNewMarketSource] = useState("");
  const [newMarketCli, setNewMarketCli] = useState<CliKind>("claude");
  const [addingMarket, setAddingMarket] = useState(false);
  const [removingMarket, setRemovingMarket] = useState<Set<string>>(new Set());
  const [marketMsg, setMarketMsg] = useState("");
  const [confirmRemove, setConfirmRemove] = useState<{ cli: string; name: string } | null>(null);
  const [overwriteConfirm, setOverwriteConfirm] = useState<{
    skillName: string;
    cli: string;
    sourcePath: string;
  } | null>(null);
  const [addingRecommended, setAddingRecommended] = useState<Set<string>>(new Set());

  const pluginInstallKey = (p: Pick<MarketplacePluginEntry, "cli" | "marketplace" | "name">) =>
    `${p.cli}:${p.marketplace}:${p.name}`;

  // Auto-clear messages with cleanup (prevents state updates on unmounted component)
  useEffect(() => {
    if (!errorMsg) return;
    const timer = setTimeout(() => setErrorMsg(""), 8000);
    return () => clearTimeout(timer);
  }, [errorMsg]);

  useEffect(() => {
    if (!marketMsg) return;
    const timer = setTimeout(() => setMarketMsg(""), 4000);
    return () => clearTimeout(timer);
  }, [marketMsg]);

  // Load marketplace data when opened
  useEffect(() => {
    if (!open) return;
    setLoading(true);
    setData(null);
    invoke<MarketplaceData>("list_marketplace_plugins", { cli: "all", projectPath: projectPath ?? null })
      .then(setData)
      .catch(() => setData(null))
      .finally(() => setLoading(false));
  }, [open, projectPath]);

  // Listen for install complete events
  useEffect(() => {
    if (!open) return;
    const unlistenInstall = listen<InstallResult>("plugin-install-complete", (event) => {
      const { name, success, message, installKey } = event.payload;
      // Refresh marketplace data
      invoke<MarketplaceData>("list_marketplace_plugins", { cli: "all", projectPath: projectPath ?? null })
        .then(setData)
        .catch(() => {});
      // Remove from installing set
      if (installKey || name) {
        setInstalling((prev) => {
          const next = new Set(prev);
          if (installKey) next.delete(installKey);
          if (name) next.delete(name);
          return next;
        });
      }
      if (success) {
        onInstalled();
        setErrorMsg("");
      } else {
        setErrorMsg(message || "安装失败");
      }
    });

    const unlistenUninstall = listen<InstallResult>("plugin-uninstall-complete", (_event) => {
        invoke<MarketplaceData>("list_marketplace_plugins", { cli: "all", projectPath: projectPath ?? null })
          .then(setData)
          .catch(() => {});
      onInstalled();
    });

    return () => {
      unlistenInstall.then((fn) => fn());
      unlistenUninstall.then((fn) => fn());
    };
  }, [open, onInstalled]);

  // Listen for marketplace add/remove events (secondary path — for other components' changes)
  useEffect(() => {
    if (!open) return;
    const unlisten = listen<{ cli: string; success: boolean; message: string }>(
      "marketplace-changed",
      async (event) => {
        // Refresh marketplace data
        invoke<MarketplaceData>("list_marketplace_plugins", { cli: "all", projectPath: projectPath ?? null })
          .then(setData)
          .catch(() => {});
        // Only show message if not already handled by the primary invoke path
        const { success, message } = event.payload;
        if (success) {
          setMarketMsg(message);
        }
        // Note: addingMarket/removingMarket/addingRecommended are cleared
        // by their respective handlers via the synchronous invoke return
      },
    );
    return () => { unlisten.then((fn) => fn()); };
  }, [open, onInstalled]);

  // Handlers for marketplace management
  const handleAddMarketplace = async () => {
    if (!newMarketSource.trim()) return;
    setAddingMarket(true);
    setMarketMsg("");
    setErrorMsg("");
    try {
      const msg = await invoke<string>("add_marketplace", { cli: newMarketCli, source: newMarketSource.trim() });
      setMarketMsg(msg);
      setNewMarketSource("");
      // Refresh marketplace data
      invoke<MarketplaceData>("list_marketplace_plugins", { cli: "all", projectPath: projectPath ?? null })
        .then(setData)
        .catch(() => {});
      onInstalled();
    } catch (e) {
      setErrorMsg(String(e));
    } finally {
      setAddingMarket(false);
    }
  };

  const handleAddRecommended = async (mkt: RecommendedMarket) => {
    setAddingRecommended((prev) => {
      const next = new Set(prev);
      next.add(mkt.id);
      return next;
    });
    setErrorMsg("");
    // Add for all supported CLIs sequentially, collect per-CLI errors
    const errors: string[] = [];
    for (const cli of mkt.cli) {
      try {
        await invoke("add_marketplace", { cli, source: mkt.source });
      } catch (e) {
        errors.push(`${CLI_LABEL[cli] || cli}: ${e}`);
      }
    }
    setAddingRecommended(new Set());
    if (errors.length > 0) {
      setErrorMsg(errors.join(" | "));
    } else {
      // Refresh marketplace data after successful adds
      invoke<MarketplaceData>("list_marketplace_plugins", { cli: "all", projectPath: projectPath ?? null })
        .then(setData)
        .catch(() => {});
      onInstalled();
    }
  };

  const handleRemoveMarketplace = async (cli: string, name: string) => {
    const key = `${cli}:${name}`;
    setRemovingMarket((prev) => {
      const next = new Set(prev);
      next.add(key);
      return next;
    });
    setErrorMsg("");
    try {
      const msg = await invoke<string>("remove_marketplace", { cli, name });
      setMarketMsg(msg);
      // Refresh marketplace data
      invoke<MarketplaceData>("list_marketplace_plugins", { cli: "all", projectPath: projectPath ?? null })
        .then(setData)
        .catch(() => {});
      onInstalled();
    } catch (e) {
      setErrorMsg(String(e));
    } finally {
      setRemovingMarket((prev) => {
        const next = new Set(prev);
        next.delete(key);
        return next;
      });
    }
  };

  // Filtered list
  const filtered = useMemo(() => {
    if (!data) return [];
    const q = search.toLowerCase();
    return data.plugins.filter((p) => {
      if (cliFilter !== "all" && p.cli !== cliFilter) return false;
      if (marketplaceFilter !== "all" && p.marketplace !== marketplaceFilter) return false;
      if (q && !p.name.toLowerCase().includes(q) && !(p.description || "").toLowerCase().includes(q)) return false;
      return true;
    });
  }, [data, search, cliFilter, marketplaceFilter]);

  // Which recommended marketplaces are already installed?
  // Match by id OR any alias (marketplace may get a different name after install)
  const installedMarketNames = useMemo(() => new Set(data?.marketplaces || []), [data]);
  const isRecommendedInstalled = (m: RecommendedMarket) =>
    installedMarketNames.has(m.id) || (m.aliases || []).some((a) => installedMarketNames.has(a));
  const hasRecommendedUninstalled = RECOMMENDED_MARKETPLACES.some((m) => !isRecommendedInstalled(m));

  const handleInstall = async (p: MarketplacePluginEntry) => {
    const key = pluginInstallKey(p);
    setInstalling((prev) => {
      const next = new Set(prev);
      next.add(key);
      return next;
    });
    setErrorMsg("");
    try {
      await invoke("install_plugin", { cli: p.cli, name: p.name, marketplace: p.marketplace, projectPath: projectPath ?? null });
      // Refresh + onInstalled are handled by the "plugin-install-complete" event listener above
    } catch (e) {
      setErrorMsg(String(e));
    } finally {
      setInstalling((prev) => {
        const next = new Set(prev);
        next.delete(key);
        return next;
      });
    }
  };

  const handleSelectDirectory = async () => {
    try {
      const selected = await tauriOpen({
        directory: true,
        multiple: false,
        title: "选择 Skill 目录（需包含 SKILL.md）",
      });
      if (selected && typeof selected === "string") {
        setSkillSource(selected);
        setSkillMessage("");
      }
    } catch {
      // user cancelled
    }
  };

  const handleInstallSkill = async (overwrite = false) => {
    if (!skillSource) return;
    setSkillInstalling(true);
    setSkillMessage("");
    try {
      const msg = await invoke<string>("install_standalone_skill", {
        cli: skillCli,
        sourcePath: skillSource,
        overwrite,
        projectPath: projectPath ?? null,
      });
      setSkillMessage(msg);
      setSkillSource("");
      onInstalled();
    } catch (e) {
      const errMsg = String(e);
      if (!overwrite && errMsg.includes("已存在")) {
        // Show overwrite confirmation
        const name = skillSource.split("/").pop()?.replace(/\.md$/, "") || skillSource;
        setOverwriteConfirm({
          skillName: name,
          cli: skillCli,
          sourcePath: skillSource,
        });
      } else {
        setSkillMessage(errMsg);
      }
    } finally {
      setSkillInstalling(false);
    }
  };

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-[100] flex items-center justify-center bg-black/50 backdrop-blur-sm animate-[fadeIn_150ms_ease-out]">
      <div
        className="relative w-full max-w-[720px] max-h-[85vh] flex flex-col
          bg-[var(--app-panel)] border border-[var(--app-border)] shadow-dialog"
      >
        {/* Header */}
        <div className="flex items-center gap-3 px-5 py-3 border-b border-[var(--app-border-light)]">
          <Globe size={16} className="text-[var(--app-accent)] shrink-0" />
          <div className="flex-1 min-w-0">
            <div className="text-sm font-mono text-[var(--app-text)] truncate">
              Plugin Marketplace
            </div>
            {projectPath && projectName && (
              <div className="text-2xs font-mono text-[var(--app-text-muted)] truncate" title={projectName}>
                项目 · {projectName}
              </div>
            )}
          </div>
          <button
            onClick={onClose}
            className="p-1 text-[var(--app-text-muted)] hover:text-[var(--app-text)] hover:bg-[var(--app-hover)] transition-colors"
          >
            <X size={16} />
          </button>
        </div>

        {/* Filters */}
        <div className="flex items-center gap-2 px-5 py-2.5 border-b border-[var(--app-border-light)]">
          <div className="flex-1">
            <SearchInput value={search} onChange={setSearch} placeholder="搜索 plugin..." />
          </div>

          {/* CLI filter */}
          <div className="relative">
            <select
              value={cliFilter}
              onChange={(e) => setCliFilter(e.target.value)}
              className="appearance-none bg-[var(--app-input)] border border-[var(--app-border)]
                text-2xs font-mono text-[var(--app-text-dim)] px-2 py-1.5 pr-5
                focus:outline-none focus:border-[var(--app-accent)] cursor-pointer"
            >
              {CLI_OPTIONS.map((o) => (
                <option key={o.value} value={o.value}>{o.label}</option>
              ))}
            </select>
            <Filter size={9} className="absolute right-1.5 top-1/2 -translate-y-1/2 text-[var(--app-text-muted)] pointer-events-none" />
          </div>

          {/* Marketplace filter */}
          {data && data.marketplaces.length > 1 && (
            <div className="relative">
              <select
                value={marketplaceFilter}
                onChange={(e) => setMarketplaceFilter(e.target.value)}
                className="appearance-none bg-[var(--app-input)] border border-[var(--app-border)]
                  text-2xs font-mono text-[var(--app-text-dim)] px-2 py-1.5 pr-5
                  focus:outline-none focus:border-[var(--app-accent)] cursor-pointer"
              >
                <option value="all">全部来源</option>
                {data.marketplaces.map((m) => (
                  <option key={m} value={m}>{m}</option>
                ))}
              </select>
              <Filter size={9} className="absolute right-1.5 top-1/2 -translate-y-1/2 text-[var(--app-text-muted)] pointer-events-none" />
            </div>
          )}
        </div>

        {/* Plugin list */}
        <div className="flex-1 overflow-y-auto px-5 py-3">
          {/* Error banner */}
          {errorMsg && (
            <div className="mb-3 px-3 py-2 border border-[var(--app-red)] bg-[var(--app-red-bg)] text-xs text-[var(--app-red)] font-mono">
              {errorMsg}
            </div>
          )}

          {/* Empty state: no marketplaces configured → guide user to add */}
          {!loading && data && data.marketplaces.length === 0 && (
            <div className="flex flex-col items-center justify-center py-12 gap-3">
              <Globe size={24} className="text-[var(--app-text-muted)] opacity-20" />
              <p className="text-xs text-[var(--app-text-muted)] font-mono text-center">
                尚未配置任何 Marketplace
              </p>
              <p className="text-2xs text-[var(--app-text-dim)] font-mono text-center max-w-xs">
                展开下方「管理市场来源」，手动添加或从推荐列表一键安装
              </p>
            </div>
          )}

          {loading && (
            <div className="flex items-center justify-center py-16">
              <Loader2 size={20} className="text-[var(--app-accent)] animate-spin" />
              <span className="ml-3 text-xs text-[var(--app-text-muted)] font-mono">加载 Marketplace...</span>
            </div>
          )}

          {!loading && data === null && (
            <div className="flex flex-col items-center justify-center py-16 gap-3">
              <Globe size={24} className="text-[var(--app-text-muted)] opacity-20" />
              <p className="text-xs text-[var(--app-text-muted)] font-mono text-center max-w-xs">
                无法加载 Marketplace。请确保已安装 CLI 工具（claude / codex）。
              </p>
            </div>
          )}

          {!loading && data && filtered.length === 0 && !search && (
            <div className="flex flex-col items-center justify-center py-16 gap-3">
              <Package size={24} className="text-[var(--app-text-muted)] opacity-20" />
              <p className="text-xs text-[var(--app-text-muted)] font-mono text-center max-w-xs">
                未找到任何 plugin。请先添加 marketplace：
              </p>
              <code className="text-2xs text-[var(--app-text-dim)] bg-[var(--app-input)] px-2 py-1 border border-[var(--app-border)] font-mono">
                claude plugin marketplace add &lt;url&gt;
              </code>
            </div>
          )}

          {!loading && data && filtered.length === 0 && search && (
            <div className="flex flex-col items-center justify-center py-16 gap-2">
              <Search size={18} className="text-[var(--app-text-muted)] opacity-25" />
              <span className="text-xs text-[var(--app-text-muted)] font-mono">无匹配项</span>
            </div>
          )}

          {!loading && data && filtered.length > 0 && (
            <div className="space-y-2">
              {filtered.map((p) => {
                const id = `${p.cli}:${p.marketplace}:${p.name}`;
                const isInstalling = installing.has(id);
                const isProjectMode = !!projectPath;
                const userInstalled = p.userInstalled ?? p.installed;
                const projectInstalled = p.projectInstalled ?? p.installed;
                const isInstalledHere = isProjectMode ? projectInstalled : p.installed;
                return (
                  <div
                    key={id}
                    className="flex items-center gap-3 px-3 py-2.5 border border-[var(--app-border-light)]
                      bg-[var(--app-bg)] hover:bg-[var(--app-hover)] transition-colors duration-fast group"
                  >
                    {/* Icon */}
                    <div className="w-8 h-8 flex items-center justify-center shrink-0 border border-[var(--app-border)]
                      bg-[var(--app-input)]">
                      <Package size={14} style={{ color: CLI_COLOR[p.cli] || "var(--app-text-muted)" }} />
                    </div>

                    {/* Info */}
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2">
                        <span className="text-sm font-mono text-[var(--app-text)] truncate">{p.name}</span>
                        <span
                          className="text-2xs font-mono px-1 py-0 leading-none border whitespace-nowrap shrink-0"
                          style={{ color: CLI_COLOR[p.cli] || "var(--app-text-muted)", borderColor: CLI_COLOR[p.cli] || "var(--app-text-muted)", opacity: 0.7 }}
                        >
                          {CLI_LABEL[p.cli] || p.cli}
                        </span>
                        {p.version && (
                          <span className="text-2xs text-[var(--app-text-muted)] font-mono">v{p.version}</span>
                        )}
                      </div>
                      <div className="flex items-center gap-2 mt-0.5">
                        <span className="text-2xs text-[var(--app-text-muted)] font-mono">
                          @{p.marketplace}
                        </span>
                        {isProjectMode && userInstalled && !projectInstalled && (
                          <span className="text-2xs text-[var(--app-text-dim)] font-mono">
                            用户级已安装
                          </span>
                        )}
                        {p.skillCount > 0 && (
                          <span className="text-2xs text-[var(--app-text-dim)] font-mono">
                            {p.skillCount} skills
                          </span>
                        )}
                      </div>
                      {p.description && (
                        <p className="text-2xs text-[var(--app-text-dim)] mt-1 line-clamp-1">{p.description}</p>
                      )}
                    </div>

                    {/* Action */}
                    {isInstalledHere ? (
                      <span className="flex items-center gap-1 text-2xs text-[var(--app-accent)] font-mono shrink-0 opacity-70">
                        <Check size={11} />
                        {isProjectMode ? "项目已安装" : "已安装"}
                      </span>
                    ) : (
                      <button
                        onClick={() => handleInstall(p)}
                        disabled={isInstalling}
                        className={`flex items-center gap-1 px-3 py-1 text-xs font-mono border transition-all duration-fast shrink-0
                          ${isInstalling
                            ? "border-[var(--app-border)] text-[var(--app-text-muted)] cursor-wait"
                            : "border-[var(--app-accent)] text-[var(--app-accent)] hover:bg-[var(--app-green-bg)]"
                          }`}
                      >
                        {isInstalling ? (
                          <>
                            <Loader2 size={11} className="animate-spin" />
                            安装中
                          </>
                        ) : (
                          <>
                            <Download size={11} />
                            {isProjectMode && userInstalled ? "加入项目" : isProjectMode ? "安装到项目" : "安装"}
                          </>
                        )}
                      </button>
                    )}
                  </div>
                );
              })}
            </div>
          )}
        </div>

        {/* ── Local Skill Install Section ── */}
        <div className="border-t border-[var(--app-border)] px-5 py-3 bg-[var(--app-bg)]">
          <div className="flex items-center gap-2 mb-2.5">
            <FolderOpen size={12} className="text-[var(--app-text-muted)]" />
            <span className="text-2xs text-[var(--app-text-muted)] font-mono uppercase tracking-[0.15em]">
              本地安装 Skill
            </span>
          </div>

          <div className="flex items-center gap-2">
            {/* CLI selector */}
            <select
              value={skillCli}
              onChange={(e) => setSkillCli(e.target.value as CliKind)}
              className="appearance-none bg-[var(--app-input)] border border-[var(--app-border)]
                text-2xs font-mono text-[var(--app-text-dim)] px-2 py-1.5
                focus:outline-none focus:border-[var(--app-accent)] cursor-pointer shrink-0"
            >
              <option value="claude">Claude</option>
              <option value="codex">Codex</option>
              <option value="qoder">Qoder</option>
            </select>

            {/* File path display / picker */}
            <button
              onClick={handleSelectDirectory}
              className="flex items-center gap-1.5 flex-1 px-3 py-1.5 border border-dashed
                border-[var(--app-border)] text-2xs text-[var(--app-text-muted)] font-mono
                hover:border-[var(--app-accent)] hover:text-[var(--app-text)] transition-colors"
            >
              <Upload size={11} />
              {skillSource ? (
                <span className="truncate text-[var(--app-text-dim)]">{basename(skillSource)}</span>
              ) : (
                <span>选择 Skill 目录...</span>
              )}
            </button>

            {/* Install button */}
            <button
              onClick={() => handleInstallSkill()}
              disabled={!skillSource || skillInstalling}
              className={`flex items-center gap-1 px-3 py-1.5 text-xs font-mono border transition-all duration-fast shrink-0
                ${!skillSource
                  ? "border-[var(--app-border)] text-[var(--app-text-muted)] opacity-40 cursor-not-allowed"
                  : skillInstalling
                    ? "border-[var(--app-border)] text-[var(--app-text-muted)]"
                    : "border-[var(--app-accent)] text-[var(--app-accent)] hover:bg-[var(--app-green-bg)]"
                }`}
            >
              {skillInstalling ? (
                <>
                  <Loader2 size={11} className="animate-spin" />
                  安装中
                </>
              ) : (
                <>
                  <Download size={11} />
                  安装
                </>
              )}
            </button>
          </div>

          {/* Skill install result message */}
          {skillMessage && (
            <p className={`mt-2 text-2xs font-mono ${skillMessage.includes("成功") ? "text-[var(--app-accent)]" : "text-[var(--app-red)]"}`}>
              {skillMessage}
            </p>
          )}

          {/* Clear selection */}
          {skillSource && (
            <button
              onClick={() => { setSkillSource(""); setSkillMessage(""); }}
              className="mt-1.5 text-2xs text-[var(--app-text-muted)] hover:text-[var(--app-text)] font-mono transition-colors"
            >
              ✕ 清除选择
            </button>
          )}
        </div>

        {/* ── Marketplace Management Section ── */}
        <div className="border-t border-[var(--app-border)] px-5 py-3 bg-[var(--app-bg)]">
          <button
            onClick={() => setShowMarketMgr(!showMarketMgr)}
            className="flex items-center gap-2 w-full text-left hover:bg-[var(--app-hover)] transition-colors -mx-2 px-2 py-1"
          >
            <Link size={12} className="text-[var(--app-text-muted)]" />
            <span className="text-2xs text-[var(--app-text-muted)] font-mono uppercase tracking-[0.15em] flex-1">
              管理市场来源
            </span>
            <span className="text-2xs text-[var(--app-text-muted)] font-mono tabular-nums mr-1">
              {data ? data.marketplaces.length : 0}
            </span>
            <svg
              width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor"
              strokeWidth="2" strokeLinecap="round"
              className={`text-[var(--app-text-muted)] transition-transform duration-200 ${showMarketMgr ? "rotate-90" : ""}`}
            >
              <polyline points="9 18 15 12 9 6" />
            </svg>
          </button>

          {showMarketMgr && (
            <div className="mt-2 space-y-2">
              {/* Market message */}
              {marketMsg && (
                <div className="px-2 py-1 text-2xs text-[var(--app-accent)] font-mono bg-[var(--app-green-bg)] border border-[var(--app-border)]">
                  {marketMsg}
                </div>
              )}

              {/* Current marketplaces list */}
              {data && data.marketplaces.length > 0 && (
                <div className="space-y-1 max-h-[150px] overflow-y-auto">
                  {data.marketplaces.map((mkt) => {
                    const cliPlugins = data.plugins.filter((p) => p.marketplace === mkt);
                    const cliSet = new Set(cliPlugins.map((p) => p.cli));
                    const cliLabels = Array.from(cliSet).map((c) => CLI_LABEL[c] || c).join(", ");

                    return (
                      <div
                        key={mkt}
                        className="flex items-center gap-2 px-2 py-1.5 border border-[var(--app-border-light)]
                          bg-[var(--app-input)] group"
                      >
                        <Globe size={11} className="text-[var(--app-text-muted)] shrink-0" />
                        <span className="text-2xs font-mono text-[var(--app-text)] flex-1 truncate">{mkt}</span>
                        {cliLabels && (
                          <span className="text-2xs text-[var(--app-text-muted)] font-mono shrink-0 opacity-60">{cliLabels}</span>
                        )}
                        {cliSet.size > 0 && (
                          <button
                            onClick={() => {
                              const cli = cliSet.values().next().value as string;
                              if (cli) setConfirmRemove({ cli, name: mkt });
                            }}
                            disabled={removingMarket.has(`${cliSet.values().next().value}:${mkt}`)}
                            className="p-0.5 text-[var(--app-text-muted)] hover:text-[var(--app-red)] hover:bg-[var(--app-hover)]
                              transition-colors opacity-0 group-hover:opacity-100 shrink-0"
                            title={`移除 ${mkt}`}
                          >
                            {removingMarket.has(`${cliSet.values().next().value}:${mkt}`)
                              ? <Loader2 size={11} className="animate-spin" />
                              : <Trash2 size={11} />
                            }
                          </button>
                        )}
                      </div>
                    );
                  })}
                </div>
              )}

              {/* ── Recommended Marketplaces ── */}
              {hasRecommendedUninstalled && (
                <div className="border border-[var(--app-border-light)] bg-[var(--app-bg)]">
                  <div className="flex items-center gap-1.5 px-2 py-1.5 border-b border-[var(--app-border-light)]">
                    <span className="text-2xs text-[var(--app-amber)] font-mono uppercase tracking-[0.15em]">推荐</span>
                    <span className="text-2xs text-[var(--app-text-muted)] font-mono opacity-50">— 一键添加</span>
                  </div>
                  <div className="space-y-1 p-1.5 max-h-[200px] overflow-y-auto">
                    {RECOMMENDED_MARKETPLACES.map((mkt) => {
                      const installed = isRecommendedInstalled(mkt);
                      const isAdding = addingRecommended.has(mkt.id);
                      return (
                        <div
                          key={mkt.id}
                          className={`flex items-center gap-2 px-2 py-1.5 transition-colors duration-fast
                            ${installed
                              ? "opacity-50"
                              : "hover:bg-[var(--app-hover)]"
                            }`}
                        >
                          <div className="flex-1 min-w-0">
                            <div className="flex items-center gap-1.5">
                              <span className="text-2xs font-mono text-[var(--app-text)]">{mkt.name}</span>
                              <span className="text-2xs text-[var(--app-text-muted)] opacity-50">{mkt.description.slice(0, 30)}…</span>
                            </div>
                          </div>
                          <div className="flex items-center gap-1 shrink-0">
                            {mkt.cli.map((c) => (
                              <span
                                key={c}
                                className="text-2xs font-mono px-0.5 leading-none border opacity-60"
                                style={{ color: CLI_COLOR[c], borderColor: CLI_COLOR[c] }}
                              >
                                {CLI_LABEL[c]}
                              </span>
                            ))}
                          </div>
                          {installed ? (
                            <span className="text-2xs text-[var(--app-accent)] font-mono shrink-0 opacity-60"><Check size={10} className="inline" /> 已添加</span>
                          ) : (
                            <button
                              onClick={() => handleAddRecommended(mkt)}
                              disabled={isAdding}
                              className={`text-2xs font-mono px-1.5 py-0.5 border transition-all shrink-0
                                ${isAdding
                                  ? "border-[var(--app-border)] text-[var(--app-text-muted)]"
                                  : "border-[var(--app-amber)] text-[var(--app-amber)] hover:bg-[var(--app-amber-bg)]"
                                }`}
                            >
                              {isAdding ? <Loader2 size={10} className="animate-spin inline" /> : "添加"}
                            </button>
                          )}
                        </div>
                      );
                    })}
                  </div>
                </div>
              )}

              {/* Add marketplace form */}
              <div className="flex items-center gap-2 pt-1">
                <select
                  value={newMarketCli}
                  onChange={(e) => setNewMarketCli(e.target.value as CliKind)}
                  className="appearance-none bg-[var(--app-input)] border border-[var(--app-border)]
                    text-2xs font-mono text-[var(--app-text-dim)] px-2 py-1.5
                    focus:outline-none focus:border-[var(--app-accent)] cursor-pointer shrink-0"
                >
                  <option value="claude">Claude</option>
                  <option value="codex">Codex</option>
                </select>
                <input
                  value={newMarketSource}
                  onChange={(e) => setNewMarketSource(e.target.value)}
                  onKeyDown={(e) => { if (e.key === "Enter") handleAddMarketplace(); }}
                  placeholder="GitHub URL 或 owner/repo..."
                  className="flex-1 bg-[var(--app-input)] border border-[var(--app-border)]
                    text-2xs font-mono text-[var(--app-text)] px-2 py-1.5
                    placeholder:text-[var(--app-text-muted)]
                    focus:outline-none focus:border-[var(--app-accent)]"
                />
                <button
                  onClick={handleAddMarketplace}
                  disabled={!newMarketSource.trim() || addingMarket}
                  className={`flex items-center gap-1 px-2.5 py-1.5 text-xs font-mono border transition-all duration-fast shrink-0
                    ${!newMarketSource.trim()
                      ? "border-[var(--app-border)] text-[var(--app-text-muted)] opacity-40 cursor-not-allowed"
                      : "border-[var(--app-accent)] text-[var(--app-accent)] hover:bg-[var(--app-green-bg)]"
                    }`}
                >
                  {addingMarket ? (
                    <Loader2 size={11} className="animate-spin" />
                  ) : (
                    <Plus size={11} />
                  )}
                  <span>添加</span>
                </button>
              </div>
              <p className="text-2xs text-[var(--app-text-muted)] font-mono opacity-50">
                支持 GitHub URL、owner/repo 或本地路径
              </p>
            </div>
          )}
        </div>
      </div>

      {confirmRemove && (
        <ConfirmDialog
          open={true}
          title="移除 Marketplace"
          message={`确定要移除 Marketplace "${confirmRemove.name}" 吗？`}
          confirmLabel="移除"
          onConfirm={() => {
            handleRemoveMarketplace(confirmRemove.cli, confirmRemove.name);
            setConfirmRemove(null);
          }}
          onCancel={() => setConfirmRemove(null)}
        />
      )}

      {/* Skill overwrite confirmation */}
      {overwriteConfirm && (
        <ConfirmDialog
          open={true}
          title="覆盖确认"
          message={`${overwriteConfirm.cli} 已有 Skill "${overwriteConfirm.skillName}"，是否覆盖安装？\n\n覆盖会用新的版本替换已有 Skill。`}
          confirmLabel="覆盖"
          onConfirm={() => {
            setOverwriteConfirm(null);
            setSkillSource(overwriteConfirm.sourcePath);
            setSkillCli(overwriteConfirm.cli as CliKind);
            handleInstallSkill(true);
          }}
          onCancel={() => {
            setOverwriteConfirm(null);
            setSkillInstalling(false);
          }}
        />
      )}
    </div>
  );
}
