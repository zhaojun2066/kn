/**
 * Provider Presets — Tool × Provider × Env Template Matrix
 *
 * Central data source for CLI tool definitions and their compatible provider
 * presets.  Used by ProfileDialog (frontend) and conceptually mirrored by
 * the shell wrapper / Rust backend for tool validation.
 */

// ── Types ──────────────────────────────────────────────────────

export interface EnvVarDef {
  /** Environment variable name, e.g. "ANTHROPIC_API_KEY" */
  key: string;
  /** Display label (defaults to key if omitted) */
  label?: string;
  /** If true, the UI will show a required indicator */
  required: boolean;
  /** Placeholder text for the value input */
  placeholder?: string;
  /** Pre-filled default value */
  defaultValue?: string;
}

export interface ProviderPreset {
  /** Unique ID, e.g. "anthropic-official" */
  id: string;
  /** Display name, e.g. "Anthropic Official" */
  name: string;
  /** Short description shown on the preset card */
  description: string;
  /** Environment variable definitions for this preset */
  envVars: EnvVarDef[];
  /** Optional note shown on the env vars step (e.g. PAT vs /login warning) */
  note?: string;
}

export interface CLIToolDef {
  /** Tool identifier — matches shell wrapper case statement */
  id: string;
  /** Display name */
  name: string;
  /** Short description shown on the tool selection card */
  description: string;
  /** Single-letter icon fallback (used when SVG icon not available) */
  iconLetter: string;
  /** Provider presets compatible with this tool. Last one should be "custom". */
  providers: ProviderPreset[];
}

// ── Provider Presets ───────────────────────────────────────────

// --- Claude Code ---

