import React, { useState, useEffect, useMemo, useRef, useCallback } from "react";
import {
  Bot, Box, Puzzle, FileText, Lock, Circle, ExternalLink,
  Layers, FolderOpen, Link2, File, Cpu, ArrowUpCircle,
  Play, Ban, Trash2, Wrench, Sparkles, ArrowUpRight, Terminal,
  FolderTree, List, Image,
} from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { ConfirmDialog } from "./ConfirmDialog";
import { Button } from "./common/Button";
import { AgentDetail } from "./AgentDetail";
import { FileTree } from "./FileTree";
import type { FileTreeNode } from "./FileTree";
import type { SelectedItem, CliKind, PluginUpdateInfo, CommandEntry } from "./SkillManager";
import type { DependencyGraphData } from "./DependencyGraph";
import { CLI_LABELS, CLI_CSS_COLORS } from "../lib/cli-constants";
import { basename, dirname } from "../lib/path-utils";
import { FileContentBlock, isImagePath, isPdfPath, langFromPath } from "./common/FileContentBlock";

interface ConfirmState {
  title: string;
  message: string;
  confirmLabel: string;
  onConfirm: () => void;
}

/* ──────────────────── Props ──────────────────── */

interface SkillDetailProps {
  item: SelectedItem | null;
  data: {
    plugins: { id: string; name: string; marketplace: string; version?: string; source: string; skills: { name: string; path: string; description?: string }[] }[];
  } | null;
  graphData?: DependencyGraphData | null;
  onTogglePlugin: (cli: CliKind, pluginId: string, enabled: boolean) => void;
  onToggleStandaloneSkill: (cli: CliKind, skillId: string, enabled: boolean, path?: string) => void;
  updateInfos: PluginUpdateInfo[];
  onUpdatePlugin: (cli: CliKind, pluginId: string) => void;
  onUninstallPlugin: (cli: CliKind, pluginId: string) => void;
  onUninstallStandaloneSkill: (cli: CliKind, skillId: string, path?: string, name?: string) => void;
  onToggleAgent: (cli: CliKind, name: string, enabled: boolean, path?: string) => void;
  onDeleteAgent: (cli: CliKind, name: string, path?: string) => void;
  onToggleCommand?: (cli: CliKind, name: string, enabled: boolean, path?: string) => void;
  onUninstallCommand?: (cli: CliKind, name: string, path?: string) => void;
  onNodeClick?: (nodeId: string) => void;
  onSelect?: (item: SelectedItem) => void;
}

/* ──────────────────── Helpers ──────────────────── */

/* CLI_LABELS and CLI_CSS_COLORS are now imported from lib/cli-constants */

function MetaRow({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex items-baseline gap-2 py-0.5">
      <span className="text-2xs text-[var(--app-text-muted)] font-mono uppercase tracking-[0.1em] w-16 shrink-0">
        {label}
      </span>
      <span className="text-xs text-[var(--app-text-dim)] font-mono">{children}</span>
    </div>
  );
}

function SectionHeader({ icon, label }: { icon: React.ReactNode; label: string }) {
  return (
    <div className="flex items-center gap-2 mb-3 pt-1">
      {icon}
      <span className="text-2xs text-[var(--app-text-muted)] font-mono uppercase tracking-[0.2em]">
        {label}
      </span>
      <div className="flex-1 border-b border-[var(--app-border-light)]" />
    </div>
  );
}

/* FileContentBlock + helpers now imported from common/FileContentBlock */

/* ──────────────────── Plugin Detail ──────────────────── */

