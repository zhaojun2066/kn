import { useEffect, useRef, useCallback } from "react";
import cytoscape, { type Core, type EventObject } from "cytoscape";

// ── Node/Edge types from Rust (mirrors agent_manager.rs) ──

export interface DepNode {
  id: string;
  kind: "plugin" | "agent" | "skill" | "tool" | "mcp";
  label: string;
  cli: string;
  source: string;
  locked: boolean;
}

export interface DepEdge {
  from: string;
  to: string;
  kind: "contains" | "references" | "spawns" | "needsTool" | "needsModel";
  label: string;
}

export interface DependencyGraphData {
  nodes: DepNode[];
  edges: DepEdge[];
}

interface DependencyGraphProps {
  data: DependencyGraphData | null;
  onNodeClick?: (nodeId: string) => void;
}

// ── Visual styling constants ──

const CLI_COLORS: Record<string, string> = {
  claude: "#D97706",
  codex: "#7C3AED",
  qoder: "#059669",
};

const NODE_SHAPES: Record<string, string> = {
  plugin: "hexagon",
  agent: "ellipse",
  skill: "diamond",
  tool: "rectangle",
  mcp: "round-rectangle",
};

const EDGE_STYLES: Record<string, { style: string; color: string }> = {
  contains: { style: "solid", color: "#6B7280" },
  references: { style: "dashed", color: "#8B5CF6" },
  spawns: { style: "dotted", color: "#3B82F6" },
  needsTool: { style: "solid", color: "#10B981" },
  needsModel: { style: "dashed", color: "#F59E0B" },
};

// ── Component ──

export function DependencyGraph({ data, onNodeClick }: DependencyGraphProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const cyRef = useRef<Core | null>(null);

  const initCytoscape = useCallback(() => {
    if (!containerRef.current || !data) return;

    // Destroy existing instance
    cyRef.current?.destroy();

    const elements: cytoscape.ElementDefinition[] = [
      ...data.nodes.map((n) => ({
        data: {
          id: n.id,
          label: n.label,
          kind: n.kind,
          cli: n.cli,
          source: n.source,
          locked: n.locked,
        },
      })),
      ...data.edges.map((e) => ({
        data: {
          id: `${e.from}->${e.to}:${e.kind}`,
          source: e.from,
          target: e.to,
          kind: e.kind,
          label: e.label,
        },
      })),
    ];

    const cy = cytoscape({
      container: containerRef.current,
      elements,
      style: [
        {
          selector: "node",
          style: {
            "background-color": (el) => {
              const cli = el.data("cli") as string;
              return CLI_COLORS[cli] || "#6B7280";
            },
            label: "data(label)",
            "font-size": "9px",
            "font-family": "ui-monospace, monospace",
            color: "#E5E7EB",
            "text-valign": "bottom",
            "text-halign": "center",
            "text-margin-y": 6,
            width: 24,
            height: 24,
            "border-width": 2,
            "border-color": (el) => {
              const locked = el.data("locked") as boolean;
              return locked ? "#6B7280" : "transparent";
            },
            "border-opacity": 0.5,
            shape: (el) => {
              const kind = el.data("kind") as string;
              return (NODE_SHAPES[kind] || "ellipse") as cytoscape.Css.NodeShape;
            },
          },
        },
        {
          selector: "node[locked=true]",
          style: {
            "border-style": "dashed",
            "background-opacity": 0.6,
          },
        },
        ...Object.entries(EDGE_STYLES).map(([kind, { style, color }]) => ({
          selector: `edge[kind="${kind}"]`,
          style: {
            "line-color": color,
            "line-style": style,
            "target-arrow-color": color,
            "target-arrow-shape": "triangle",
            width: 1.5,
            "arrow-scale": 0.8,
            opacity: 0.6,
            label: "data(label)",
            "font-size": "7px",
            color: "#9CA3AF",
          },
        })),
      ],
      layout: {
        name: "cose",
        animate: true,
        animationDuration: 500,
        nodeRepulsion: () => 4000,
        gravity: 0.25,
        idealEdgeLength: () => 100,
      },
      minZoom: 0.3,
      maxZoom: 3,
    });

    // Highlight neighbors on hover
    cy.on("mouseover", "node", (evt: EventObject) => {
      const node = evt.target;
      const neighborhood = node.closedNeighborhood();
      cy.elements().not(neighborhood).style("opacity", "0.2");
      neighborhood.style("opacity", "1");
    });

    cy.on("mouseout", "node", () => {
      cy.elements().style("opacity", "1");
    });

    // Click → select node
    cy.on("tap", "node", (evt: EventObject) => {
      const nodeId = evt.target.id();
      onNodeClick?.(nodeId);
    });

    cyRef.current = cy;
  }, [data, onNodeClick]);

  useEffect(() => {
    initCytoscape();
    return () => {
      cyRef.current?.destroy();
    };
  }, [initCytoscape]);

  if (!data) {
    return (
      <div className="flex items-center justify-center h-full text-xs text-[var(--app-text-dim)]">
        No dependency data available
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center gap-2 px-3 py-2 border-b border-[var(--app-border)]">
        <span className="text-2xs text-[var(--app-text-dim)] font-mono uppercase tracking-wider">
          Dependency Graph
        </span>
        <div className="flex-1" />
        <Legend />
      </div>
      <div ref={containerRef} className="flex-1 bg-[var(--app-bg)]" />
    </div>
  );
}

function Legend() {
  return (
    <div className="flex items-center gap-3 text-2xs text-[var(--app-text-dim)] font-mono">
      <span className="flex items-center gap-1">
        <span className="w-2 h-2 rounded-full bg-[#D97706]" /> Claude
      </span>
      <span className="flex items-center gap-1">
        <span className="w-2 h-2 rounded-full bg-[#7C3AED]" /> Codex
      </span>
      <span className="flex items-center gap-1">
        <span className="w-2 h-2 rounded-full bg-[#059669]" /> Qoder
      </span>
      <span className="text-[var(--app-border)]">|</span>
      <span className="flex items-center gap-1">
        <span className="w-3 border-t border-dashed border-[#8B5CF6]" /> refs
      </span>
      <span className="flex items-center gap-1">
        <span className="w-3 border-t border-[#6B7280]" /> contains
      </span>
    </div>
  );
}
