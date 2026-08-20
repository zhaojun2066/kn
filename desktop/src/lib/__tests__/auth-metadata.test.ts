import { describe, expect, it } from "vitest";
import { authModeLabel, displayEnvValue, inferAuthMode, isSystemEnvKey } from "../auth-metadata";

describe("auth metadata helpers", () => {
  it("infers Codex API key and local-login profiles", () => {
    expect(inferAuthMode("codex", { OPENAI_API_KEY: "sk-test" })).toBe("api_key");
    expect(inferAuthMode("codex", { _KN_CLI_TYPE: "codex" })).toBe("local_login");
  });

  it("infers QoderCN token and local-login profiles", () => {
    expect(inferAuthMode("qoderclicn", { QODERCN_PERSONAL_ACCESS_TOKEN: "qo-test" })).toBe("token");
    expect(inferAuthMode("qoderclicn", { _KN_CLI_TYPE: "qoderclicn" })).toBe("local_login");
  });

  it("labels auth modes and recognizes system metadata keys", () => {
    expect(authModeLabel("api_key")).toBe("API Key");
    expect(authModeLabel("local_login")).toBe("账号登录");
    expect(authModeLabel("token")).toBe("Token/PAT");
    expect(isSystemEnvKey("_KN_AUTH_MODE")).toBe(true);
    expect(isSystemEnvKey("OPENAI_API_KEY")).toBe(false);
  });

  it("masks secret values for previews", () => {
    expect(displayEnvValue("OPENAI_API_KEY", "sk-1234567890")).toBe("sk-1••••7890");
    expect(displayEnvValue("OPENAI_BASE_URL", "https://api.example.com")).toBe("https://api.example.com");
  });
});