function PluginDetail({
  plugin,
  onToggle,
  updateInfo,
  onUpdate,
  onUninstall,
  onSkillClick,
  onAgentClick,
  onCommandClick,
}: {
  plugin: NonNullable<SkillDetailProps["item"]>["data"];
  onToggle: (enabled: boolean) => void;
  updateInfo?: PluginUpdateInfo;
  onUpdate: (cli: CliKind, pluginId: string) => void;
  onUninstall: (cli: CliKind, pluginId: string) => void;
  onSkillClick?: (skill: { name: string; path: string; description?: string }, cli: string) => void;
  onAgentClick?: (agent: any) => void;
  onCommandClick?: (command: CommandEntry) => void;
}) {
  const [confirm, setConfirm] = useState<ConfirmState | null>(null);
  const [tab, setTab] = useState<"skills" | "agents" | "commands">("skills");
  const [viewMode, setViewMode] = useState<"list" | "file">("list");
  const [fileActivePath, setFileActivePath] = useState<string>("");
  const [fileContent, setFileContent] = useState<string>("");
  const [fileError, setFileError] = useState<string>("");
  if (!("marketplace" in plugin)) return null;
  const p = plugin as Extract<typeof plugin, { marketplace: string }>;

  // Derive plugin root dir from first skill/agent/command path
  const pluginRootDir = useMemo(() => {
    const firstPath = p.skills[0]?.path || p.agents[0]?.path || p.commands?.[0]?.path;
    if (!firstPath) return "";
    return dirname(firstPath);
  }, [p.skills, p.agents, p.commands]);

  // Reset file state when plugin changes
  useEffect(() => {
    setFileActivePath("");
    setFileContent("");
    setFileError("");
  }, [p.id]);

  // Race-condition-safe file reader
  const loadIdRef = useRef(0);
  const handleFileSelect = useCallback((node: FileTreeNode) => {
    if (node.is_dir) return;
    const id = ++loadIdRef.current;
    setFileActivePath(node.path);
    setFileError("");
    invoke<string>("read_file", { path: node.path })
      .then((c) => { if (id === loadIdRef.current) setFileContent(c); })
      .catch((e) => { if (id === loadIdRef.current) setFileError(`读取失败: ${String(e).slice(0, 80)}`); });
  }, []);

  // Build color → agent names mapping for the legend
  const colorMap = new Map<string, string[]>();
  p.agents.forEach((a) => {
    if (a.color) {
      const names = colorMap.get(a.color) || [];
      names.push(a.name);
      colorMap.set(a.color, names);
    }
  });

  return (
    <div className="flex flex-col h-full animate-[fadeIn_150ms_ease-out]">
      {/* Hero */}
      <div className="px-6 pt-8 pb-4 border-b border-[var(--app-border-light)]">
        <div className="flex items-start gap-3">
          <div
            className="w-10 h-10 flex items-center justify-center shrink-0 border"
            style={{
              background: "var(--app-input)",
              borderColor: "var(--app-border)",
            }}
          >
            <Puzzle size={18} style={{ color: p.enabled ? "var(--app-accent)" : "var(--app-text-muted)" }} />
          </div>
          <div className="min-w-0 flex-1">
            <div className="flex items-center justify-between">
              <div className="min-w-0">
                <h2 className="text-base font-mono font-semibold text-[var(--app-text)] truncate">
                  {p.name}
                </h2>
                <p className="text-xs text-[var(--app-text-muted)] font-mono mt-0.5">
                  @{p.marketplace}
                </p>
              </div>
              {/* View mode toggle */}
              <div className="flex items-center border border-[var(--app-border)] rounded overflow-hidden shrink-0 ml-3">
                <button
                  onClick={() => setViewMode("list")}
                  className={`px-2.5 py-1 text-2xs font-mono transition-colors ${
                    viewMode === "list"
                      ? "bg-[var(--app-accent)] text-[var(--app-bg)]"
                      : "text-[var(--app-text-muted)] hover:text-[var(--app-text)] hover:bg-[var(--app-hover)]"
                  }`}
                  title="列表视图"
                >
                  <List size={12} />
                </button>
                <button
                  onClick={() => setViewMode("file")}
                  className={`px-2.5 py-1 text-2xs font-mono transition-colors ${
                    viewMode === "file"
                      ? "bg-[var(--app-accent)] text-[var(--app-bg)]"
                      : "text-[var(--app-text-muted)] hover:text-[var(--app-text)] hover:bg-[var(--app-hover)]"
                  }`}
                  title="文件视图"
                >
                  <FolderTree size={12} />
                </button>
              </div>
            </div>
          </div>
        </div>
      </div>

      {/* Metadata card */}
      <div className="px-6 py-4 border-b border-[var(--app-border-light)]">
        <div className="space-y-1">
          <MetaRow label="CLI">
            <span style={{ color: CLI_CSS_COLORS[p.cli as CliKind] }}>{CLI_LABELS[p.cli as CliKind]}</span>
          </MetaRow>
          {p.version && <MetaRow label="版本">v{p.version}</MetaRow>}
          <MetaRow label="来源">{p.source}</MetaRow>
          <MetaRow label="状态">
            <span className="flex items-center gap-1.5">
              <Circle
                size={5}
                className={`shrink-0 ${p.enabled ? "fill-[var(--app-accent)] text-[var(--app-accent)]" : "fill-[var(--app-text-muted)] text-[var(--app-text-muted)]"}`}
                style={p.enabled ? { boxShadow: "0 0 4px var(--app-glow)" } : undefined}
              />
              {p.enabled ? "已启用" : "已禁用"}
            </span>
          </MetaRow>
        </div>

        {(p.skills.length > 0 || p.agents.length > 0) && (
          <p className="mt-2 text-2xs text-[var(--app-text-muted)] font-mono leading-relaxed">
            {p.enabled
              ? `启用后，此 Plugin 下的 ${p.skills.length} 个 Skill、${p.agents.length} 个智能体均可用。`
              : `禁用后，此 Plugin 下的 ${p.skills.length} 个 Skill、${p.agents.length} 个智能体都将不可用。`
            }
            <br />
            子项目不支持单独启用/禁用，跟随 Plugin 统一管理。
          </p>
        )}

        <div className="flex items-center gap-1 mt-3">
          {p.enabled ? (
            <Button variant="icon" size="sm" onClick={() => onToggle(false)} title="禁用" prompt="!">
              <Ban size={14} />
            </Button>
          ) : (
            <Button variant="icon" size="sm" onClick={() => onToggle(true)} title="启用" prompt=">">
              <Play size={14} />
            </Button>
          )}
          <Button variant="icon" size="sm"
            onClick={() => {
              setConfirm({
                title: "删除 Plugin",
                message: `确定要删除 "${p.name}" 吗？Plugin 下的所有 Skill 将不可用。`,
                confirmLabel: "删除",
                onConfirm: () => {
                  onUninstall(p.cli as CliKind, p.id);
                  setConfirm(null);
                },
              });
            }}
            title="删除"
            className="hover:text-[var(--app-red)] hover:border-[var(--app-red)] hover:bg-[var(--app-red-bg)]"
          >
            <Trash2 size={14} />
          </Button>
        </div>

        {updateInfo?.hasUpdate && (
          <div className="mt-3 p-3 border border-[var(--app-amber)] bg-[var(--app-amber-bg)]">
            <div className="flex items-center gap-1.5 mb-1.5">
              <ArrowUpCircle size={12} className="text-[var(--app-amber)]" />
              <span className="text-2xs text-[var(--app-amber)] font-mono">有可用更新</span>
            </div>
            <p className="text-2xs text-[var(--app-text-dim)] font-mono leading-relaxed">
              {updateInfo.currentVersion} → <span className="text-[var(--app-amber)]">{updateInfo.latestSha}</span>
            </p>
            <button
              onClick={() => onUpdate(p.cli as CliKind, p.id)}
              className="mt-2 p-1.5 border border-[var(--app-amber)] text-[var(--app-amber)]
                hover:bg-[var(--app-amber)] hover:text-[var(--app-bg)] transition-all duration-fast"
              title="更新到最新版本"
            >
              <ArrowUpCircle size={14} />
            </button>
          </div>
        )}
      </div>

      {viewMode === "list" && (
        <>
          {/* Tab bar */}
          <div className="flex border-b border-[var(--app-border-light)]" role="tablist">
        <button
          role="tab"
          aria-selected={tab === "skills"}
          onClick={() => setTab("skills")}
          className={`flex-1 flex items-center justify-center gap-1.5 py-2 text-xs font-mono transition-colors duration-fast
            ${tab === "skills"
              ? "text-[var(--app-accent)] border-b-[2px] border-b-[var(--app-accent)] -mb-px"
              : "text-[var(--app-text-muted)] hover:text-[var(--app-text)]"
            }`}
        >
          <Layers size={12} aria-hidden="true" />
          Skills
          <span className="text-2xs opacity-50 ml-0.5">{p.skills.length}</span>
        </button>
        <button
          role="tab"
          aria-selected={tab === "agents"}
          onClick={() => setTab("agents")}
          className={`flex-1 flex items-center justify-center gap-1.5 py-2 text-xs font-mono transition-colors duration-fast
            ${tab === "agents"
              ? "text-[var(--app-accent)] border-b-[2px] border-b-[var(--app-accent)] -mb-px"
              : "text-[var(--app-text-muted)] hover:text-[var(--app-text)]"
            }`}
        >
          <Bot size={12} aria-hidden="true" />
          Agents
          <span className="text-2xs opacity-50 ml-0.5">{p.agents.length}</span>
        </button>
        {(p.commands?.length > 0) && (
          <button
            role="tab"
            aria-selected={tab === "commands"}
            onClick={() => setTab("commands")}
            className={`flex-1 flex items-center justify-center gap-1.5 py-2 text-xs font-mono transition-colors duration-fast
              ${tab === "commands"
                ? "text-[var(--app-accent)] border-b-[2px] border-b-[var(--app-accent)] -mb-px"
                : "text-[var(--app-text-muted)] hover:text-[var(--app-text)]"
              }`}
          >
            <Terminal size={12} aria-hidden="true" />
            Commands
            <span className="text-2xs opacity-50 ml-0.5">{p.commands.length}</span>
          </button>
        )}
      </div>

      {/* Tab content */}
      <div className="flex-1 overflow-y-auto px-6 py-4">
        {tab === "skills" ? (
          p.skills.length > 0 ? (
            <div className="space-y-1.5">
              {p.skills.map((sk) => (
                <button
                  key={sk.name}
                  onClick={() => onSkillClick?.(sk, p.cli)}
                  className="group w-full text-left px-3 py-2 border border-[var(--app-border-light)]
                    bg-[var(--app-bg)] hover:bg-[var(--app-hover)] hover:border-[var(--app-accent)]/30
                    transition-colors duration-fast"
                >
                  <div className="flex items-center gap-2">
                    <FileText size={12} className="shrink-0 text-[var(--app-accent)] opacity-60" />
                    <span className="text-sm font-mono text-[var(--app-text)]">{sk.name}</span>
                  </div>
                  {sk.description && (
                    <p className="mt-1 ml-5 text-xs text-[var(--app-text-muted)] leading-relaxed line-clamp-2">
                      {sk.description}
                    </p>
                  )}
                  <div className="mt-1.5 ml-5 flex items-center gap-1 text-2xs text-[var(--app-text-muted)] font-mono opacity-0 group-hover:opacity-60 transition-opacity">
                    <FolderOpen size={9} />
                    <span className="truncate">{shortenPath(sk.path)}</span>
                  </div>
                </button>
              ))}
            </div>
          ) : (
            <div className="flex-1 flex items-center justify-center h-full">
              <div className="text-center">
                <Box size={24} className="mx-auto text-[var(--app-text-muted)] opacity-20 mb-2" />
                <p className="text-xs text-[var(--app-text-muted)] font-mono">此 Plugin 不包含 Skill</p>
              </div>
            </div>
          )
        ) : tab === "agents" ? (
          p.agents.length > 0 ? (
            <div className="space-y-3">
              {/* Color legend */}
              {colorMap.size > 0 && (
                <div className="flex flex-wrap items-center gap-x-3 gap-y-1 px-3 py-2 border border-[var(--app-border-light)] bg-[var(--app-bg)]">
                  <span className="text-2xs text-[var(--app-text-muted)] font-mono mr-0.5">
                    色点说明
                  </span>
                  {[...colorMap.entries()].map(([color, names]) => (
                    <span
                      key={color}
                      className="inline-flex items-center gap-1 text-2xs text-[var(--app-text-dim)] font-mono"
                      title={names.join(", ")}
                    >
                      <span
                        className="inline-block w-2 h-2 rounded-full border border-white/20 shrink-0"
                        style={{ backgroundColor: color }}
                      />
                      {names.join("/")}
                    </span>
                  ))}
                </div>
              )}
              <div className="space-y-1.5">
              {p.agents.map((agent) => (
                <button
                  key={agent.id}
                  onClick={() => onAgentClick?.(agent)}
                  className="group w-full text-left px-3 py-2 border border-[var(--app-border-light)]
                    bg-[var(--app-bg)] hover:bg-[var(--app-hover)] hover:border-[var(--app-accent)]/30
                    transition-colors duration-fast"
                >
                  <div className="flex items-center gap-2">
                    <Bot size={12} className="shrink-0 text-[var(--app-accent)] opacity-60" />
                    <span className="text-sm font-mono text-[var(--app-text)]">{agent.name}</span>
                    {agent.color && (
                      <span
                        className="inline-block w-2 h-2 rounded-full border border-white/20 shrink-0"
                        style={{ backgroundColor: agent.color }}
                      />
                    )}
                  </div>
                  {agent.description && (
                    <p className="mt-1 ml-5 text-xs text-[var(--app-text-muted)] leading-relaxed line-clamp-2">
                      {agent.description}
                    </p>
                  )}
                  <div className="mt-1.5 ml-5 flex items-center gap-2 flex-wrap">
                    {agent.model && (
                      <span className="text-2xs text-[var(--app-text-muted)] font-mono opacity-60">
                        model:{agent.model}
                      </span>
                    )}
                    {agent.tools.length > 0 && (
                      <span className="flex items-center gap-1 text-2xs text-[var(--app-text-muted)] font-mono opacity-60">
                        <Wrench size={9} />
                        {agent.tools.join(", ")}
                      </span>
                    )}
                  </div>
                </button>
              ))}
            </div>
            </div>
          ) : (
            <div className="flex-1 flex items-center justify-center h-full">
              <div className="text-center">
                <Box size={24} className="mx-auto text-[var(--app-text-muted)] opacity-20 mb-2" />
                <p className="text-xs text-[var(--app-text-muted)] font-mono">此 Plugin 不包含 Agent</p>
              </div>
            </div>
          )
        ) : (
          /* Commands tab */
          (p.commands?.length > 0) ? (
            <div className="space-y-1.5">
              {p.commands.map((cmd: CommandEntry) => (
                <button
                  key={cmd.id}
                  onClick={() => onCommandClick?.(cmd)}
                  className="group w-full text-left px-3 py-2 border border-[var(--app-border-light)]
                    bg-[var(--app-bg)] hover:bg-[var(--app-hover)] hover:border-[var(--app-accent)]/30
                    transition-colors duration-fast"
                >
                  <div className="flex items-center gap-2">
                    <Terminal size={12} className="shrink-0 text-[var(--app-accent)] opacity-60" />
                    <span className="text-sm font-mono text-[var(--app-text)]">/{cmd.name}</span>
                  </div>
                  {cmd.description && (
                    <p className="mt-1 ml-5 text-xs text-[var(--app-text-muted)] leading-relaxed line-clamp-2">
                      {cmd.description}
                    </p>
                  )}
                  <div className="mt-1.5 ml-5 flex items-center gap-1 text-2xs text-[var(--app-text-muted)] font-mono opacity-0 group-hover:opacity-60 transition-opacity">
                    <FolderOpen size={9} />
                    <span className="truncate">{cmd.path}</span>
                  </div>
                </button>
              ))}
            </div>
          ) : (
            <div className="flex-1 flex items-center justify-center h-full">
              <div className="text-center">
                <Box size={24} className="mx-auto text-[var(--app-text-muted)] opacity-20 mb-2" />
                <p className="text-xs text-[var(--app-text-muted)] font-mono">此 Plugin 不包含 Commands</p>
              </div>
            </div>
          )
        )}
      </div>
        </>
      )}

      {/* File mode */}
      {viewMode === "file" && pluginRootDir && (
        <div className="flex flex-row flex-1 min-h-0">
          {/* Left: FileTree — key includes enabled to force refresh on toggle */}
          <div className="w-56 shrink-0 border-r border-[var(--app-border-light)] overflow-y-auto bg-[var(--app-sidebar)]">
            <FileTree
              key={`${pluginRootDir}-${p.enabled}`}
              rootPath={pluginRootDir}
              onSelect={handleFileSelect}
              activePath={fileActivePath}
            />
          </div>
          {/* Right: file content */}
          <div className="flex-1 overflow-y-auto">
            {fileActivePath ? (
              <>
                <div className="px-6 pt-3 pb-1 flex items-center gap-1.5">
                  <File size={10} className="text-[var(--app-text-muted)] shrink-0" />
                  <span className="text-2xs text-[var(--app-text-muted)] font-mono truncate">
                    {basename(fileActivePath) || fileActivePath}
                  </span>
                </div>
                {fileError ? (
                  <div className="px-6 pb-6">
                    <div className="p-3 border border-[var(--app-red)] bg-[var(--app-red-bg)] text-xs text-[var(--app-red)] font-mono">
                      {fileError}
                    </div>
                  </div>
                ) : (
                  <div className="px-6 pb-6">
                    <FileContentBlock content={fileContent} filePath={fileActivePath} />
                  </div>
                )}
              </>
            ) : (
              <div className="flex items-center justify-center h-full">
                <div className="text-center">
                  <FolderOpen size={24} className="mx-auto text-[var(--app-text-muted)] opacity-20 mb-2" />
                  <p className="text-xs text-[var(--app-text-muted)] font-mono">从左侧文件树选择一个文件查看</p>
                </div>
              </div>
            )}
          </div>
        </div>
      )}

      {viewMode === "file" && !pluginRootDir && (
        <div className="flex-1 flex items-center justify-center">
          <div className="text-center">
            <Box size={24} className="mx-auto text-[var(--app-text-muted)] opacity-20 mb-2" />
            <p className="text-xs text-[var(--app-text-muted)] font-mono">此 Plugin 没有可浏览的文件目录</p>
          </div>
        </div>
      )}

      {confirm && (
        <ConfirmDialog
          open={true}
          title={confirm.title}
          message={confirm.message}
          confirmLabel={confirm.confirmLabel}
          onConfirm={confirm.onConfirm}
          onCancel={() => setConfirm(null)}
        />
      )}
    </div>
  );
}

