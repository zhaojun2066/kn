/** 当前平台是 macOS */
export function isMac(): boolean {
  return navigator.userAgent.includes("Mac");
}

/** 返回修饰键符号：Mac → "⌘"，Windows/Linux → "Ctrl" */
export function modKey(): string {
  return isMac() ? "⌘" : "Ctrl";
}

/** 返回 Option/Alt 键符号：Mac → "⌥"，Windows/Linux → "Alt" */
export function altKey(): string {
  return isMac() ? "⌥" : "Alt";
}

/**
 * 格式化快捷键字符串，自动替换修饰键占位符
 *
 * 用法：
 *   formatShortcut("mod+N")       → Mac: "⌘N",   Win: "Ctrl+N"
 *   formatShortcut("mod+Shift+M") → Mac: "⌘⇧M",  Win: "Ctrl+⇧M"
 *   formatShortcut("Ctrl+`")      → 不变（没有 mod 占位符）
 */
export function formatShortcut(shortcut: string): string {
  return shortcut.replace(/\bmod\b/g, modKey());
}
