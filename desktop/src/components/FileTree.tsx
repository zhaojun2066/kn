import React, { useState, useEffect, useCallback, useRef } from "react";
import {
  Folder, FolderOpen, File, FileText, FileCode, FileJson,
  FileImage, Loader, AlertTriangle, ChevronRight,
} from "lucide-react";
import { invoke } from "@tauri-apps/api/core";

/* ──────────────────── Types ──────────────────── */

export interface FileTreeNode {
  name: string;
  path: string;
  is_dir: boolean;
  children?: FileTreeNode[];
}

interface FileTreeProps {
  rootPath: string;
  onSelect: (node: FileTreeNode) => void;
  activePath?: string;
  defaultOpenFile?: string;
}

/* ──────────────────── File icon resolver ──────────────────── */

const SIZE = 14;

function FileIcon({ name, isDir, expanded }: { name: string; isDir: boolean; expanded?: boolean }) {
  if (isDir) {
    return expanded
      ? <FolderOpen size={SIZE} className="text-[var(--app-amber)] shrink-0" />
      : <Folder size={SIZE} className="text-[var(--app-amber)] shrink-0" />;
  }
  const ext = name.includes(".") ? name.split(".").pop()?.toLowerCase() : "";
  switch (ext) {
    case "md":
      return <FileText size={SIZE} className="text-[var(--app-accent)] shrink-0" />;
    case "toml":
    case "yaml":
    case "yml":
    case "json":
      return <FileJson size={SIZE} className="text-[var(--app-blue)] shrink-0" />;
    case "sh":
    case "py":
    case "js":
    case "ts":
      return <FileCode size={SIZE} className="text-[var(--app-purple)] shrink-0" />;
    case "png":
    case "jpg":
    case "jpeg":
    case "gif":
    case "svg":
    case "webp":
    case "bmp":
    case "ico":
      return <FileImage size={SIZE} className="text-[var(--app-teal)] shrink-0" />;
    case "pdf":
      return <FileText size={SIZE} className="text-[var(--app-red)] shrink-0" />;
    case "lock":
      return <FileJson size={SIZE} className="text-[var(--app-text-muted)] shrink-0" />;
    default:
      return <File size={SIZE} className="text-[var(--app-text-dim)] shrink-0" />;
  }
}

/* ──────────────────── Tree node ──────────────────── */

const INDENT = 16;

function TreeNode({
  node,
  depth,
  activePath,
  onSelect,
  defaultExpanded,
}: {
  node: FileTreeNode;
  depth: number;
  activePath: string | undefined;
  onSelect: (node: FileTreeNode) => void;
  defaultExpanded: boolean;
}) {
  const [expanded, setExpanded] = useState(depth === 0 ? true : defaultExpanded);
  const isActive = activePath === node.path;
  const hasChildren = node.is_dir && node.children && node.children.length > 0;

  const handleClick = useCallback(() => {
    if (node.is_dir) {
      if (hasChildren) setExpanded((v) => !v);
    } else {
      onSelect(node);
    }
  }, [node, hasChildren, onSelect]);

  return (
    <div>
      <div
        className="flex items-center gap-1.5 cursor-pointer select-none group"
        style={{ paddingLeft: depth * INDENT + 8 }}
        onClick={handleClick}
      >
        {/* Expand/collapse chevron for directories */}
        {node.is_dir ? (
          <span className="w-4 h-4 flex items-center justify-center shrink-0">
            <ChevronRight
              size={12}
              className={`text-[var(--app-text-muted)] transition-transform duration-150 ${
                expanded ? "rotate-90" : ""
              }`}
            />
          </span>
        ) : (
          <span className="w-4 shrink-0" />
        )}
        <FileIcon name={node.name} isDir={node.is_dir} expanded={expanded} />
        <span
          className={`text-[11px] font-mono truncate leading-6 ${
            isActive
              ? "text-[var(--app-accent)]"
              : "text-[var(--app-text-dim)] group-hover:text-[var(--app-text)]"
          }`}
        >
          {node.name}
        </span>
        {/* Active indicator bar */}
        {isActive && (
          <div className="ml-auto mr-1 w-1 h-4 rounded-full bg-[var(--app-accent)] shrink-0" />
        )}
      </div>
      {/* Children */}
      {expanded && hasChildren && (
        <div>
          {node.children!.map((child) => (
            <TreeNode
              key={child.path}
              node={child}
              depth={depth + 1}
              activePath={activePath}
              onSelect={onSelect}
              defaultExpanded={depth < 1}
            />
          ))}
        </div>
      )}
      {/* Empty directory indicator */}
      {expanded && node.is_dir && (!node.children || node.children.length === 0) && (
        <div
          className="flex items-center gap-1.5 text-[10px] text-[var(--app-text-muted)] font-mono pl-4"
          style={{ paddingLeft: (depth + 1) * INDENT + 20 }}
        >
          <Folder size={10} className="opacity-40 shrink-0" />
          <span>空目录</span>
        </div>
      )}
    </div>
  );
}