/* ──────────────────── Plugin Skill Detail (read-only) ──────────────────── */

function PluginSkillDetail({
  skill,
  cli,
  onBack,
}: {
  skill: { name: string; path: string; description?: string };
  cli: string;
  onBack?: () => void;
}) {
  const [skillContent, setSkillContent] = useState<{ description: string; body: string } | null>(null);
  const [contentError, setContentError] = useState<string>("");
  const [activePath, setActivePath] = useState<string>(skill.path);
  const [fileContent, setFileContent] = useState<string>("");
  const loadRef = useRef(0);

  useEffect(() => {
    if (skill.path) {
      setContentError("");
      invoke<{ description: string; body: string }>("read_skill_content", { path: skill.path })
        .then(setSkillContent)
        .catch(() => setContentError("无法读取 Skill 内容"));
    }
  }, [skill.path]);

  const handleFileSelect = useCallback((node: FileTreeNode) => {
    if (node.is_dir) return;
    const id = ++loadRef.current;
    setActivePath(node.path);
    invoke<string>("read_file", { path: node.path })
      .then((c) => { if (id === loadRef.current) setFileContent(c); })
      .catch(() => { if (id === loadRef.current) setFileContent(""); });
  }, []);

  const displayDesc = skillContent?.description || skill.description || "";
  const coreFileName = skill.path.split("/").pop() || "";

  return (
    <div className="flex flex-col h-full animate-[fadeIn_150ms_ease-out]">
      {onBack && (
        <div className="px-6 pt-3">
          <button onClick={onBack} className="text-2xs text-[var(--app-text-muted)] hover:text-[var(--app-text)] font-mono transition-colors">
            ← 返回 Plugin
          </button>
        </div>
      )}
      {/* Hero */}
      <div className={`px-6 ${onBack ? "pt-4" : "pt-8"} pb-5 border-b border-[var(--app-border-light)]`}>
        <div className="flex items-start gap-3">
          <div
            className="w-10 h-10 flex items-center justify-center shrink-0 border"
            style={{ background: "var(--app-input)", borderColor: "var(--app-border)" }}
          >
            <FileText size={18} style={{ color: CLI_CSS_COLORS[cli as CliKind] || "var(--app-text-muted)" }} />
          </div>
          <div className="min-w-0">
            <h2 className="text-base font-mono font-semibold text-[var(--app-text)] truncate">{skill.name}</h2>
            <p className="text-xs text-[var(--app-text-muted)] font-mono mt-0.5">Plugin Skill（只读）</p>
          </div>
        </div>
      </div>

      {/* Metadata */}
      <div className="px-6 py-4 border-b border-[var(--app-border-light)]">
        <div className="space-y-1">
          <MetaRow label="CLI">
            <span style={{ color: CLI_CSS_COLORS[cli as CliKind] }}>{CLI_LABELS[cli as CliKind]}</span>
          </MetaRow>
          <MetaRow label="状态">
            <span className="flex items-center gap-1.5">
              <Circle size={5} className="shrink-0 fill-[var(--app-accent)] text-[var(--app-accent)]" />
              跟随 Plugin
            </span>
          </MetaRow>
        </div>
      </div>

      {/* FileTree + Content */}
      <div className="flex flex-row flex-1 min-h-0">
        {/* Left: FileTree — key includes path for refresh on data change */}
        <div className="w-56 shrink-0 border-r border-[var(--app-border-light)] overflow-y-auto bg-[var(--app-sidebar)]">
          <FileTree
            key={skill.path}
            rootPath={skill.path}
            onSelect={handleFileSelect}
            activePath={activePath}
            defaultOpenFile={coreFileName}
          />
        </div>
        {/* Right: Content */}
        <div className="flex-1 overflow-y-auto">
          {displayDesc && (
            <div className="px-6 py-4 border-b border-[var(--app-border-light)]">
              <SectionHeader icon={<FileText size={12} className="text-[var(--app-text-muted)]" />} label="描述" />
              <p className="text-xs text-[var(--app-text-dim)] font-mono leading-relaxed">{displayDesc}</p>
            </div>
          )}

          {/* Current file indicator */}
          <div className="px-6 pt-3 pb-1 flex items-center gap-1.5">
            <File size={10} className="text-[var(--app-text-muted)] shrink-0" />
            <span className="text-2xs text-[var(--app-text-muted)] font-mono truncate">
              {activePath === (skill as any).path ? coreFileName : (basename(activePath) || activePath)}
            </span>
          </div>

          {/* File content */}
          <div className="px-6 pb-6">
            <FileContentBlock
              content={activePath === (skill as any).path ? (skillContent?.body || "") : fileContent}
              filePath={activePath}
            />
          </div>
        </div>
      </div>
    </div>
  );
}

