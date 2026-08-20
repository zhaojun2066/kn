import { describe, expect, it } from "vitest";
import { buildRedactedHealthReport, healthToolLabel } from "../healthReport";

describe("buildRedactedHealthReport", () => {
  it("keeps only the public health contract", () => {
    const potentiallyUnsafeSnapshot = {
      schemaVersion: 1,
      generatedAt: 123,
      agent: {
        version: "1.2.3",
        environment: "production",
        token: "must-not-copy",
      },
      connection: { state: "connected", url: "wss://private" },
      tools: [
        { name: "gh", state: "notAuthenticated", version: "2.61.0", output: "private" },
        { name: "unknown", state: "available", version: "9.9.9" },
      ],
      projectPath: "/private/project",
    };
    const report = buildRedactedHealthReport(
      potentiallyUnsafeSnapshot as unknown as Parameters<typeof buildRedactedHealthReport>[0],
    );

    expect(report).toContain('"schemaVersion": 1');
    expect(report).toContain('"name": "gh"');
    expect(report).not.toContain("must-not-copy");
    expect(report).not.toContain("wss://private");
    expect(report).not.toContain("/private/project");
    expect(report).not.toContain('"unknown"');
  });

  it("keeps the domestic QoderCN diagnostic tool and labels it clearly", () => {
    const report = buildRedactedHealthReport({
      schemaVersion: 1,
      generatedAt: 123,
      agent: { version: "1.2.3", environment: "production" },
      connection: { state: "connected" },
      tools: [{ name: "qoderclicn", state: "available", version: "1.0.0" }],
    });

    expect(report).toContain('"name": "qoderclicn"');
    expect(healthToolLabel({ name: "qoderclicn", state: "available" })).toBe(
      "QoderCN · 可用",
    );
  });
});
