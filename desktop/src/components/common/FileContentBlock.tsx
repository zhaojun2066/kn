import { useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import hljs from "highlight.js";
import { marked } from "marked";
import { PdfViewer } from "../FileTree";
import { basename } from "../../lib/path-utils";

/* ──────────────────── Helpers ──────────────────── */

const IMG_EXTS = ["png", "jpg", "jpeg", "gif", "svg", "webp", "bmp", "ico"];

export function isImagePath(path: string): boolean {
  const ext = path.split(".").pop()?.toLowerCase();
  return !!ext && IMG_EXTS.includes(ext);
}

export function isPdfPath(path: string): boolean {
  return /\.pdf$/i.test(path);
}

export function langFromPath(path: string): string | null {
  const ext = path.split(".").pop()?.toLowerCase() || "";
  const map: Record<string, string> = {
    ts: "typescript", tsx: "typescript", js: "javascript", jsx: "javascript",
    py: "python", rs: "rust", go: "go", rb: "ruby",
    sh: "bash", bash: "bash", zsh: "bash",
    md: "markdown", markdown: "markdown",
    yaml: "yaml", yml: "yaml", toml: "ini",
    json: "json", jsonc: "json",
    css: "css", scss: "scss", less: "less",
    html: "xml", htm: "xml", xml: "xml", svg: "xml",
    sql: "sql", graphql: "graphql", gql: "graphql",
    java: "java", kt: "kotlin", scala: "scala",
    c: "c", cpp: "cpp", h: "c", hpp: "cpp",
    cs: "csharp", fs: "fsharp",
    swift: "swift", m: "objectivec",
    lua: "lua", r: "r", dart: "dart",
    dockerfile: "dockerfile", makefile: "makefile",
    nix: "nix", tf: "hcl", hcl: "hcl",
    proto: "protobuf",
  };
  return map[ext] || null;
}

/* ──────────────────── Component ──────────────────── */

interface FileContentBlockProps {
  content: string;
  filePath?: string;
}

export function FileContentBlock({ content, filePath }: FileContentBlockProps) {
  const [mdView, setMdView] = useState<"source" | "preview">("preview");
  const isMd = filePath ? /\.(md|markdown)$/i.test(filePath) : false;

  if (filePath && isImagePath(filePath)) {
    return (
      <div className="flex items-center justify-center p-4">
        <img
          src={convertFileSrc(filePath)}
          alt={basename(filePath) || "image"}
          className="max-w-full max-h-[60vh] object-contain border border-[var(--app-border-light)]"
          style={{ background: "repeating-conic-gradient(var(--app-border-light) 0% 25%, transparent 0% 50%) 50% / 20px 20px" }}
        />
      </div>
    );
  }

  if (filePath && isPdfPath(filePath)) {
    return <PdfViewer filePath={filePath} />;
  }

  if (!content) return null;

  const lang = filePath ? langFromPath(filePath) : null;
  let highlighted = "";
  if (lang) {
    try {
      highlighted = hljs.highlight(content, { language: lang, ignoreIllegals: true }).value;
    } catch (_) { /* fall through */ }
  }
  if (!highlighted) {
    try {
      const r = hljs.highlightAuto(content);
      if (r.relevance > 3) highlighted = r.value;
    } catch (_) { /* fall through */ }
  }

  if (isMd) {
    let previewHtml = "";
    try { previewHtml = marked.parse(content) as string; } catch (_) {}
    return (
      <div>
        <div className="flex items-center justify-end gap-0.5 px-3 py-1.5">
          <button
            onClick={() => setMdView("source")}
            className={`px-2 py-0.5 text-2xs font-mono border transition-colors ${
              mdView === "source"
                ? "bg-[var(--app-accent)] text-[var(--app-bg)] border-[var(--app-accent)]"
                : "text-[var(--app-text-muted)] border-[var(--app-border)] hover:text-[var(--app-text)]"
            }`}
          >源码</button>
          <button
            onClick={() => setMdView("preview")}
            className={`px-2 py-0.5 text-2xs font-mono border transition-colors ${
              mdView === "preview"
                ? "bg-[var(--app-accent)] text-[var(--app-bg)] border-[var(--app-accent)]"
                : "text-[var(--app-text-muted)] border-[var(--app-border)] hover:text-[var(--app-text)]"
            }`}
          >预览</button>
        </div>
        {mdView === "source" ? (
          highlighted ? (
            <pre className="p-3 text-2xs font-mono leading-relaxed whitespace-pre-wrap bg-[var(--app-bg)] border border-[var(--app-border-light)] max-h-[60vh] overflow-y-auto">
              <code dangerouslySetInnerHTML={{ __html: highlighted }} />
            </pre>
          ) : (
            <pre className="p-3 text-2xs text-[var(--app-text-dim)] font-mono leading-relaxed whitespace-pre-wrap bg-[var(--app-bg)] border border-[var(--app-border-light)] max-h-[60vh] overflow-y-auto">
              {content}
            </pre>
          )
        ) : (
          <div
            className="md-preview p-4 text-xs text-[var(--app-text-dim)] leading-relaxed bg-[var(--app-bg)] border border-[var(--app-border-light)] max-h-[60vh] overflow-y-auto"
            dangerouslySetInnerHTML={{ __html: previewHtml }}
          />
        )}
      </div>
    );
  }

  if (highlighted) {
    return (
      <pre className="p-3 text-2xs font-mono leading-relaxed whitespace-pre-wrap bg-[var(--app-bg)] border border-[var(--app-border-light)] max-h-[60vh] overflow-y-auto">
        <code dangerouslySetInnerHTML={{ __html: highlighted }} />
      </pre>
    );
  }

  return (
    <pre className="p-3 text-2xs text-[var(--app-text-dim)] font-mono leading-relaxed whitespace-pre-wrap bg-[var(--app-bg)] border border-[var(--app-border-light)] max-h-[60vh] overflow-y-auto">
      {content}
    </pre>
  );
}
