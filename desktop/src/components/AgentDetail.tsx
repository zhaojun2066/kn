import { useState, useEffect, useMemo, useRef, useCallback } from "react";
import { Bot, Lock, Circle, FolderOpen, Play, Ban, Trash2, Wrench, Sparkles, ArrowUpRight, File, Image } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "./common/Button";
import type { AgentEntry } from "./ResourceList";
import type { DependencyGraphData } from "./DependencyGraph";
import { ConfirmDialog } from "./ConfirmDialog";
import { FileTree } from "./FileTree";
import type { FileTreeNode } from "./FileTree";
import { CLI_HEX_COLORS } from "../lib/cli-constants";
import { basename } from "../lib/path-utils";
import { FileContentBlock, isImagePath, isPdfPath, langFromPath } from "./common/FileContentBlock";

interface AgentDetailProps {
  agent: AgentEntry;
  graphData?: DependencyGraphData | null;
  onToggle?: (agent: AgentEntry, enabled: boolean) => void;
  onDelete?: (agent: AgentEntry) => void;
  onNodeClick?: (nodeId: string) => void;
}

interface ConfirmState {
  title: string;
  message: string;
  confirmLabel: string;
  onConfirm: () => void;
}

/* ──────────────────── Helpers ──────────────────── */

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

/* FileContentBlock is now imported from common/FileContentBlock */

/* ── Agent Content section ── */

function AgentContent({ content }: { content: string }) {
  if (!content) return null;

  return (
    <div className="px-6 py-4 border-b border-[var(--app-border-light)]">
      <SectionHeader
        icon={<Sparkles size={12} className="text-[var(--app-text-muted)]" />}
        label="系统提示词"
      />
      <pre className="p-3 text-2xs text-[var(--app-text-dim)] font-mono leading-relaxed whitespace-pre-wrap bg-[var(--app-bg)] border border-[var(--app-border-light)] max-h-96 overflow-y-auto">
        {content}
      </pre>
    </div>
  );
}

/* ── Reverse References section ── */

function ReverseRefs({ agentId, graphData, onNodeClick }: { agentId: string; graphData?: DependencyGraphData | null; onNodeClick?: (id: string) => void }) {
  const refs = useMemo(() => {
    if (!graphData) return [];
    return graphData.edges
      .filter((e) => e.to === agentId)
      .map((e) => {
        const node = graphData.nodes.find((n) => n.id === e.from);
        return node ? { id: node.id, label: node.label, kind: node.kind, cli: node.cli } : null;
      })
      .filter(Boolean) as { id: string; label: string; kind: string; cli: string }[];
  }, [graphData, agentId]);

  if (refs.length === 0) return null;

  const CLI_COLORS: Record<string, string> = CLI_HEX_COLORS;

  return (
    <div className="px-6 py-4 border-b border-[var(--app-border-light)]">
      <SectionHeader
        icon={<ArrowUpRight size={12} className="text-[var(--app-text-muted)]" />}
        label={`被 ${refs.length} 个节点引用`}
      />
      <div className="space-y-1">
        {refs.map((ref) => (
          <button
            key={ref.id}
            onClick={() => onNodeClick?.(ref.id)}
            className="flex items-center gap-2 w-full px-2 py-1 text-left hover:bg-[var(--app-hover)] transition-colors group"
          >
            <span className="text-xs font-mono truncate" style={{ color: CLI_COLORS[ref.cli] || "#6B7280" }}>
              {ref.label}
            </span>
            <span className="text-2xs text-[var(--app-text-muted)] font-mono opacity-0 group-hover:opacity-60 transition-opacity">
              {ref.kind}
            </span>
          </button>
        ))}
      </div>
    </div>
  );
}

/* ──────────────────── Main ──────────────────── */

