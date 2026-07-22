export type HealthConnectionState = "connected" | "reconnecting" | "notReady" | "unavailable";
export type HealthToolState = "available" | "missing" | "notAuthenticated" | "timedOut" | "error";

export interface AgentHealthSnapshot {
  schemaVersion: number;
  generatedAt: number;
  agent: { version: string; environment: "development" | "production" };
  connection: { state: HealthConnectionState };
  tools: Array<{ name: string; state: HealthToolState; version?: string }>;
}

const allowedToolNames = new Set(["git", "gh", "codex", "claude", "qoderclicn"]);

/**
 * Produces the only diagnostic text users can copy from the Desktop app.
 * Rebuild the object field-by-field so accidental IPC additions never leak
 * paths, command output, URLs, environment variables, or credentials.
 */
export function buildRedactedHealthReport(snapshot: AgentHealthSnapshot): string {
  const report = {
    schemaVersion: snapshot.schemaVersion,
    generatedAt: snapshot.generatedAt,
    agent: {
      version: snapshot.agent.version,
      environment: snapshot.agent.environment,
    },
    connection: {
      state: snapshot.connection.state,
    },
    tools: snapshot.tools
      .filter((tool) => allowedToolNames.has(tool.name))
      .map((tool) => ({
        name: tool.name,
        state: tool.state,
        ...(tool.version ? { version: tool.version } : {}),
      })),
  };

  return JSON.stringify(report, null, 2);
}

export function healthToolLabel(tool: AgentHealthSnapshot["tools"][number]): string {
  const names: Record<string, string> = {
    git: "Git",
    gh: "GitHub CLI",
    codex: "Codex",
    claude: "Claude",
    qoderclicn: "Qoderclicn（国内版）",
  };
  const states: Record<HealthToolState, string> = {
    available: "可用",
    missing: "未安装",
    notAuthenticated: "未登录",
    timedOut: "检查超时",
    error: "暂时无法检查",
  };
  return `${names[tool.name] ?? tool.name} · ${states[tool.state]}`;
}