/* ──────────────────── FileTree ──────────────────── */

export function FileTree({ rootPath, onSelect, activePath, defaultOpenFile }: FileTreeProps) {
  const [tree, setTree] = useState<FileTreeNode | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const loadedRef = useRef<string | null>(null);
  const onSelectRef = useRef(onSelect);
  onSelectRef.current = onSelect;
  const defaultOpenFileRef = useRef(defaultOpenFile);
  defaultOpenFileRef.current = defaultOpenFile;

  useEffect(() => {
    if (!rootPath || loadedRef.current === rootPath) return;
    loadedRef.current = rootPath;
    setLoading(true);
    setError(null);

    let cancelled = false;
    invoke<FileTreeNode>("list_directory_tree", { path: rootPath })
      .then((t) => {
        if (cancelled) return;
        setTree(t);
        const file = defaultOpenFileRef.current;
        if (file) {
          const found = findNode(t, file);
          if (found) onSelectRef.current(found);
        }
      })
      .catch((e) => { if (!cancelled) setError(String(e)); })
      .finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; loadedRef.current = null; };
  }, [rootPath]);

  /* ── States ── */
  if (loading) {
    return (
      <div className="flex items-center gap-2 px-3 py-4 text-xs text-[var(--app-text-muted)] font-mono">
        <Loader size={14} className="animate-spin shrink-0" />
        加载目录...
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex items-start gap-2 px-3 py-4 text-xs text-[var(--app-red)] font-mono">
        <AlertTriangle size={14} className="shrink-0 mt-0.5" />
        <span className="leading-relaxed">{error}</span>
      </div>
    );
  }

  if (!tree) {
    return (
      <div className="px-3 py-4 text-xs text-[var(--app-text-muted)] font-mono">
        无法加载目录
      </div>
    );
  }

  return (
    <div className="py-2">
      <TreeNode
        node={tree}
        depth={0}
        activePath={activePath}
        onSelect={onSelect}
        defaultExpanded={true}
      />
    </div>
  );
}

/* ──────────────────── PdfViewer ──────────────────── */

export function PdfViewer({ filePath }: { filePath: string }) {
  const [src, setSrc] = useState<string>("");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setSrc("");
    setError(null);
    invoke<string>("read_file_base64", { path: filePath })
      .then((base64) => {
        if (cancelled) return;
        setSrc(`data:application/pdf;base64,${base64}`);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [filePath]);

  if (error) {
    return (
      <div className="flex items-center gap-2 px-3 py-4 text-xs text-[var(--app-red)] font-mono">
        <AlertTriangle size={14} className="shrink-0" />
        <span>PDF 加载失败: {error}</span>
      </div>
    );
  }

  if (!src) {
    return (
      <div className="flex items-center gap-2 px-3 py-4 text-xs text-[var(--app-text-muted)] font-mono">
        <Loader size={14} className="animate-spin shrink-0" />
        加载 PDF...
      </div>
    );
  }

  return (
    <iframe
      src={src}
      className="w-full h-[60vh] border border-[var(--app-border-light)]"
      title="PDF Preview"
    />
  );
}

/* ──────────────────── Utility ──────────────────── */

function findNode(tree: FileTreeNode, name: string): FileTreeNode | null {
  // Exact match on basename
  if (tree.name === name && !tree.is_dir) return tree;
  if (tree.children) {
    for (const child of tree.children) {
      // Direct match first
      if (!child.is_dir && child.name === name) return child;
      // Recurse into dirs
      const found = findNode(child, name);
      if (found) return found;
    }
  }
  return null;
}

export default FileTree;
