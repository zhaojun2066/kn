export type AuthMode = "api_key" | "local_login" | "token";

export const AUTH_MODE_LABELS: Record<AuthMode, string> = {
  api_key: "API Key",
  local_login: "账号登录",
  token: "Token/PAT",
};

const SYSTEM_ENV_KEYS = new Set(["_KN_CLI_TYPE", "_KN_TAGS", "_KN_AUTH_MODE", "_KN_PROVIDER_ID"]);

export function isSystemEnvKey(key: string): boolean {
  return SYSTEM_ENV_KEYS.has(key);
}

export function isSecretEnvKey(key: string): boolean {
  const upper = key.toUpperCase();
  return upper.includes("KEY") || upper.includes("TOKEN") || upper.includes("SECRET") || upper.includes("PASSWORD");
}

export function maskSecret(value: string): string {
  if (!value) return "";
  if (value.length <= 8) return "••••";
  return `${value.slice(0, 4)}••••${value.slice(-4)}`;
}

export function displayEnvValue(key: string, value: string): string {
  return isSecretEnvKey(key) ? maskSecret(value) : value;
}

export function inferAuthMode(cliType: string | undefined, env: Record<string, string>): AuthMode | undefined {
  const stored = env._KN_AUTH_MODE as AuthMode | undefined;
  if (stored === "api_key" || stored === "local_login" || stored === "token") return stored;

  if (cliType === "qoderclicn" || env._KN_CLI_TYPE === "qoderclicn") {
    if (env.QODERCN_PERSONAL_ACCESS_TOKEN) return "token";
    return "local_login";
  }
  if (cliType === "codex" || env._KN_CLI_TYPE === "codex") {
    if (env.OPENAI_API_KEY) return "api_key";
    return "local_login";
  }
  if (env.ANTHROPIC_API_KEY || env.ANTHROPIC_AUTH_TOKEN || env.OPENAI_API_KEY) return "api_key";
  return undefined;
}

export function authModeLabel(mode?: string): string {
  if (mode === "api_key" || mode === "local_login" || mode === "token") return AUTH_MODE_LABELS[mode];
  return "未标记";
}
