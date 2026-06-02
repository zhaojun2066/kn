/** Shorten home directory paths for display across platforms */
export function shortenPath(path: string): string {
  return path
    .replace(/^\/Users\/[^/]+/, "~")     // macOS
    .replace(/^\/home\/[^/]+/, "~")       // Linux
    .replace(/^[A-Z]:\\Users\\[^\\]+/i, "~"); // Windows
}
