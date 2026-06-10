import type { ProfileSummary } from "../lib/types";

interface UsageStatus {
  todayTokens: number;
  loading: boolean;
}

interface StatusBarProps {
  loading: boolean;
  profiles: ProfileSummary[];
  terminalOpen: boolean;
  colorScheme: string;
  usage: UsageStatus;
  selectedName: string | null;
  defaultProfile: string | null;
  appVersion: string;
  onShowUsage: () => void;
}

export function StatusBar({
  loading,
  profiles,
  terminalOpen,
  colorScheme,
  usage,
  selectedName,
  defaultProfile,
  appVersion,
  onShowUsage,
}: StatusBarProps) {
  return (
    <div className="flex items-center h-[26px] px-3 bg-app-statusbar border-t border-app-border select-none shrink-0 gap-3">
      <span className="text-2xs text-app-text-muted font-mono shrink-0">
        {loading ? "..." : profiles.length > 0 ? `${profiles.length} 个 profile` : "就绪"}
      </span>
      {terminalOpen ? (
        <span className="text-2xs text-app-accent font-mono shrink-0 flex items-center gap-1">
          <span className="w-1.5 h-1.5 rounded-full bg-app-accent" style={{ boxShadow: "0 0 4px var(--app-glow)" }} />
          终端
        </span>
      ) : (
        <span className="text-2xs text-app-text-muted font-mono shrink-0 flex items-center gap-1 opacity-50">
          <span className="w-1.5 h-1.5 rounded-full bg-app-text-muted" />
          终端
        </span>
      )}
      <span className="flex-1" />
      <span className="text-2xs text-app-text-muted font-mono shrink-0">
        {colorScheme ? colorScheme.charAt(0).toUpperCase() + colorScheme.slice(1) : ""}
      </span>
      {(usage.todayTokens > 0 || !usage.loading) && (
        <span
          className={`text-2xs font-mono shrink-0 cursor-pointer transition-colors ${
            usage.todayTokens > 0
              ? "text-app-amber hover:text-app-amber-glow"
              : "text-app-text-dim hover:text-app-text-muted"
          }`}
          onClick={onShowUsage}
          title="查看 Token 用量"
        >
          ◉ {usage.todayTokens >= 1000 ? `${(usage.todayTokens / 1000).toFixed(1)}K` : usage.todayTokens}
        </span>
      )}
      <span className="text-2xs text-app-text-muted font-mono shrink-0">
        {selectedName ? (
          <>
            <span className="text-app-text-dim">{selectedName}</span>
            {defaultProfile === selectedName && (
              <span className="text-app-accent ml-1.5">(默认)</span>
            )}
          </>
        ) : "--"}
      </span>
      {appVersion && (
        <span className="text-2xs text-app-text-dim font-mono shrink-0 pl-3 border-l border-app-border">
          v{appVersion}
        </span>
      )}
    </div>
  );
}
