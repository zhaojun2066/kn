import { Bot, Lock, Shield, Wrench } from "lucide-react";
import type { AgentEntry } from "./SkillManager";

interface AgentDetailProps {
  agent: AgentEntry;
  onToggle?: (agent: AgentEntry, enabled: boolean) => void;
  onDelete?: (agent: AgentEntry) => void;
}

export function AgentDetail({ agent, onToggle, onDelete }: AgentDetailProps) {
  const isBuiltin = agent.source === "builtin";

  return (
    <div className="flex flex-col h-full animate-fadeIn">
      {/* Hero */}
      <div className="flex items-center gap-3 px-4 py-6 border-b border-[var(--app-border)]">
        <div className="w-10 h-10 rounded-lg bg-[var(--app-accent)]/10 flex items-center justify-center">
          <Bot size={20} className="text-[var(--app-accent)]" />
        </div>
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <h2 className="text-sm font-semibold text-[var(--app-text)] font-mono truncate">
              {agent.name}
            </h2>
            {isBuiltin && (
              <span className="text-2xs px-1.5 py-0.5 rounded bg-[var(--app-border)] text-[var(--app-text-dim)] font-mono">
                Built-in
              </span>
            )}
          </div>
          {agent.description && (
            <p className="text-xs text-[var(--app-text-dim)] mt-1 line-clamp-2">
              {agent.description}
            </p>
          )}
        </div>
      </div>

      {/* Metadata */}
      <div className="flex-1 overflow-y-auto px-4 py-4 space-y-4">
        {/* Status */}
        <MetaSection title="Status">
          <MetaRow label="CLI" value={agent.cli} />
          <MetaRow label="Source" value={agent.source} />
          <MetaRow
            label="State"
            value={agent.enabled ? "Enabled" : "Disabled"}
          />
          {agent.model && <MetaRow label="Model" value={agent.model} />}
          {agent.color && (
            <MetaRow label="Color">
              <span className="flex items-center gap-1.5">
                <span
                  className="inline-block w-3 h-3 rounded-full border border-white/20"
                  style={{ backgroundColor: agent.color }}
                />
                {agent.color}
              </span>
            </MetaRow>
          )}
        </MetaSection>

        {/* Tools */}
        {agent.tools.length > 0 && (
          <MetaSection title="Tools" icon={<Wrench size={12} />}>
            <div className="flex flex-wrap gap-1">
              {agent.tools.map((tool) => (
                <span
                  key={tool}
                  className="text-2xs px-1.5 py-0.5 rounded bg-[var(--app-accent)]/10 text-[var(--app-accent)] font-mono"
                >
                  {tool}
                </span>
              ))}
            </div>
          </MetaSection>
        )}

        {/* Referenced Skills */}
        {agent.skills.length > 0 && (
          <MetaSection title="Referenced Skills">
            <div className="flex flex-wrap gap-1">
              {agent.skills.map((skill) => (
                <span
                  key={skill}
                  className="text-2xs px-1.5 py-0.5 rounded bg-purple-500/10 text-purple-400 font-mono"
                >
                  {skill}
                </span>
              ))}
            </div>
          </MetaSection>
        )}

        {/* Path (only for file-based agents) */}
        {agent.path && (
          <MetaSection title="Location">
            <div className="text-2xs text-[var(--app-text-dim)] font-mono break-all">
              {agent.path}
            </div>
          </MetaSection>
        )}

        {/* Codex sandbox */}
        {agent.sandboxMode && (
          <MetaSection title="Sandbox" icon={<Shield size={12} />}>
            <MetaRow label="Mode" value={agent.sandboxMode} />
          </MetaSection>
        )}
      </div>

      {/* Actions (hidden for builtin) */}
      {!isBuiltin && (
        <div className="px-4 py-3 border-t border-[var(--app-border)] space-y-2">
          <button
            onClick={() => onToggle?.(agent, !agent.enabled)}
            className="w-full px-3 py-1.5 rounded text-xs font-mono transition-colors bg-[var(--app-accent)]/10 hover:bg-[var(--app-accent)]/20 text-[var(--app-accent)]"
          >
            {agent.enabled ? "Disable Agent" : "Enable Agent"}
          </button>
          <button
            onClick={() => onDelete?.(agent)}
            className="w-full px-3 py-1.5 rounded text-xs font-mono transition-colors bg-red-500/10 hover:bg-red-500/20 text-red-400"
          >
            Delete Agent
          </button>
        </div>
      )}

      {/* Builtin notice */}
      {isBuiltin && (
        <div className="px-4 py-3 border-t border-[var(--app-border)]">
          <div className="flex items-center gap-2 text-2xs text-[var(--app-text-dim)]">
            <Lock size={10} />
            <span>System built-in agent — cannot be modified</span>
          </div>
        </div>
      )}
    </div>
  );
}

// ── Sub-components ──

function MetaSection({
  title,
  icon,
  children,
}: {
  title: string;
  icon?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div>
      <div className="flex items-center gap-1.5 mb-2">
        {icon}
        <span className="text-2xs font-semibold text-[var(--app-text-dim)] uppercase tracking-wider">
          {title}
        </span>
      </div>
      <div className="space-y-1">{children}</div>
    </div>
  );
}

function MetaRow({
  label,
  value,
  children,
}: {
  label: string;
  value?: string;
  children?: React.ReactNode;
}) {
  return (
    <div className="flex items-center justify-between text-2xs">
      <span className="text-[var(--app-text-dim)]">{label}</span>
      {children || (
        <span className="text-[var(--app-text)] font-mono">{value}</span>
      )}
    </div>
  );
}
