/// <reference types="vitest" />
import { describe, it, expect } from "vitest";
import { shortenPath, basename, dirname } from "../path-utils";

describe("shortenPath", () => {
  it("shortens macOS home path", () => {
    expect(shortenPath("/Users/john/project/file.ts")).toBe("~/project/file.ts");
  });
  it("shortens Linux home path", () => {
    expect(shortenPath("/home/john/project/file.ts")).toBe("~/project/file.ts");
  });
  it("does not affect non-home paths", () => {
    expect(shortenPath("/opt/homebrew/bin/node")).toBe("/opt/homebrew/bin/node");
  });
  it("returns path unchanged if not in home", () => {
    expect(shortenPath("/opt/homebrew/bin/node")).toBe("/opt/homebrew/bin/node");
  });
  it("handles empty path", () => {
    expect(shortenPath("")).toBe("");
  });
});

describe("basename", () => {
  it("extracts last segment on Unix", () => {
    expect(basename("/a/b/c/file.txt")).toBe("file.txt");
  });
  it("extracts last segment on Windows", () => {
    expect(basename("C:\\a\\b\\c\\file.txt")).toBe("file.txt");
  });
  it("returns full path if no separator", () => {
    expect(basename("file.txt")).toBe("file.txt");
  });
  it("returns empty for empty", () => {
    expect(basename("")).toBe("");
  });
});

describe("dirname", () => {
  it("returns parent directory on Unix", () => {
    expect(dirname("/a/b/c/file.txt")).toBe("/a/b/c");
  });
  it("returns parent directory on Windows (preserves backslashes)", () => {
    expect(dirname("C:\\a\\b\\c\\file.txt")).toBe("C:\\a\\b\\c");
  });
  it("returns empty for top-level file", () => {
    expect(dirname("file.txt")).toBe("");
  });
  it("returns empty for empty", () => {
    expect(dirname("")).toBe("");
  });
});
