/// <reference types="vitest" />
import { describe, expect, it } from "vitest";
import { getRunCommandPolicy } from "../utils";

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
