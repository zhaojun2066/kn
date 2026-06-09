/// <reference types="vitest" />
import { describe, it, expect } from "vitest";
import { detectKeyFormat } from "../key-format-detector";

describe("detectKeyFormat", () => {
  it("detects Anthropic key (sk-ant-api03-)", () => {
    const r = detectKeyFormat("sk-ant-api03-abc123");
    expect(r).not.toBeNull();
    expect(r!.compatibleTools).toContain("claude");
  });

  it("detects OpenRouter key (sk-or-v1-)", () => {
    const r = detectKeyFormat("sk-or-v1-xyz789");
    expect(r).not.toBeNull();
    expect(r!.compatibleTools).toContain("codex");
  });

  it("detects Groq key (gsk_)", () => {
    const r = detectKeyFormat("gsk_abc123");
    expect(r).not.toBeNull();
  });

  it("detects generic OpenAI-format key (sk-)", () => {
    const r = detectKeyFormat("sk-proj-abc123");
    expect(r).not.toBeNull();
  });

  it("returns null for empty input", () => {
    expect(detectKeyFormat("")).toBeNull();
  });

  it("returns null for whitespace-only input", () => {
    expect(detectKeyFormat("   ")).toBeNull();
  });

  it("returns null for unrecognized format", () => {
    expect(detectKeyFormat("my-custom-key")).toBeNull();
  });

  it("trims whitespace before matching", () => {
    expect(detectKeyFormat("  sk-ant-api03-abc  ")).not.toBeNull();
  });

  it("case-insensitive match", () => {
    expect(detectKeyFormat("SK-ANT-API03-ABC")).not.toBeNull();
  });
});
