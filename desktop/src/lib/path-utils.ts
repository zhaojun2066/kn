/** Shorten home directory paths for display across platforms */
export function shortenPath(path: string): string {
  return path
    .replace(/^\/Users\/[^/]+/, "~")     // macOS
    .replace(/^\/home\/[^/]+/, "~")       // Linux
    .replace(/^[A-Z]:\\Users\\[^\\]+/i, "~"); // Windows
}

/**
 * Cross-platform basename (last path segment).
 * Works with both / and \ separators.
 */
export function basename(path: string): string {
  return path.replace(/\\/g, "/").split("/").pop() || "";
}

/**
 * Cross-platform dirname (all but last path segment).
 * Works with both / and \ separators.
 */
export function dirname(path: string): string {
  const parts = path.replace(/\\/g, "/").split("/");
  parts.pop();
  return parts.join("/");
}
