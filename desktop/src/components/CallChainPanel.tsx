import { useMemo, useState } from "react";
import {
  Bot, Puzzle, FileText, Wrench, Server, Lock,
  ChevronRight, ArrowUp, ArrowDown,
  Circle,
} from "lucide-react";
import type { DependencyGraphData } from "./DependencyGraph";

// ── Types ──

export interface CallChainNode {
  id: string;
  label: string;
  kind: string;
  cli: string;
  source: string;
  locked: boolean;
  depth: number;
  edgeKind: string;
  edgeLabel: string;
}

export interface CallChain {
  target: CallChainNode;
  ancestors: CallChainNode[];
  descendants: CallChainNode[];
}

// ── BFS builder ──

export function buildCallChain(
  targetId: string,
  graph: DependencyGraphData,
): CallChain | null {
  const targetNode = graph.nodes.find((n) => n.id === targetId);
  if (!targetNode) return null;

  const target: CallChainNode = {
    id: targetNode.id,
    label: targetNode.label,
    kind: targetNode.kind,
    cli: targetNode.cli,
    source: targetNode.source,
    locked: targetNode.locked,
    depth: 0,
    edgeKind: "",
    edgeLabel: "",
  };

  const MAX_DEPTH = 5;

  // ── Ancestors (reverse BFS: who points TO me?) ──
  const ancestors: CallChainNode[] = [];
  const visited = new Set<string>([targetId]);
  let queue = [targetId];
  let depth = 0;
  while (queue.length > 0 && depth < MAX_DEPTH) {
    depth++;
    const nextQueue: string[] = [];
    for (const current of queue) {
      for (const edge of graph.edges) {
        if (edge.to === current && !visited.has(edge.from)) {
          visited.add(edge.from);
          const node = graph.nodes.find((n) => n.id === edge.from);
          if (node) {
            ancestors.push({
              id: node.id,
              label: node.label,
              kind: node.kind,
              cli: node.cli,
              source: node.source,
              locked: node.locked,
              depth,
              edgeKind: edge.kind,
              edgeLabel: edge.label,
            });
            nextQueue.push(edge.from);
          }
        }
      }
    }
    queue = nextQueue;
  }

  // ── Descendants (forward BFS: who do I point TO?) ──
  const descendants: CallChainNode[] = [];
  visited.clear();
  visited.add(targetId);
  queue = [targetId];
  depth = 0;
  while (queue.length > 0 && depth < MAX_DEPTH) {
    depth++;
    const nextQueue: string[] = [];
    for (const current of queue) {
      for (const edge of graph.edges) {
        if (edge.from === current && !visited.has(edge.to)) {
          visited.add(edge.to);
          const node = graph.nodes.find((n) => n.id === edge.to);
          if (node) {
            descendants.push({
              id: node.id,
              label: node.label,
              kind: node.kind,
              cli: node.cli,
              source: node.source,
              locked: node.locked,
              depth,
              edgeKind: edge.kind,
              edgeLabel: edge.label,
            });
            nextQueue.push(edge.to);
          }
        }
      }
    }
    queue = nextQueue;
  }

  return { target, ancestors, descendants };
}

// ── Visual constants ──

import { CLI_HEX_COLORS, CLI_LABELS } from "../lib/cli-constants";
const CLI_COLORS: Record<string, string> = CLI_HEX_COLORS;

/* eslint-disable @typescript-eslint/no-explicit-any */
const KIND_ICONS: Record<string, React.ComponentType<any>> = {
  plugin: Puzzle,
  agent: Bot,
  skill: FileText,
  tool: Wrench,
  mcp: Server,
};

const EDGE_LABELS: Record<string, string> = {
  contains: "包含",
  references: "引用",
  spawns: "派生",
  needsTool: "依赖工具",
  needsModel: "依赖模型",
};

// ── Props ──

interface CallChainPanelProps {
  targetId: string;
  graphData: DependencyGraphData;
  onNodeClick?: (nodeId: string) => void;
}

// ── Sub-components ──

