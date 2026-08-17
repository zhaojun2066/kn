/// <reference types="vitest" />
import { describe, expect, it } from "vitest";
import { getRunCommandPolicy, isProfileCompatibleWithSession } from "../utils";

describe("run command policy", () => {
  it("runs AI profile commands locally and registers them as Relay sessions", () => {
    expect(getRunCommandPolicy("ai claude claude-config")).toEqual({
      execution: "local-pty",
      registerRelay: true,
      tool: "claude",
      profile: "claude-config",
    });
  });

  it("runs non-AI commands locally without agent registration", () => {
    expect(getRunCommandPolicy("npm test")).toEqual({
      execution: "local-pty",
      registerRelay: false,
      tool: null,
      profile: null,
    });
  });
});

describe("local session history profile compatibility", () => {
  it("rejects restoring a Codex history item with a same-named Claude profile", () => {
    expect(isProfileCompatibleWithSession(
      "ai codex shared-profile",
      [{ name: "shared-profile", cli_type: "claude" }],
    )).toBe(false);
  });

  it("does not treat qoder as qoderclicn", () => {
    expect(isProfileCompatibleWithSession(
      "ai qoderclicn qoder-profile",
      [{ name: "qoder-profile", cli_type: "qoder" }],
    )).toBe(false);
  });
});
