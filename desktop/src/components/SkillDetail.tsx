import React, { useState } from "react";
import {
  Box, Puzzle, FileText, Lock, Circle, ExternalLink,
  Layers, FolderOpen, Link2, File, Cpu, ArrowUpCircle,
  Play, Ban, Trash2,
} from "lucide-react";
import { ConfirmDialog } from "./ConfirmDialog";
import { AgentDetail } from "./AgentDetail";
import type { SelectedItem, CliKind, PluginUpdateInfo } from "./SkillManager";

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
  onTogglePlugin: (cli: CliKind, pluginId: string, enabled: boolean) => void;
  onToggleStandaloneSkill: (cli: CliKind, skillId: string, enabled: boolean) => void;
  updateInfos: PluginUpdateInfo[];
  onUpdatePlugin: (cli: CliKind, pluginId: string) => void;
  onUninstallPlugin: (cli: CliKind, pluginId: string) => void;
  onUninstallStandaloneSkill: (cli: CliKind, skillId: string) => void;
}

/* ──────────────────── Helpers ──────────────────── */

const CLI_LABEL: Record<CliKind, string> = { claude: "Claude", codex: "Codex", qoder: "Qoder" };
const CLI_COLOR: Record<CliKind, string> = { claude: "var(--app-accent)", codex: "var(--app-blue)", qoder: "var(--app-purple)" };

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

/* ──────────────────── Plugin Detail ──────────────────── */