const claudeProviders: ProviderPreset[] = [
  {
    id: "anthropic-official",
    name: "Anthropic 官方",
    description: "直接使用 Anthropic API",
    envVars: [
      { key: "ANTHROPIC_API_KEY", required: true, placeholder: "sk-ant-api03-..." },
      { key: "ANTHROPIC_BASE_URL", required: false, placeholder: "https://api.anthropic.com" },
      { key: "ANTHROPIC_MODEL", required: false, placeholder: "claude-opus-4-20250514" },
      {
        key: "ANTHROPIC_DEFAULT_HAIKU_MODEL", required: false,
        placeholder: "claude-haiku-4-5-20251001",
      },
      {
        key: "ANTHROPIC_DEFAULT_SONNET_MODEL", required: false,
        placeholder: "claude-sonnet-4-20250514",
      },
      {
        key: "ANTHROPIC_DEFAULT_OPUS_MODEL", required: false,
        placeholder: "claude-opus-4-20250514",
      },
      { key: "DISABLE_AUTOUPDATER", required: false, placeholder: "1 或 0" },
      { key: "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", required: false, placeholder: "1 或 0" },
      { key: "CLAUDE_CODE_EFFORT_LEVEL", required: false, placeholder: "max" },
    ],
  },
  {
    id: "aws-bedrock",
    name: "AWS Bedrock",
    description: "通过 AWS Bedrock 调用 Claude 模型",
    envVars: [
      { key: "AWS_ACCESS_KEY_ID", required: true, placeholder: "AKIA..." },
      { key: "AWS_SECRET_ACCESS_KEY", required: true, placeholder: "..." },
      { key: "AWS_REGION", required: true, defaultValue: "us-west-2" },
      { key: "ANTHROPIC_BEDROCK_MODEL", required: true, placeholder: "anthropic.claude-sonnet-4-20250514-v1:0" },
    ],
  },
  {
    id: "google-vertex",
    name: "Google Vertex AI",
    description: "通过 Vertex AI 调用 Claude 模型",
    envVars: [
      { key: "GOOGLE_APPLICATION_CREDENTIALS", required: true, placeholder: "/path/to/service-account.json" },
      { key: "GOOGLE_CLOUD_PROJECT", required: true, placeholder: "my-project-id" },
      { key: "GOOGLE_CLOUD_LOCATION", required: false, defaultValue: "us-central1" },
    ],
  },
  {
    id: "openrouter-anthropic",
    name: "OpenRouter",
    description: "通过 OpenRouter 调用 Claude 等模型（Anthropic 协议）",
    envVars: [
      { key: "ANTHROPIC_API_KEY", required: true, placeholder: "sk-or-v1-..." },
      { key: "ANTHROPIC_BASE_URL", required: true, defaultValue: "https://openrouter.ai/anthropic" },
      { key: "ANTHROPIC_MODEL", required: false, placeholder: "anthropic/claude-sonnet-4" },
    ],
  },
  {
    id: "deepseek-anthropic",
    name: "DeepSeek",
    description: "通过 DeepSeek Anthropic 兼容端点调用（官方推荐）",
    envVars: [
      { key: "ANTHROPIC_AUTH_TOKEN", required: true, placeholder: "sk-..." },
      { key: "ANTHROPIC_BASE_URL", required: true, defaultValue: "https://api.deepseek.com/anthropic" },
      { key: "ANTHROPIC_MODEL", required: false, defaultValue: "deepseek-v4-pro" },
      { key: "ANTHROPIC_DEFAULT_HAIKU_MODEL", required: false, defaultValue: "deepseek-v4-flash" },
      { key: "ANTHROPIC_DEFAULT_SONNET_MODEL", required: false, defaultValue: "deepseek-v4-pro" },
      { key: "ANTHROPIC_DEFAULT_OPUS_MODEL", required: false, defaultValue: "deepseek-v4-pro[1m]" },
      { key: "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME", required: false, defaultValue: "deepseek-v4-pro" },
      { key: "ANTHROPIC_DEFAULT_OPUS_MODEL_NAME", required: false, defaultValue: "deepseek-v4-pro" },
      { key: "DISABLE_AUTOUPDATER", required: false, defaultValue: "1" },
      { key: "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", required: false, defaultValue: "1" },
      { key: "CLAUDE_CODE_EFFORT_LEVEL", required: false, defaultValue: "max" },
    ],
  },
  {
    id: "xiaomi-mimo",
    name: "小米 Mimo",
    description: "小米 Mimo 大模型 — Anthropic 兼容协议",
    envVars: [
      { key: "ANTHROPIC_AUTH_TOKEN", required: true, placeholder: "mimo-...", label: "Mimo API Key" },
      { key: "ANTHROPIC_BASE_URL", required: true, defaultValue: "https://api.xiaomimimo.com/anthropic" },
      { key: "ANTHROPIC_MODEL", required: false, defaultValue: "mimo-v2.5-pro" },
      { key: "ANTHROPIC_DEFAULT_HAIKU_MODEL", required: false, defaultValue: "mimo-v2.5-pro" },
      { key: "ANTHROPIC_DEFAULT_SONNET_MODEL", required: false, defaultValue: "mimo-v2.5-pro" },
      { key: "ANTHROPIC_DEFAULT_OPUS_MODEL", required: false, defaultValue: "mimo-v2.5-pro" },
    ],
  },
  {
    id: "custom",
    name: "自定义中转",
    description: "兼容 Anthropic 协议的三方网关",
    envVars: [
      { key: "ANTHROPIC_API_KEY", required: true, placeholder: "sk-..." },
      { key: "ANTHROPIC_BASE_URL", required: true, placeholder: "https://your-gateway.com" },
      { key: "ANTHROPIC_MODEL", required: false, placeholder: "claude-sonnet-4-20250514" },
      { key: "DISABLE_AUTOUPDATER", required: false, placeholder: "1 或 0" },
    ],
  },
];

// --- Codex CLI ---