function ChainNodeRow({
  node,
  direction,
  indent,
  onClick,
}: {
  node: CallChainNode;
  direction: "up" | "down";
  indent: number;
  onClick?: (nodeId: string) => void;
}) {
  const KindIcon = KIND_ICONS[node.kind] || Circle;
  const cliColor = CLI_COLORS[node.cli] || "#6B7280";
  const cliLabel = CLI_LABELS[node.cli as keyof typeof CLI_LABELS] || node.cli;
  const edgeLabel = EDGE_LABELS[node.edgeKind] || node.edgeKind;
  const arrow = direction === "up" ? "←" : "→";

  return (
    <button
      className="flex items-center gap-1.5 py-1 min-w-0 w-full text-left hover:bg-[var(--app-hover)] transition-colors"
      style={{ paddingLeft: `${12 + indent * 16}px` }}
      onClick={() => onClick?.(node.id)}
    >
      {/* Edge kind badge — only if not root */}
      {node.edgeKind && (
        <span className="text-2xs text-[var(--app-text-muted)] font-mono shrink-0 opacity-60">
          {arrow}{edgeLabel}
        </span>
      )}

      {/* Node icon */}
      <KindIcon
        size={12}
        className="shrink-0"
        style={{ color: node.locked ? "var(--app-text-muted)" : cliColor, opacity: node.locked ? 0.5 : 0.8 }}
      />

      {/* Label */}
      <span
        className={`text-xs font-mono truncate ${
          node.locked ? "text-[var(--app-text-muted)]" : "text-[var(--app-text)]"
        }`}
      >
        {node.label}
      </span>

      {/* Locked badge */}
      {node.locked && (
        <Lock size={9} className="shrink-0 text-[var(--app-text-muted)]" />
      )}

      {/* CLI badge */}
      <span
        className="text-2xs font-mono px-1 py-px leading-none border shrink-0"
        style={{ color: cliColor, borderColor: cliColor, opacity: 0.6 }}
      >
        {cliLabel}
      </span>
    </button>
  );
}

function TargetNodeRow({ node }: { node: CallChainNode }) {
  const KindIcon = KIND_ICONS[node.kind] || Circle;
  return (
    <div className="flex items-center gap-1.5 py-1.5 px-2 my-0.5
      bg-[var(--app-accent)]/10 border-l-[3px] border-l-[var(--app-accent)]">
      <KindIcon
        size={13}
        className="shrink-0"
        style={{ color: "var(--app-accent)" }}
      />
      <span className="text-xs font-mono text-[var(--app-text)] font-semibold truncate">
        {node.label}
      </span>
      {node.locked && (
        <Lock size={9} className="shrink-0 text-[var(--app-text-muted)]" />
      )}
      {node.source && (
        <span className="text-2xs text-[var(--app-text-muted)] font-mono shrink-0">
          {node.source}
        </span>
      )}
      <span className="text-2xs text-[var(--app-accent)] font-mono shrink-0 ml-auto">
        当前
      </span>
    </div>
  );
}

function ChainGroup({
  label,
  count,
  nodes,
  direction,
  icon: Icon,
  onNodeClick,
}: {
  label: string;
  count: number;
  nodes: CallChainNode[];
  direction: "up" | "down";
  icon: React.ComponentType<any>;
  onNodeClick?: (nodeId: string) => void;
}) {
  const [collapsed, setCollapsed] = useState(false);

  if (count === 0) {
    return (
      <div className="py-2 px-3">
        <span className="text-2xs text-[var(--app-text-muted)] font-mono italic">
          {direction === "up" ? "无上游依赖" : "无下游依赖"}
        </span>
      </div>
    );
  }

  // Sort by depth, then label
  const sorted = [...nodes].sort((a, b) => a.depth - b.depth || a.label.localeCompare(b.label));

  return (
    <div>
      <button
        onClick={() => setCollapsed(!collapsed)}
        className="flex items-center gap-1.5 w-full px-3 py-1.5 text-left
          hover:bg-[var(--app-hover)] transition-colors duration-fast"
      >
        <ChevronRight
          size={9}
          className={`shrink-0 text-[var(--app-text-muted)] transition-transform duration-200
            ${collapsed ? "" : "rotate-90"}`}
        />
        <Icon size={11} className="shrink-0 text-[var(--app-text-muted)]" />
        <span className="text-2xs text-[var(--app-text-muted)] font-mono uppercase tracking-[0.15em] flex-1">
          {label}
        </span>
        <span className="text-2xs text-[var(--app-text-muted)] font-mono tabular-nums">
          {count}
        </span>
      </button>

      {!collapsed && (
        <div className="pb-1">
          {sorted.map((node, i) => (
            <ChainNodeRow
              key={`${node.id}-${i}`}
              node={node}
              direction={direction}
              indent={node.depth}
              onClick={onNodeClick}
            />
          ))}
        </div>
      )}
    </div>
  );
}