function PluginDetail({
  plugin,
  onToggle,
  updateInfo,
  onUpdate,
  onUninstall,
}: {
  plugin: NonNullable<SkillDetailProps["item"]>["data"];
  onToggle: (enabled: boolean) => void;
  updateInfo?: PluginUpdateInfo;
  onUpdate: (cli: CliKind, pluginId: string) => void;
  onUninstall: (cli: CliKind, pluginId: string) => void;
}) {
  const [confirm, setConfirm] = useState<ConfirmState | null>(null);
  if (!("marketplace" in plugin)) return null;

  return (
    <div className="flex flex-col h-full animate-[fadeIn_150ms_ease-out]">
      {/* Hero */}
      <div className="px-6 pt-8 pb-5 border-b border-[var(--app-border-light)]">
        <div className="flex items-start gap-3">
          <div
            className="w-10 h-10 flex items-center justify-center shrink-0 border"
            style={{
              background: "var(--app-input)",
              borderColor: "var(--app-border)",
            }}
          >
            <Puzzle size={18} style={{ color: plugin.enabled ? "var(--app-accent)" : "var(--app-text-muted)" }} />
          </div>
          <div className="min-w-0">
            <h2 className="text-base font-mono text-[var(--app-text)] truncate">
              {plugin.name}
            </h2>
            <p className="text-xs text-[var(--app-text-muted)] font-mono mt-0.5">
              @{plugin.marketplace}
            </p>
          </div>
        </div>
      </div>

      {/* Metadata card */}
      <div className="px-6 py-4 border-b border-[var(--app-border-light)]">
        <div className="space-y-1">
          <MetaRow label="CLI">
            <span style={{ color: CLI_COLOR[plugin.cli as CliKind] }}>{CLI_LABEL[plugin.cli as CliKind]}</span>
          </MetaRow>
          {plugin.version && <MetaRow label="版本">v{plugin.version}</MetaRow>}
          <MetaRow label="来源">{plugin.source}</MetaRow>
          <MetaRow label="状态">
            <span className="flex items-center gap-1.5">
              <Circle
                size={5}
                className={`shrink-0 ${plugin.enabled ? "fill-[var(--app-accent)] text-[var(--app-accent)]" : "fill-[var(--app-text-muted)] text-[var(--app-text-muted)]"}`}
                style={plugin.enabled ? { boxShadow: "0 0 4px var(--app-glow)" } : undefined}
              />
              {plugin.enabled ? "已启用" : "已禁用"}
            </span>
          </MetaRow>
        </div>

        {plugin.skills.length > 0 && (
          <p className="mt-2 text-2xs text-[var(--app-text-muted)] font-mono leading-relaxed">
            {plugin.enabled
              ? `启用后，此 Plugin 下的 ${plugin.skills.length} 个 Skill 均可用。`
              : `禁用后，此 Plugin 下的 ${plugin.skills.length} 个 Skill 都将不可用。`
            }
            <br />
            子 Skill 不支持单独启用/禁用，跟随 Plugin 统一管理。
          </p>
        )}

        <div className="flex items-center gap-1">
          <button
            onClick={() => onToggle(!plugin.enabled)}
            className={`mt-3 p-1.5 border transition-all duration-fast
              ${plugin.enabled
                ? "text-[var(--app-text-muted)] border-[var(--app-border)] hover:text-[var(--app-red)] hover:border-[var(--app-red)] hover:bg-[var(--app-red-bg)]"
                : "text-[var(--app-text-muted)] border-[var(--app-border)] hover:text-[var(--app-accent)] hover:border-[var(--app-accent)] hover:bg-[var(--app-green-bg)]"
              }`}
            title={plugin.enabled ? "禁用" : "启用"}
          >
            {plugin.enabled ? <Ban size={14} /> : <Play size={14} />}
          </button>
          <button
            onClick={() => {
              setConfirm({
                title: "卸载 Plugin",
                message: `确定要卸载 "${plugin.name}" 吗？Plugin 下的所有 Skill 将不可用。`,
                confirmLabel: "卸载",
                onConfirm: () => {
                  onUninstall(plugin.cli as CliKind, plugin.id);
                  setConfirm(null);
                },
              });
            }}
            className="mt-3 p-1.5 border border-[var(--app-border)] text-[var(--app-text-muted)]
              hover:text-[var(--app-red)] hover:border-[var(--app-red)] hover:bg-[var(--app-red-bg)]
              transition-all duration-fast"
            title="卸载"
          >
            <Trash2 size={14} />
          </button>
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
              onClick={() => onUpdate(plugin.cli as CliKind, plugin.id)}
              className="mt-2 p-1.5 border border-[var(--app-amber)] text-[var(--app-amber)]
                hover:bg-[var(--app-amber)] hover:text-[var(--app-bg)] transition-all duration-fast"
              title="更新到最新版本"
            >
              <ArrowUpCircle size={14} />
            </button>
          </div>
        )}
      </div>

      {/* Skills list */}
      {plugin.skills.length > 0 && (
        <div className="flex-1 overflow-y-auto px-6 py-4">
          <SectionHeader
            icon={<Layers size={12} className="text-[var(--app-text-muted)]" />}
            label={`包含 ${plugin.skills.length} 个 Skill`}
          />

          <div className="space-y-1.5">
            {plugin.skills.map((sk) => (
              <div
                key={sk.name}
                className="group px-3 py-2 border border-[var(--app-border-light)]
                  bg-[var(--app-bg)] hover:bg-[var(--app-hover)]
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
              </div>
            ))}
          </div>
        </div>
      )}

      {plugin.skills.length === 0 && (
        <div className="flex-1 flex items-center justify-center">
          <div className="text-center">
            <Box size={24} className="mx-auto text-[var(--app-text-muted)] opacity-20 mb-2" />
            <p className="text-xs text-[var(--app-text-muted)] font-mono">此 Plugin 不包含 Skill</p>
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

/* ──────────────────── Standalone Skill Detail ──────────────────── */

function StandaloneDetail({
  skill,
  readonly,
  onToggle,
  onUninstall,
}: {
  skill: NonNullable<SkillDetailProps["item"]>["data"];
  readonly: boolean;
  onToggle: (enabled: boolean) => void;
  onUninstall: (cli: CliKind, skillId: string) => void;
}) {
  const [confirm, setConfirm] = useState<ConfirmState | null>(null);
  if (!("linkType" in skill)) return null;

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
            <h2 className="text-base font-mono text-[var(--app-text)] truncate">
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
            <span style={{ color: CLI_COLOR[skill.cli as CliKind] }}>{CLI_LABEL[skill.cli as CliKind]}</span>
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
                  title: "卸载 Skill",
                  message: `确定要卸载 "${skill.name}" 吗？将从 skills 目录中移除。`,
                  confirmLabel: "卸载",
                  onConfirm: () => {
                    onUninstall(skill.cli as CliKind, skill.id);
                    setConfirm(null);
                  },
                });
              }}
              className="mt-3 p-1.5 border border-[var(--app-border)] text-[var(--app-text-muted)]
                hover:text-[var(--app-red)] hover:border-[var(--app-red)] hover:bg-[var(--app-red-bg)]
                transition-all duration-fast"
              title="卸载"
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

      {/* Empty space for future: file preview, usage stats */}
      <div className="flex-1" />

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
    <div className="flex flex-col items-center justify-center h-full gap-4 text-center px-8">
      <div
        className="w-16 h-16 flex items-center justify-center border border-dashed"
        style={{
          borderColor: "var(--app-border)",
          background: "var(--app-bg)",
        }}
      >
        <Cpu size={24} className="text-[var(--app-text-muted)] opacity-25" />
      </div>
      <div>
        <h3 className="text-sm font-mono text-[var(--app-text-dim)] mb-1">
          Skill & Plugin Manager
        </h3>
        <p className="text-xs text-[var(--app-text-muted)] leading-relaxed max-w-xs">
          从左侧列表选择一个 Plugin 或 Skill 查看详情。
          <br />
          你可以在这里启用 / 禁用插件和技能。
        </p>
      </div>
    </div>
  );
}

/* ──────────────────── Main ──────────────────── */

export function SkillDetail({ item, onTogglePlugin, onToggleStandaloneSkill, updateInfos, onUpdatePlugin, onUninstallPlugin, onUninstallStandaloneSkill }: SkillDetailProps) {
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
      />
    );
  }

  if (item.type === "standalone") {
    return (
      <StandaloneDetail
        skill={item.data}
        readonly={false}
        onToggle={(enabled) =>
          onToggleStandaloneSkill((item.data as any).cli, (item.data as any).id, enabled)
        }
        onUninstall={onUninstallStandaloneSkill}
      />
    );
  }

  if (item.type === "agent") {
    return (
      <AgentDetail
        agent={item.data}
        onToggle={() => {}}
        onDelete={() => {}}
      />
    );
  }

  if (item.type === "system") {
    return (
      <StandaloneDetail
        skill={item.data}
        readonly
        onToggle={() => {}}
        onUninstall={() => {}}
      />
    );
  }

  return <EmptyState />;
}

/* ──────────────────── Utils ──────────────────── */

function shortenPath(path: string): string {
  const parts = path.split(/[/\\]/);
  if (parts.length <= 4) return path;
  return parts.slice(0, 2).join("/") + "/.../" + parts.slice(-2).join("/");
}
