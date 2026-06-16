/**
 * Detect the dominant path separator in a string.
 * Returns `\` if backslashes are present, `/` otherwise.
 */
function detectSep(path: string): "/" | "\\" {
  return path.includes("\\") ? "\\" : "/";
}

/** Shorten home directory paths for display */
export function shortenPath(path: string): string {
  return path.replace(/^\/Users\/[^/]+/, "~");
}

/**
 * Cross-platform basename (last path segment).
 * Preserves the original path separator style.
 */
export function basename(path: string): string {
  // Split on both separator styles, return the last non-empty segment.
  // Normalize to `/` for splitting, then return the raw last part.
  const normalized = path.replace(/\\/g, "/");
  const idx = normalized.lastIndexOf("/");
  return idx >= 0 ? path.slice(idx + 1) : path;
}

/**
 * Cross-platform dirname (all but last path segment).
 * Preserves the original path separator style.
 */
export function dirname(path: string): string {
  const sep = detectSep(path);
  // Normalize to `/` for splitting, join back with detected separator
  const parts = path.replace(/\\/g, "/").split("/");
  parts.pop();
  return parts.join(sep);
}
