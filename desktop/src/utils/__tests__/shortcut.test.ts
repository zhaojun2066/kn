/// <reference types="vitest" />
import { describe, it, expect } from "vitest";
import { formatShortcut } from "../shortcut";

// Mock navigator for jsdom
function mockUserAgent(ua: string) {
  Object.defineProperty(navigator, "userAgent", {
    value: ua, configurable: true,
  });
}

describe("formatShortcut", () => {
  it("replaces mod+ with ⌘ on Mac", () => {
    mockUserAgent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)");
    expect(formatShortcut("mod+N")).toBe("⌘+N");
  });

  it("replaces mod+ with Ctrl on Windows", () => {
    mockUserAgent("Mozilla/5.0 (Windows NT 10.0; Win64; x64)");
    expect(formatShortcut("mod+N")).toBe("Ctrl+N");
  });

  it("leaves shortcut without mod unchanged", () => {
    expect(formatShortcut("Ctrl+`")).toBe("Ctrl+`");
  });

  it("replaces multiple mod occurrences", () => {
    mockUserAgent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)");
    expect(formatShortcut("mod+K mod+U")).toBe("⌘+K ⌘+U");
  });

  it("does not replace 'mod' inside a word", () => {
    mockUserAgent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)");
    expect(formatShortcut("modification")).toBe("modification");
  });
});
