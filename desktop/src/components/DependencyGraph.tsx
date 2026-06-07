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
  parent?: string;
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

import { CLI_HEX_COLORS } from "../lib/cli-constants";
const CLI_COLORS = CLI_HEX_COLORS;

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
  const onNodeClickRef = useRef(onNodeClick);
  onNodeClickRef.current = onNodeClick;

  const initCytoscape = useCallback(() => {
    if (!containerRef.current || !data) return;

    // Destroy existing instance
    cyRef.current?.destroy();

    const nodeIds = new Set(data.nodes.map((n) => n.id));
    const elements: cytoscape.ElementDefinition[] = [
      ...data.nodes.map((n) => ({
        data: {
          id: n.id,
          label: n.label,
          kind: n.kind,
          cli: n.cli,
          source: n.source,
          locked: n.locked ? "yes" : "no",
          ...(n.parent && nodeIds.has(n.parent) ? { parent: n.parent } : {}),
        },
      })),
      ...data.edges
        .filter((e) => nodeIds.has(e.from) && nodeIds.has(e.to))
        .map((e) => ({
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
            "font-size": "8px",
            "font-family": "ui-monospace, monospace",
            color: "#E5E7EB",
            "text-valign": "bottom",
            "text-halign": "center",
            "text-margin-y": 4,
            "text-max-width": "60px",
            "text-wrap": "ellipsis",
            width: 14,
            height: 14,
            "border-width": 1.5,
            "border-color": (el) => {
              const locked = el.data("locked") as string;
              return locked === "yes" ? "#6B7280" : "transparent";
            },
            "border-opacity": 0.5,
            shape: (el) => {
              const kind = el.data("kind") as string;
              return (NODE_SHAPES[kind] || "ellipse") as cytoscape.Css.NodeShape;
            },
          },
        },
        {
          selector: 'node[locked="yes"]',
          style: {
            "border-style": "dashed",
            "background-opacity": 0.6,
          },
        },
        {
          selector: "node:parent",
          style: {
            "background-color": (el) => {
              const cli = el.data("cli") as string;
              return CLI_COLORS[cli] || "#6B7280";
            },
            "background-opacity": 0.08,
            "border-width": 2,
            "border-color": (el) => {
              const cli = el.data("cli") as string;
              return CLI_COLORS[cli] || "#6B7280";
            },
            "border-opacity": 0.4,
            "padding-top": 18,
            "padding-left": 8,
            "padding-right": 8,
            "padding-bottom": 8,
            shape: "round-rectangle",
            label: "data(label)",
            "text-valign": "top",
            "text-halign": "center",
            "text-margin-y": -4,
            "font-size": "10px",
            "font-weight": "bold",
            color: "#E5E7EB",
          },
        },
        ...Object.entries(EDGE_STYLES).map(([kind, { style, color }]) => ({
          selector: `edge[kind="${kind}"]`,
          style: {
            "line-color": color,
            "line-style": style,
            "target-arrow-color": color,
            "target-arrow-shape": "triangle",
            width: 0.8,
            "arrow-scale": 0.6,
            opacity: 0.3,
            label: "data(label)",
            "font-size": "6px",
            color: "#9CA3AF",
          },
        })),
      ],
      layout: {
        name: "cose",
        animate: false,
        nodeRepulsion: 8000,
        idealEdgeLength: 50,
        edgeElasticity: 100,
        gravity: 0.25,
        numIter: 500,
        tile: true,
        tilingPaddingVertical: 20,
        tilingPaddingHorizontal: 20,
        initialTemp: 200,
        coolingFactor: 0.95,
        minTemp: 1.0,
      },
      minZoom: 0.3,
      maxZoom: 3,
    });

    // Fit the graph AFTER layout completes (layout is async even with animate:false)
    cy.one("layoutstop", () => {
      cy.fit(undefined, 50);
    });

    // Zoom-based label visibility
    const updateLabels = () => {
      const zoom = cy.zoom();
      cy.batch(() => {
        cy.nodes().style("text-opacity", zoom > 1.0 ? 1 : 0);
        cy.edges().style("text-opacity", zoom > 1.5 ? 0.8 : 0);
      });
    };
    cy.on("zoom", updateLabels);
    updateLabels();

    // Highlight neighbors on hover
    cy.on("mouseover", "node", (evt: EventObject) => {
      const node = evt.target;
      if (node.isParent()) return;
      const neighborhood = node.closedNeighborhood();
      cy.elements().not(neighborhood).style("opacity", "0.1");
      neighborhood.style({ opacity: "1", "text-opacity": "1" });
      node.style({ width: 20, height: 20 });
    });

    cy.on("mouseout", "node", (evt: EventObject) => {
      const node = evt.target;
      if (node.isParent()) return;
      const zoom = cy.zoom();
      cy.elements().style("opacity", "1");
      cy.nodes().not(":parent").style({ width: 14, height: 14 });
      cy.nodes().not(":parent").style("text-opacity", zoom > 1.0 ? 1 : 0);
      cy.edges().style("text-opacity", zoom > 1.5 ? 0.8 : 0);
    });

    // Click → select node
    cy.on("tap", "node", (evt: EventObject) => {
      const nodeId = evt.target.id();
      onNodeClickRef.current?.(nodeId);
    });

    cyRef.current = cy;
  }, [data]);

  useEffect(() => {
    // Small delay to ensure container has dimensions
    const timer = setTimeout(() => initCytoscape(), 100);
    return () => {
      clearTimeout(timer);
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
    <div className="flex flex-col h-full" style={{ flex: 1, minHeight: 0 }}>
      <div className="flex items-center gap-2 px-3 py-2 border-b border-[var(--app-border)] shrink-0">
        <span className="text-2xs text-[var(--app-text-dim)] font-mono uppercase tracking-wider">
          Dependency Graph
        </span>
        <div className="flex-1" />
        <span className="text-2xs text-[var(--app-text-muted)] font-mono">
          {data.nodes.length} nodes · {data.edges.length} edges
        </span>
        <Legend />
      </div>
      <div
        style={{
          flex: 1,
          width: "100%",
          minHeight: 400,
          background: "#16162a",
          position: "relative",
        }}
      >
        <div
          ref={containerRef}
          style={{
            width: "100%",
            height: "100%",
            position: "absolute",
            top: 0,
            left: 0,
          }}
        />
      </div>
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