export function AgentDetail({ agent, graphData, onToggle, onDelete, onNodeClick }: AgentDetailProps) {
  const isBuiltin = agent.source === "builtin";
  const [confirm, setConfirm] = useState<ConfirmState | null>(null);
  const [impactNodes, setImpactNodes] = useState<string[]>([]);
  const [showImpactConfirm, setShowImpactConfirm] = useState(false);
  const [pendingAction, setPendingAction] = useState<"toggle" | "delete" | null>(null);
  const [agentContent, setAgentContent] = useState<string>("");
  const [activePath, setActivePath] = useState<string>(agent.path);
  const [fileContent, setFileContent] = useState<string>("");
  const coreFileName = basename(agent.path);
  const loadRef = useRef(0);

  useEffect(() => {
    if (agent.path) {
      setActivePath(agent.path);
      loadRef.current++;
      invoke<string>("read_agent_content", { path: agent.path })
        .then(setAgentContent)
        .catch(() => setAgentContent(""));
    } else {
      setAgentContent("");
    }
  }, [agent.path]);

  const handleFileSelect = useCallback((node: FileTreeNode) => {
    if (node.is_dir) return;
    const id = ++loadRef.current;
    setActivePath(node.path);
    invoke<string>("read_file", { path: node.path })
      .then((c) => { if (id === loadRef.current) setFileContent(c); })
      .catch(() => { if (id === loadRef.current) setFileContent(""); });
  }, []);

  async function handleToggle() {
    if (graphData) {
      try {
        const impacted = await invoke<string[]>("analyze_impact", {
          targetId: agent.id,
          graph: graphData,
        });
        if (impacted.length > 0) {
          setImpactNodes(impacted);
          setPendingAction("toggle");
          setShowImpactConfirm(true);
          return;
        }
      } catch (e) {
        console.error("Impact analysis failed:", e);
      }
    }
    onToggle?.(agent, !agent.enabled);
  }

  async function handleDelete() {
    if (graphData) {
      try {
        const impacted = await invoke<string[]>("analyze_impact", {
          targetId: agent.id,
          graph: graphData,
        });
        if (impacted.length > 0) {
          setImpactNodes(impacted);
          setPendingAction("delete");
          setShowImpactConfirm(true);
          return;
        }
      } catch (e) {
        console.error("Impact analysis failed:", e);
      }
    }
    setConfirm({
      title: "删除 Agent",
      message: `确定要删除 "${agent.name}" 吗？将从 agents 目录中移除。`,
      confirmLabel: "删除",
      onConfirm: () => {
        onDelete?.(agent);
        setConfirm(null);
      },
    });
  }

  function confirmImpactAction() {
    if (pendingAction === "toggle") {
      onToggle?.(agent, !agent.enabled);
    } else if (pendingAction === "delete") {
      setConfirm({
        title: "删除 Agent",
        message: `确定要删除 "${agent.name}" 吗？将从 agents 目录中移除。\n\n影响节点:\n${impactNodes.map((n) => "• " + n).join("\n")}`,
        confirmLabel: "删除",
        onConfirm: () => {
          onDelete?.(agent);
          setConfirm(null);
        },
      });
    }
    setShowImpactConfirm(false);
    setPendingAction(null);
  }

  return (
    <div className="flex flex-col h-full animate-[fadeIn_150ms_ease-out]">
      {/* Hero */}
      <div className="px-6 pt-8 pb-5 border-b border-[var(--app-border-light)]">
        <div className="flex items-start gap-3">
          <div
            className="w-10 h-10 flex items-center justify-center shrink-0 border"
            style={{
              background: "var(--app-input)",
              borderColor: isBuiltin ? "var(--app-border)" : "var(--app-border)",
            }}
          >
            {isBuiltin
              ? <Lock size={18} className="text-[var(--app-text-muted)]" />
              : <Bot size={18} style={{ color: agent.enabled ? "var(--app-accent)" : "var(--app-text-muted)" }} />
            }
          </div>
          <div className="min-w-0">
            <h2 className="text-base font-mono text-[var(--app-text)] truncate">
              {agent.name}
            </h2>
            <p className="text-xs text-[var(--app-text-muted)] font-mono mt-0.5">
              {isBuiltin ? "系统内置" : agent.source === "user" ? "用户级 Agent" : agent.source === "project" ? "项目级 Agent" : "Agent"}
            </p>
            {agent.description && (
              <p className="text-xs text-[var(--app-text-dim)] font-mono mt-2 leading-relaxed">
                {agent.description}
              </p>
            )}
          </div>
        </div>
      </div>

      {/* Metadata */}
      <div className="px-6 py-4 border-b border-[var(--app-border-light)]">
        <div className="space-y-1">
          <MetaRow label="CLI">
            <span style={{ color: agent.cli === "claude" ? "var(--app-accent)" : agent.cli === "codex" ? "var(--app-blue)" : "var(--app-purple)" }}>
              {agent.cli === "claude" ? "Claude" : agent.cli === "codex" ? "Codex" : "Qoder"}
            </span>
          </MetaRow>
          <MetaRow label="来源">{agent.source}</MetaRow>
          <MetaRow label="状态">
            <span className="flex items-center gap-1.5">
              <Circle
                size={5}
                className={`shrink-0 ${agent.enabled ? "fill-[var(--app-accent)] text-[var(--app-accent)]" : "fill-[var(--app-text-muted)] text-[var(--app-text-muted)]"}`}
                style={agent.enabled ? { boxShadow: "0 0 4px var(--app-glow)" } : undefined}
              />
              {isBuiltin ? "内置（只读）" : agent.enabled ? "已启用" : "已禁用"}
            </span>
          </MetaRow>
          {agent.model && <MetaRow label="模型">{agent.model}</MetaRow>}
          {agent.color && (
            <MetaRow label="颜色">
              <span className="flex items-center gap-1.5">
                <span
                  className="inline-block w-3 h-3 rounded-full border border-white/20"
                  style={{ backgroundColor: agent.color }}
                />
                {agent.color}
              </span>
            </MetaRow>
          )}
        </div>

        {/* Path (only for file-based agents) */}
        {agent.path && (
          <div className="mt-3 flex items-center gap-1 text-2xs text-[var(--app-text-muted)] font-mono">
            <FolderOpen size={10} className="shrink-0" />
            <span className="truncate">{agent.path}</span>
          </div>
        )}

        {/* Action buttons (hidden for builtin or read-only agents) */}
        {!isBuiltin && (onToggle || onDelete) && (
          <div className="flex items-center gap-1 mt-3">
            <Button
              variant="icon"
              size="sm"
              onClick={handleToggle}
              className={`p-1.5 border-[var(--app-border)] ${
                agent.enabled
                  ? "hover:text-[var(--app-red)] hover:border-[var(--app-red)] hover:bg-[var(--app-red-bg)]"
                  : "hover:text-[var(--app-accent)] hover:border-[var(--app-accent)] hover:bg-[var(--app-green-bg)]"
              }`}
              title={agent.enabled ? "禁用" : "启用"}
            >
              {agent.enabled ? <Ban size={14} /> : <Play size={14} />}
            </Button>
            <Button
              variant="icon"
              size="sm"
              onClick={handleDelete}
              className="p-1.5 border-[var(--app-border)] hover:text-[var(--app-red)] hover:border-[var(--app-red)] hover:bg-[var(--app-red-bg)]"
              title="删除"
            >
              <Trash2 size={14} />
            </Button>
          </div>
        )}

        {/* Read-only notice */}
        {!isBuiltin && !onToggle && !onDelete && (
          <div className="mt-3">
            <span className="text-2xs text-[var(--app-text-muted)] font-mono">Plugin Agent，跟随 Plugin 统一管理</span>
          </div>
        )}
        {isBuiltin && (
          <div className="mt-3">
            <span className="text-2xs text-[var(--app-text-muted)] font-mono">系统内置 Agent，不可操作</span>
          </div>
        )}
      </div>

      {/* FileTree + Content */}
      <div className="flex flex-row flex-1 min-h-0">
        {/* Left: FileTree */}
        {agent.path && (
          <div className="w-56 shrink-0 border-r border-[var(--app-border-light)] overflow-y-auto bg-[var(--app-sidebar)]">
            <FileTree
              rootPath={agent.path}
              onSelect={handleFileSelect}
              activePath={activePath}
              defaultOpenFile={coreFileName}
            />
          </div>
        )}
        {/* Right: Content */}
        <div className="flex-1 overflow-y-auto">
          {/* Tools section */}
          {agent.tools.length > 0 && (
            <div className="px-6 py-4 border-b border-[var(--app-border-light)]">
              <SectionHeader
                icon={<Wrench size={12} className="text-[var(--app-text-muted)]" />}
                label={`${agent.tools.length} 个工具`}
              />
              <div className="flex flex-wrap gap-1">
                {agent.tools.map((tool) => (
                  <span
                    key={tool}
                    className="text-2xs px-1.5 py-0.5 border border-[var(--app-border-light)]
                      bg-[var(--app-bg)] text-[var(--app-accent)] font-mono"
                  >
                    {tool}
                  </span>
                ))}
              </div>
            </div>
          )}

          {/* Referenced Skills */}
          {agent.skills.length > 0 && (
            <div className="px-6 py-4 border-b border-[var(--app-border-light)]">
              <SectionHeader
                icon={<Wrench size={12} className="text-[var(--app-text-muted)]" />}
                label={`引用 ${agent.skills.length} 个 Skill`}
              />
              <div className="flex flex-wrap gap-1">
                {agent.skills.map((skill) => (
                  <span
                    key={skill}
                    className="text-2xs px-1.5 py-0.5 border border-[var(--app-border-light)]
                      bg-[var(--app-purple)]/10 text-[var(--app-purple)] font-mono"
                  >
                    {skill}
                  </span>
                ))}
              </div>
            </div>
          )}

          {/* Codex sandbox */}
          {agent.sandboxMode && (
            <div className="px-6 py-4 border-b border-[var(--app-border-light)]">
              <MetaRow label="沙箱">{agent.sandboxMode}</MetaRow>
            </div>
          )}

          {/* Current file indicator */}
          {agent.path && (
            <div className="px-6 pt-3 pb-1 flex items-center gap-1.5">
              <File size={10} className="text-[var(--app-text-muted)] shrink-0" />
              <span className="text-2xs text-[var(--app-text-muted)] font-mono truncate">
                {activePath === agent.path ? coreFileName : (basename(activePath) || activePath)}
              </span>
            </div>
          )}

          {/* File content */}
          {agent.path && (
            <div className="px-6 pb-6">
              <FileContentBlock
                content={activePath === agent.path ? agentContent : fileContent}
                filePath={activePath}
              />
            </div>
          )}

          {/* Reverse References */}
          <ReverseRefs agentId={agent.id} graphData={graphData} onNodeClick={onNodeClick} />
        </div>
      </div>

      {/* Delete confirmation */}
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

      {/* Impact confirmation */}
      {showImpactConfirm && (
        <ConfirmDialog
          open={true}
          title={pendingAction === "toggle" ? "确认禁用" : "确认删除"}
          message={`${pendingAction === "toggle" ? "禁用" : "删除"} "${agent.name}" 将影响以下节点:\n\n${impactNodes.map((n) => "• " + n).join("\n")}`}
          confirmLabel={pendingAction === "toggle" ? "仍然禁用" : "仍然删除"}
          onConfirm={confirmImpactAction}
          onCancel={() => {
            setShowImpactConfirm(false);
            setPendingAction(null);
          }}
        />
      )}
    </div>
  );
}