/* ──────────────────── Standalone Skill Detail ──────────────────── */

function StandaloneDetail({
  skill,
  readonly,
  graphData,
  onToggle,
  onUninstall,
  onNodeClick,
}: {
  skill: NonNullable<SkillDetailProps["item"]>["data"];
  readonly: boolean;
  graphData?: DependencyGraphData | null;
  onToggle: (enabled: boolean) => void;
  onUninstall: (cli: CliKind, skillId: string, path: string, name: string) => void;
  onNodeClick?: (nodeId: string) => void;
}) {
  const [confirm, setConfirm] = useState<ConfirmState | null>(null);
  const [skillContent, setSkillContent] = useState<{ description: string; body: string } | null>(null);
  const [contentError, setContentError] = useState<string | null>(null);
  const [activePath, setActivePath] = useState<string>("");
  const [fileContent, setFileContent] = useState<string>("");
  const loadRef2 = useRef(0);

  const hasLinkType = "linkType" in skill;

  useEffect(() => {
    if (hasLinkType && (skill as any).path) {
      setActivePath((skill as any).path);
      setContentError(null);
      invoke<{ description: string; body: string }>("read_skill_content", { path: (skill as any).path })
        .then((c) => {
          setSkillContent(c);
          if (!c.description && !c.body) setContentError("Skill 文件为空");
        })
        .catch((e) => setContentError(`无法读取 Skill 内容: ${String(e).slice(0, 80)}`));
    }
  }, [hasLinkType, (skill as any)?.path]);

  const handleFileSelect = useCallback((node: FileTreeNode) => {
    if (node.is_dir) return;
    const id = ++loadRef2.current;
    setActivePath(node.path);
    invoke<string>("read_file", { path: node.path })
      .then((c) => { if (id === loadRef2.current) setFileContent(c); })
      .catch(() => { if (id === loadRef2.current) setFileContent(""); });
  }, []);

  // Reverse references from graph
  const reverseRefs = useMemo(() => {
    if (!graphData || !hasLinkType) return [];
    const skillId = `${(skill as any).cli}:skill:${(skill as any).name}`;
    return graphData.edges
      .filter((e) => e.to === skillId)
      .map((e) => {
        const node = graphData.nodes.find((n) => n.id === e.from);
        return node ? { id: node.id, label: node.label, kind: node.kind, cli: node.cli } : null;
      })
      .filter(Boolean) as { id: string; label: string; kind: string; cli: string }[];
  }, [graphData, skill, hasLinkType]);

  if (!hasLinkType) return null;

  const coreFileName = (skill as any).path ? basename((skill as any).path) : "";

  const iconMap: Record<string, React.ReactNode> = {
    symlink: <Link2 size={18} />,
    directory: <FolderOpen size={18} />,
    file: <File size={18} />,
  };

  return (
    <div className="flex flex-col h-full animate-[fadeIn_150ms_ease-out]">
      {/* Hero */}
      <div className="px-6 pt-8 pb-5 border-b border-[var(--app-border-light)]">
        <div className="flex items-start gap-3">
          <div
            className="w-10 h-10 flex items-center justify-center shrink-0 border"
            style={{
              background: "var(--app-input)",
              borderColor: readonly ? "var(--app-border)" : "var(--app-border)",
            }}
          >
            {readonly
              ? <Lock size={18} className="text-[var(--app-text-muted)]" />
              : iconMap[skill.linkType] || <File size={18} className="text-[var(--app-text-dim)]" />
            }
          </div>
          <div className="min-w-0">
            <h2 className="text-base font-mono font-semibold text-[var(--app-text)] truncate">
              {skill.name}
            </h2>
            <p className="text-xs text-[var(--app-text-muted)] font-mono mt-0.5">
              {readonly ? "系统内置" : "独立 Skill"}
            </p>
          </div>
        </div>
      </div>

      {/* Metadata */}
      <div className="px-6 py-4 border-b border-[var(--app-border-light)]">
        <div className="space-y-1">
          <MetaRow label="CLI">
            <span style={{ color: CLI_CSS_COLORS[skill.cli as CliKind] }}>{CLI_LABELS[skill.cli as CliKind]}</span>
          </MetaRow>
          <MetaRow label="类型">{skill.linkType}</MetaRow>
          <MetaRow label="状态">
            <span className="flex items-center gap-1.5">
              <Circle
                size={5}
                className={`shrink-0 ${readonly || skill.enabled ? "fill-[var(--app-accent)] text-[var(--app-accent)]" : "fill-[var(--app-text-muted)] text-[var(--app-text-muted)]"}`}
                style={!readonly && skill.enabled ? { boxShadow: "0 0 4px var(--app-glow)" } : undefined}
              />
              {readonly ? "内置（只读）" : skill.enabled ? "已启用" : "已禁用"}
            </span>
          </MetaRow>
        </div>

        <div className="mt-3 flex items-center gap-1 text-2xs text-[var(--app-text-muted)] font-mono">
          <FolderOpen size={10} className="shrink-0" />
          <span className="truncate">{skill.path}</span>
        </div>

        {!readonly && (
          <div className="flex items-center gap-1">
            <button
              onClick={() => onToggle(!skill.enabled)}
              className={`mt-3 p-1.5 border transition-all duration-fast
                ${skill.enabled
                  ? "text-[var(--app-text-muted)] border-[var(--app-border)] hover:text-[var(--app-red)] hover:border-[var(--app-red)] hover:bg-[var(--app-red-bg)]"
                  : "text-[var(--app-text-muted)] border-[var(--app-border)] hover:text-[var(--app-accent)] hover:border-[var(--app-accent)] hover:bg-[var(--app-green-bg)]"
                }`}
              title={skill.enabled ? "禁用" : "启用"}
            >
              {skill.enabled ? <Ban size={14} /> : <Play size={14} />}
            </button>
            <button
              onClick={() => {
                setConfirm({
                  title: "删除 Skill",
                  message: `确定要删除 "${skill.name}" 吗？将从 skills 目录中移除。`,
                  confirmLabel: "删除",
                  onConfirm: () => {
                    onUninstall(skill.cli as CliKind, skill.id, (skill as any).path, skill.name as string);
                    setConfirm(null);
                  },
                });
              }}
              className="mt-3 p-1.5 border border-[var(--app-border)] text-[var(--app-text-muted)]
                hover:text-[var(--app-red)] hover:border-[var(--app-red)] hover:bg-[var(--app-red-bg)]
                transition-all duration-fast"
              title="删除"
            >
              <Trash2 size={14} />
            </button>
          </div>
        )}
        {readonly && (
          <div className="mt-3">
            <span className="text-2xs text-[var(--app-text-muted)] font-mono">系统内置 Skill，不可操作</span>
          </div>
        )}
      </div>

      {/* FileTree + Content */}
      <div className="flex flex-row flex-1 min-h-0">
        {/* Left: FileTree — key includes enabled to force refresh on toggle */}
        <div className="w-56 shrink-0 border-r border-[var(--app-border-light)] overflow-y-auto bg-[var(--app-sidebar)]">
          <FileTree
            key={`${(skill as any).path}-${(skill as any).enabled}`}
            rootPath={(skill as any).path}
            onSelect={handleFileSelect}
            activePath={activePath}
            defaultOpenFile={coreFileName}
          />
        </div>
        {/* Right: Content */}
        <div className="flex-1 overflow-y-auto">
          {/* Content load error */}
          {contentError && (
            <div className="px-6 py-4 border-b border-[var(--app-border-light)]">
              <div className="p-3 border border-[var(--app-amber)] bg-[var(--app-amber-bg)]">
                <p className="text-xs text-[var(--app-text-dim)] font-mono">{contentError}</p>
                <p className="text-2xs text-[var(--app-text-muted)] font-mono mt-1">
                  路径: {(skill as any).path}
                </p>
              </div>
            </div>
          )}

          {/* Description */}
          {skillContent?.description && (
            <div className="px-6 py-4 border-b border-[var(--app-border-light)]">
              <SectionHeader
                icon={<FileText size={12} className="text-[var(--app-text-muted)]" />}
                label="描述"
              />
              <p className="text-xs text-[var(--app-text-dim)] font-mono leading-relaxed">
                {skillContent.description}
              </p>
            </div>
          )}

          {/* Current file indicator */}
          <div className="px-6 pt-3 pb-1 flex items-center gap-1.5">
            <File size={10} className="text-[var(--app-text-muted)] shrink-0" />
            <span className="text-2xs text-[var(--app-text-muted)] font-mono truncate">
              {activePath === (skill as any).path ? coreFileName : (basename(activePath) || activePath)}
            </span>
          </div>

          {/* File content */}
          <div className="px-6 pb-6">
            <FileContentBlock
              content={activePath === (skill as any).path ? (skillContent?.body || "") : fileContent}
              filePath={activePath}
            />
          </div>

          {/* Reverse References */}
          {reverseRefs.length > 0 && (
            <div className="px-6 py-4 border-t border-[var(--app-border-light)]">
              <SectionHeader
                icon={<ArrowUpRight size={12} className="text-[var(--app-text-muted)]" />}
                label={`被 ${reverseRefs.length} 个节点引用`}
              />
              <div className="space-y-1">
                {reverseRefs.map((ref) => (
                  <button
                    key={ref.id}
                    onClick={() => onNodeClick?.(ref.id)}
                    className="flex items-center gap-2 w-full px-2 py-1 text-left hover:bg-[var(--app-hover)] transition-colors group"
                  >
                    <span className="text-xs font-mono truncate" style={{ color: CLI_CSS_COLORS[ref.cli as CliKind] || "#6B7280" }}>
                      {ref.label}
                    </span>
                    <span className="text-2xs text-[var(--app-text-muted)] font-mono opacity-0 group-hover:opacity-60 transition-opacity">
                      {ref.kind}
                    </span>
                  </button>
                ))}
              </div>
            </div>
          )}
        </div>
      </div>

      {confirm && (
        <ConfirmDialog
          open={true}
          title={confirm.title}
          message={confirm.message}
          confirmLabel={confirm.confirmLabel}
          onConfirm={confirm.onConfirm}
          onCancel={() => setConfirm(null)}
        />
      )}
    </div>
  );
}

