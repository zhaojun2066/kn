export type ApprovalMode = "disabled" | "nativePermission" | "preToolUse";

export interface RemoteApprovalConfig {
  enabled: boolean;
  claudeMode: ApprovalMode;
  codexMode: ApprovalMode;
  qoderCnMode: ApprovalMode;
  rules: {
    destructiveFilesystem: boolean;
    forceGit: boolean;
    deployPublish: boolean;
    projectExternalWrite: boolean;
    credentialsSecurity: boolean;
  };
}

export const DEFAULT_REMOTE_APPROVAL_CONFIG: RemoteApprovalConfig = {
  enabled: false,
  claudeMode: "nativePermission",
  codexMode: "nativePermission",
  qoderCnMode: "preToolUse",
  rules: {
    destructiveFilesystem: true,
    forceGit: true,
    deployPublish: true,
    projectExternalWrite: true,
    credentialsSecurity: true,
  },
};

export const REMOTE_APPROVAL_CLI_CAPABILITIES = [
  {
    id: "claudeMode" as const,
    name: "Claude Code",
    modes: ["disabled", "nativePermission", "preToolUse"] as ApprovalMode[],
    detail: "原生权限或 kn 高风险拦截；每个会话只能选择一种模式。",
  },
  {
    id: "codexMode" as const,
    name: "Codex CLI",
    modes: ["disabled", "nativePermission", "preToolUse"] as ApprovalMode[],
    detail: "原生权限或 kn 高风险拦截；每个会话只能选择一种模式。",
  },
  {
    id: "qoderCnMode" as const,
    name: "Qoder CLI CN",
    modes: ["disabled", "preToolUse"] as ApprovalMode[],
    detail: "仅支持 kn 高风险拦截，不适配 Qoder 国际版。",
  },
] as const;

export const REMOTE_APPROVAL_RULES = [
  { id: "destructiveFilesystem" as const, label: "破坏性文件操作", detail: "Shell：明确的删除、清空、格式化或覆盖命令。" },
  { id: "forceGit" as const, label: "强制 Git 操作", detail: "Shell：force push、reset --hard、clean -f。" },
  { id: "deployPublish" as const, label: "发布与部署", detail: "Shell：明确的 deploy、publish、terraform/pulumi/kubectl apply。" },
  { id: "projectExternalWrite" as const, label: "项目外写入", detail: "结构化写文件工具：绝对路径不在当前项目内。" },
  { id: "credentialsSecurity" as const, label: "凭证与安全配置", detail: "结构化 .env、SSH、密钥和认证相关目标。" },
] as const;

export const APPROVAL_MODE_LABEL: Record<ApprovalMode, string> = {
  disabled: "关闭",
  nativePermission: "CLI 原生权限",
  preToolUse: "kn 高风险拦截",
};