const codexProviders: ProviderPreset[] = [
  {
    id: "openai-official",
    name: "OpenAI 官方",
    description: "直接使用 OpenAI API",
    envVars: [
      { key: "OPENAI_API_KEY", required: true, placeholder: "sk-..." },
      { key: "OPENAI_BASE_URL", required: false, placeholder: "https://api.openai.com/v1" },
      { key: "OPENAI_MODEL", required: false, placeholder: "gpt-5-codex" },
    ],
  },
  {
    id: "azure-openai",
    name: "Azure OpenAI",
    description: "通过 Azure OpenAI 服务",
    envVars: [
      { key: "AZURE_OPENAI_API_KEY", required: true, placeholder: "..." },
      { key: "AZURE_OPENAI_ENDPOINT", required: true, placeholder: "https://<name>.openai.azure.com" },
      { key: "OPENAI_API_VERSION", required: false, defaultValue: "2025-03-01" },
    ],
  },
  {
    id: "openrouter",
    name: "OpenRouter",
    description: "200+ 模型统一 API",
    envVars: [
      { key: "OPENROUTER_API_KEY", required: true, placeholder: "sk-or-v1-..." },
      { key: "OPENROUTER_BASE_URL", required: false, defaultValue: "https://openrouter.ai/api/v1" },
    ],
  },
  {
    id: "deepseek",
    name: "DeepSeek",
    description: "DeepSeek V3 / R1 模型",
    envVars: [
      { key: "DEEPSEEK_API_KEY", required: true, placeholder: "sk-..." },
      { key: "DEEPSEEK_BASE_URL", required: false, defaultValue: "https://api.deepseek.com" },
    ],
  },
  {
    id: "groq",
    name: "Groq",
    description: "高速推理 LPU 引擎",
    envVars: [
      { key: "GROQ_API_KEY", required: true, placeholder: "gsk_..." },
      { key: "GROQ_BASE_URL", required: false, defaultValue: "https://api.groq.com/openai/v1" },
    ],
  },
  {
    id: "ollama",
    name: "Ollama (本地)",
    description: "本地运行的开源模型",
    envVars: [
      { key: "OLLAMA_BASE_URL", required: false, defaultValue: "http://localhost:11434/v1" },
    ],
  },
  {
    id: "custom",
    name: "自定义中转",
    description: "任何 OpenAI 兼容的三方网关",
    envVars: [
      { key: "OPENAI_API_KEY", required: true, placeholder: "sk-..." },
      { key: "OPENAI_BASE_URL", required: true, placeholder: "https://your-gateway.com/v1" },
    ],
  },
];

// --- Qwen CLI (Qoder) ---

const qoderclicnProviders: ProviderPreset[] = [
  {
    id: "alibaba-bailian",
    name: "阿里百炼 官方",
    description: "通义千问官方 API — 个人访问令牌（PAT）认证",
    note: "💡 请始终使用 PAT 令牌（环境变量），勿在 Qoder 内执行 /login。\n/login 存储的令牌会覆盖环境变量，导致多 Profile 无法正确切换。",
    envVars: [
      { key: "QODERCN_PERSONAL_ACCESS_TOKEN", required: true, placeholder: "qo-...", label: "个人访问令牌" },
    ],
  },
];

// ── Tool Definitions ───────────────────────────────────────────

export const CLI_TOOLS: CLIToolDef[] = [
  {
    id: "claude",
    name: "Claude Code",
    description: "Anthropic 协议 — 兼容 Anthropic 官方、Bedrock、Vertex 及三方网关",
    iconLetter: "C",
    providers: claudeProviders,
  },
  {
    id: "codex",
    name: "Codex CLI",
    description: "OpenAI 协议 — 兼容 OpenAI、Azure、OpenRouter、DeepSeek、Groq 等",
    iconLetter: "O",
    providers: codexProviders,
  },
  {
    id: "qoderclicn",
    name: "Qoder CLI (国内版)",
    description: "通义灵码 CLI — 仅支持阿里百炼平台，不支持第三方 API",
    iconLetter: "Q",
    providers: qoderclicnProviders,
  },
];

// ── Helpers ────────────────────────────────────────────────────

export function getToolById(id: string): CLIToolDef | undefined {
  return CLI_TOOLS.find((t) => t.id === id);
}

export function getProviderById(toolId: string, providerId: string): ProviderPreset | undefined {
  const tool = getToolById(toolId);
  if (!tool) return undefined;
  return tool.providers.find((p) => p.id === providerId);
}

/**
 * Get the complete env var list for a tool+provider combination.
 * Returns an array of [key, defaultValue] tuples ready for ProfileDialog state.
 */
export function getEnvTemplate(toolId: string, providerId: string): [string, string][] {
  const provider = getProviderById(toolId, providerId);
  if (!provider) return [];
  return provider.envVars.map((v) => [v.key, v.defaultValue ?? ""]);
}

/**
 * Get EnvVarDef metadata for a specific key in a tool+provider combination.
 * Useful for rendering hints, placeholders, and required indicators.
 */
export function getEnvVarDef(toolId: string, providerId: string, key: string): EnvVarDef | undefined {
  const provider = getProviderById(toolId, providerId);
  if (!provider) return undefined;
  return provider.envVars.find((v) => v.key === key);
}