/* ──────────────────── Command Detail (read-only) ──────────────────── */

function CommandDetail({
  command,
  onBack,
  onToggle,
  onUninstall,
}: {
  command: CommandEntry;
  onBack?: () => void;
  onToggle?: (enabled: boolean) => void;
  onUninstall?: () => void;
}) {
  const [content, setContent] = useState<string>("");
  const [confirm, setConfirm] = useState<{ title: string; message: string; confirmLabel: string; onConfirm: () => void } | null>(null);
  const [activePath, setActivePath] = useState<string>(command.path);
  const [fileContent, setFileContent] = useState<string>("");
  const isPluginCommand = !!onBack;
  const coreFileName = basename(command.path);
  const loadRef3 = useRef(0);

  useEffect(() => {
    if (command.path) {
      invoke<string>("read_agent_content", { path: command.path })
        .then(setContent)
        .catch(() => setContent(""));
    }
  }, [command.path]);

  const handleFileSelect = useCallback((node: FileTreeNode) => {
    if (node.is_dir) return;
    const id = ++loadRef3.current;
    setActivePath(node.path);
    invoke<string>("read_file", { path: node.path })
      .then((c) => { if (id === loadRef3.current) setFileContent(c); })
      .catch(() => { if (id === loadRef3.current) setFileContent(""); });
  }, []);

  return (
    <div className="flex flex-col h-full animate-[fadeIn_150ms_ease-out]">
      {onBack && (
        <div className="px-6 pt-3">
          <button onClick={onBack} className="text-2xs text-[var(--app-text-muted)] hover:text-[var(--app-text)] font-mono transition-colors">
            ← 返回 Plugin
          </button>
        </div>
      )}
      {/* Hero */}
      <div className={`px-6 ${onBack ? "pt-4" : "pt-8"} pb-5 border-b border-[var(--app-border-light)]`}>
        <div className="flex items-start gap-3">
          <div
            className="w-10 h-10 flex items-center justify-center shrink-0 border"
            style={{ background: "var(--app-input)", borderColor: "var(--app-border)" }}
          >
            <Terminal size={18} style={{ color: CLI_CSS_COLORS[command.cli as CliKind] || "var(--app-text-muted)" }} />
          </div>
          <div className="min-w-0">
            <h2 className="text-base font-mono font-semibold text-[var(--app-text)] truncate">/{command.name}</h2>
            <p className="text-xs text-[var(--app-text-muted)] font-mono mt-0.5">
              {isPluginCommand ? "Plugin Command（只读）" : "独立 Command"}
            </p>
          </div>
        </div>
      </div>

      {/* Metadata */}
      <div className="px-6 py-4 border-b border-[var(--app-border-light)]">
        <div className="space-y-1">
          <MetaRow label="CLI">
            <span style={{ color: CLI_CSS_COLORS[command.cli as CliKind] }}>{CLI_LABELS[command.cli as CliKind]}</span>
          </MetaRow>
          <MetaRow label="状态">
            <span className="flex items-center gap-1.5">
              <Circle
                size={5}
                className={`shrink-0 ${command.enabled ? "fill-[var(--app-accent)] text-[var(--app-accent)]" : "fill-[var(--app-text-muted)] text-[var(--app-text-muted)]"}`}
                style={command.enabled ? { boxShadow: "0 0 4px var(--app-glow)" } : undefined}
              />
              {isPluginCommand ? "跟随 Plugin" : command.enabled ? "已启用" : "已禁用"}
            </span>
          </MetaRow>
        </div>

        {/* Action buttons — only for standalone commands */}
        {!isPluginCommand && (
          <div className="flex items-center gap-1">
            <button
              onClick={() => onToggle?.(!command.enabled)}
              className={`mt-3 p-1.5 border transition-all duration-fast
                ${command.enabled
                  ? "text-[var(--app-text-muted)] border-[var(--app-border)] hover:text-[var(--app-red)] hover:border-[var(--app-red)] hover:bg-[var(--app-red-bg)]"
                  : "text-[var(--app-text-muted)] border-[var(--app-border)] hover:text-[var(--app-accent)] hover:border-[var(--app-accent)] hover:bg-[var(--app-green-bg)]"
                }`}
              title={command.enabled ? "禁用" : "启用"}
            >
              {command.enabled ? <Ban size={14} /> : <Play size={14} />}
            </button>
            <button
              onClick={() => {
                setConfirm({
                  title: "删除 Command",
                  message: `确定要删除 "/${command.name}" 吗？`,
                  confirmLabel: "删除",
                  onConfirm: () => {
                    onUninstall?.();
                    setConfirm(null);
                  },
                });
              }}
              className="mt-3 p-1.5 border border-[var(--app-border)] text-[var(--app-text-muted)]
                hover:text-[var(--app-red)] hover:border-[var(--app-red)] hover:bg-[var(--app-red-bg)]
                transition-all duration-fast"
              title="删除"
            >
              <Trash2 size={14} />
            </button>
          </div>
        )}
        {isPluginCommand && (
          <div className="mt-3">
            <span className="text-2xs text-[var(--app-text-muted)] font-mono">Plugin Command，不可单独操作</span>
          </div>
        )}
      </div>

      {/* FileTree + Content */}
      <div className="flex flex-row flex-1 min-h-0">
        {/* Left: FileTree — key includes enabled to force refresh on toggle */}
        <div className="w-56 shrink-0 border-r border-[var(--app-border-light)] overflow-y-auto bg-[var(--app-sidebar)]">
          <FileTree
            key={`${command.path}-${command.enabled}`}
            rootPath={command.path}
            onSelect={handleFileSelect}
            activePath={activePath}
            defaultOpenFile={coreFileName}
          />
        </div>
        {/* Right: Content */}
        <div className="flex-1 overflow-y-auto">
          {/* Description */}
          {command.description && (
            <div className="px-6 py-4 border-b border-[var(--app-border-light)]">
              <SectionHeader icon={<FileText size={12} className="text-[var(--app-text-muted)]" />} label="描述" />
              <p className="text-xs text-[var(--app-text-dim)] font-mono leading-relaxed">{command.description}</p>
            </div>
          )}

          {/* Current file indicator */}
          <div className="px-6 pt-3 pb-1 flex items-center gap-1.5">
            <File size={10} className="text-[var(--app-text-muted)] shrink-0" />
            <span className="text-2xs text-[var(--app-text-muted)] font-mono truncate">
              {activePath === command.path ? coreFileName : (basename(activePath) || activePath)}
            </span>
          </div>

          {/* File content */}
          <div className="px-6 pb-6">
            <FileContentBlock
              content={activePath === command.path ? content : fileContent}
              filePath={activePath}
            />
          </div>
        </div>
      </div>

      {confirm && (
        <ConfirmDialog
          open={true}
          title={confirm.title}
          message={confirm.message}
          confirmLabel={confirm.confirmLabel}
          onConfirm={confirm.onConfirm}
          onCancel={() => setConfirm(null)}
        />
      )}
    </div>
  );
}