// ── Empty state ──

function EmptyCallChain({ target }: { target: CallChainNode }) {
  return (
    <div className="px-6 py-4">
      <div className="flex items-center gap-2 mb-3">
        <Server size={12} className="text-[var(--app-text-muted)]" />
        <span className="text-2xs text-[var(--app-text-muted)] font-mono uppercase tracking-[0.2em]">
          调用链
        </span>
        <div className="flex-1 border-b border-[var(--app-border-light)]" />
      </div>

      <TargetNodeRow node={target} />

      <div className="py-3 flex flex-col items-center gap-2">
        <Server size={18} className="text-[var(--app-text-muted)] opacity-20" />
        <span className="text-2xs text-[var(--app-text-muted)] font-mono">
          未找到 "{target.label}" 的依赖关系
        </span>
        <span className="text-2xs text-[var(--app-text-muted)] font-mono opacity-60">
          该智能体没有与其他技能、智能体、工具或模型的关联。
        </span>
      </div>
    </div>
  );
}

// ── Main ──

export function CallChainPanel({ targetId, graphData, onNodeClick }: CallChainPanelProps) {
  const chain = useMemo(
    () => buildCallChain(targetId, graphData),
    [targetId, graphData],
  );

  if (!chain) return null;

  const { target, ancestors, descendants } = chain;
  const hasAny = ancestors.length > 0 || descendants.length > 0;

  if (!hasAny) {
    return <EmptyCallChain target={target} />;
  }

  return (
    <div className="border-b border-[var(--app-border-light)]">
      {/* Section header */}
      <div className="flex items-center gap-2 px-6 pt-4 pb-2">
        <Server size={12} className="text-[var(--app-text-muted)]" />
        <span className="text-2xs text-[var(--app-text-muted)] font-mono uppercase tracking-[0.2em]">
          调用链
        </span>
        <div className="flex-1 border-b border-[var(--app-border-light)]" />
        <span className="text-2xs text-[var(--app-text-muted)] font-mono tabular-nums">
          {ancestors.length + descendants.length} 条边
        </span>
      </div>

      {/* Upstream */}
      <ChainGroup
        label="上游"
        count={ancestors.length}
        nodes={ancestors}
        direction="up"
        icon={ArrowUp}
        onNodeClick={onNodeClick}
      />

      {/* Target */}
      <div className="px-2 py-0.5">
        <TargetNodeRow node={target} />
      </div>

      {/* Downstream */}
      <ChainGroup
        label="下游"
        count={descendants.length}
        nodes={descendants}
        direction="down"
        icon={ArrowDown}
        onNodeClick={onNodeClick}
      />

      {/* Legend */}
      <div className="px-6 py-2 flex items-center gap-3 text-2xs text-[var(--app-text-muted)] font-mono border-t border-[var(--app-border-light)]">
        <span className="flex items-center gap-1">
          <span className="text-[var(--app-text-muted)]">←</span> 上游
        </span>
        <span className="flex items-center gap-1">
          <span className="text-[var(--app-text-muted)]">→</span> 下游
        </span>
        <span className="text-[var(--app-border)]">|</span>
        {Object.entries(EDGE_LABELS).map(([kind, label]) => (
          <span key={kind} className="opacity-50">{label}</span>
        ))}
      </div>
    </div>
  );
}