/* ──────────────────── Empty State ──────────────────── */

function EmptyState() {
  return (
    <div className="flex-1 flex items-center justify-center bg-[var(--app-bg)]">
      <div className="flex flex-col items-center gap-5 text-center max-w-md px-4">
        <div className="w-16 h-16 rounded-full bg-[var(--app-selected)] flex items-center justify-center">
          <Cpu size={28} className="text-[var(--app-accent)]" />
        </div>
        <div>
          <div className="text-base font-mono font-semibold text-[var(--app-text)] mb-1">
            Skill &amp; Plugin Manager
          </div>
          <div className="text-xs text-[var(--app-text-dim)] leading-relaxed">
            统一管理 Plugin、Skill、Agent 和 Command
          </div>
        </div>

        {/* 使用说明 */}
        <div className="text-xs text-[var(--app-text-dim)] font-mono text-left space-y-1.5 bg-[var(--app-cmd-bg)] border border-[var(--app-border)] p-3 w-full">
          <div className="text-[var(--app-text-muted)] font-semibold mb-2">📋 使用说明</div>
          <div className="space-y-1">
            <div className="flex items-start gap-1.5">
              <span className="text-[var(--app-accent)] shrink-0 mt-0.5">▸</span>
              <span>左侧面板浏览 Plugin / Skill / Agent / Command，点击展开分组</span>
            </div>
            <div className="flex items-start gap-1.5">
              <span className="text-[var(--app-accent)] shrink-0 mt-0.5">▸</span>
              <span>使用 <span className="text-[var(--app-text-muted)]">全部 / 用户级 / 项目级</span> 标签切换作用域</span>
            </div>
            <div className="flex items-start gap-1.5">
              <span className="text-[var(--app-accent)] shrink-0 mt-0.5">▸</span>
              <span>选择项目后查看该项目下的资源</span>
            </div>
            <div className="flex items-start gap-1.5">
              <span className="text-[var(--app-accent)] shrink-0 mt-0.5">▸</span>
              <span>点击工具栏 <span className="text-[var(--app-text-muted)]">⋮</span> 展开批量操作：全选、启用/禁用、移动/复制、删除</span>
            </div>
            <div className="flex items-start gap-1.5">
              <span className="text-[var(--app-accent)] shrink-0 mt-0.5">▸</span>
              <span>右键资源单独操作：启用/禁用、移动/复制到不同作用域、删除</span>
            </div>
            <div className="flex items-start gap-1.5">
              <span className="text-[var(--app-accent)] shrink-0 mt-0.5">▸</span>
              <span>使用搜索框和过滤器快速定位资源</span>
            </div>
          </div>
        </div>

        {/* 注意事项 */}
        <div className="text-xs font-mono text-left bg-[var(--app-amber-bg)] border border-[var(--app-amber)]/30 p-3 w-full">
          <div className="flex items-start gap-1.5 text-[var(--app-amber)]">
            <span className="shrink-0 mt-0.5">⚠️</span>
            <div>
              <div className="font-semibold mb-1">注意事项</div>
              <div className="space-y-0.5 text-[var(--app-text-dim)]">
                <div>• Plugin 暂不支持复制和移动操作</div>
                <div>• 仅 Skills、Agents、Commands 可在用户级和项目级之间移动/复制</div>
                <div>• 系统内置 Agent 和 System Skill 为只读，不可修改</div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

/* ──────────────────── Main ──────────────────── */

export function SkillDetail({ item, graphData, onTogglePlugin, onToggleStandaloneSkill, updateInfos, onUpdatePlugin, onUninstallPlugin, onUninstallStandaloneSkill, onToggleAgent, onDeleteAgent, onToggleCommand, onUninstallCommand, onNodeClick, onSelect }: SkillDetailProps) {
  if (!item) return <EmptyState />;

  if (item.type === "plugin") {
    const updateInfo = updateInfos.find((u) => u.pluginId === (item.data as any).id);
    return (
      <PluginDetail
        plugin={item.data}
        onToggle={(enabled) =>
          onTogglePlugin((item.data as any).cli, (item.data as any).id, enabled)
        }
        updateInfo={updateInfo}
        onUpdate={onUpdatePlugin}
        onUninstall={onUninstallPlugin}
        onSkillClick={(skill, cli) => onSelect?.({ type: "plugin-skill", data: { skill, cli, parentPlugin: item.data as any } })}
        onAgentClick={(agent) => onSelect?.({ type: "plugin-agent", data: { ...agent, parentPlugin: item.data as any } })}
        onCommandClick={(cmd) => {
          console.log("[DEBUG] plugin command clicked:", cmd);
          onSelect?.({ type: "plugin-command", data: { ...cmd, parentPlugin: item.data as any } });
        }}
      />
    );
  }

  if (item.type === "standalone") {
    return (
      <StandaloneDetail
        skill={item.data}
        readonly={false}
        graphData={graphData}
        onToggle={(enabled) =>
          onToggleStandaloneSkill((item.data as any).cli, (item.data as any).id, enabled, (item.data as any).path)
        }
        onUninstall={onUninstallStandaloneSkill}
        onNodeClick={onNodeClick}
      />
    );
  }

  if (item.type === "agent") {
    return (
      <AgentDetail
        agent={item.data}
        graphData={graphData}
        onToggle={(agent, enabled) => onToggleAgent(agent.cli, agent.name, enabled, agent.path)}
        onDelete={(agent) => onDeleteAgent(agent.cli, agent.name, agent.path)}
        onNodeClick={onNodeClick}
      />
    );
  }

  if (item.type === "system") {
    return (
      <StandaloneDetail
        skill={item.data}
        readonly
        graphData={graphData}
        onToggle={() => {}}
        onUninstall={() => {}}
        onNodeClick={onNodeClick}
      />
    );
  }

  if (item.type === "plugin-skill") {
    const goBack = () => onSelect?.({ type: "plugin", data: item.data.parentPlugin });
    return (
      <PluginSkillDetail
        skill={item.data.skill}
        cli={item.data.cli}
        onBack={goBack}
      />
    );
  }

  if (item.type === "plugin-agent") {
    const goBack = () => onSelect?.({ type: "plugin", data: item.data.parentPlugin });
    return (
      <div className="flex flex-col h-full">
        <div className="px-6 pt-3">
          <button onClick={goBack} className="text-2xs text-[var(--app-text-muted)] hover:text-[var(--app-text)] font-mono transition-colors">
            ← 返回 Plugin
          </button>
        </div>
        <div className="flex-1 min-h-0 overflow-y-auto">
          <AgentDetail
            agent={item.data}
            graphData={graphData}
            onNodeClick={onNodeClick}
          />
        </div>
      </div>
    );
  }

  if (item.type === "command") {
    return (
      <CommandDetail
        command={item.data}
        onToggle={(enabled) => onToggleCommand?.(item.data.cli, item.data.name, enabled, item.data.path)}
        onUninstall={() => onUninstallCommand?.(item.data.cli, item.data.name, item.data.path)}
      />
    );
  }

  if (item.type === "plugin-command") {
    console.log("[DEBUG] rendering plugin-command detail:", item.data.name);
    const goBack = () => onSelect?.({ type: "plugin", data: item.data.parentPlugin });
    return <CommandDetail command={item.data} onBack={goBack} />;
  }

  return <EmptyState />;
}

/* ──────────────────── Utils ──────────────────── */

function shortenPath(path: string): string {
  const parts = path.split(/[/\\]/);
  if (parts.length <= 4) return path;
  return parts.slice(0, 2).join("/") + "/.../" + parts.slice(-2).join("/");
}
