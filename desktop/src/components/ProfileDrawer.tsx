import React from "react";
import type { ProfileDetail, ProfileSummary } from "../lib/types";
import { EnvVarTable } from "./EnvVarTable";
import { SearchInput } from "./common/SearchInput";

interface ProfileDrawerProps {
  open: boolean;
  profiles: ProfileSummary[];
  selectedProfile: ProfileDetail | null;
  selectedName: string | null;
  searchQuery: string;
  onClose: () => void;
  onSelect: (name: string) => void;
  onSearch: (query: string) => void;
  onAdd: () => void;
  onRunInCurrentProject: (profileName: string) => void;
}

export function ProfileDrawer({
  open,
  profiles,
  selectedProfile,
  selectedName,
  searchQuery,
  onClose,
  onSelect,
  onSearch,
  onAdd,
  onRunInCurrentProject,
}: ProfileDrawerProps) {
  if (!open) return null;

  return (
    <div className="fixed inset-0 z-40 flex justify-end">
      <button
        aria-label="关闭 Profile 管理遮罩"
        className="absolute inset-0 bg-black/45"
        onClick={onClose}
      />
      <section className="relative z-10 w-[min(1080px,88vw)] h-full bg-app-bg border-l border-app-border shadow-dialog flex flex-col">
        <div className="h-[44px] shrink-0 flex items-center gap-3 px-4 border-b border-app-border bg-app-toolbar">
          <div className="text-sm font-mono text-app-text font-semibold">Profile Management</div>
          <div className="text-2xs font-mono text-app-text-muted">全局 Profiles</div>
          <button aria-label="关闭 Profile 管理" onClick={onClose} className="ml-auto text-app-text-muted hover:text-app-text">
            x
          </button>
        </div>
        <div className="flex-1 min-h-0 grid grid-cols-[300px_1fr]">
          <aside className="border-r border-app-border bg-app-sidebar flex flex-col min-h-0">
            <div className="p-2 border-b border-app-border">
              <SearchInput value={searchQuery} onChange={onSearch} placeholder="搜索 profile..." />
            </div>
            <div className="p-2 border-b border-app-border">
              <button onClick={onAdd} className="px-2 py-1 text-xs font-mono bg-app-accent text-[var(--app-bg)]">
                + 新建 Profile
              </button>
            </div>
            <div className="flex-1 overflow-y-auto py-1">
              {profiles.map((profile) => (
                <button
                  key={profile.name}
                  onClick={() => onSelect(profile.name)}
                  className={`w-full flex items-center gap-2 px-3 py-2 text-left font-mono text-xs border-l-[3px] ${
                    selectedName === profile.name
                      ? "border-l-app-accent bg-app-selected text-app-text"
                      : "border-l-transparent text-app-text-dim hover:bg-app-hover"
                  }`}
                >
                  <span className="truncate flex-1">{profile.name}</span>
                  {profile.cli_type && <span className="text-2xs text-app-text-muted">{profile.cli_type}</span>}
                  <span className="text-2xs text-app-text-muted">{profile.env_count}</span>
                </button>
              ))}
            </div>
          </aside>
          <main className="min-h-0 overflow-y-auto">
            {selectedProfile ? (
              <div className="p-5 h-full flex flex-col min-h-0">
                <div className="flex items-center gap-3 mb-4 shrink-0">
                  <div>
                    <h2 className="text-base font-mono font-semibold text-app-text">{selectedProfile.name}</h2>
                    <p className="text-xs text-app-text-muted">全局 Profile</p>
                  </div>
                  <button
                    aria-label="在当前项目运行"
                    onClick={() => onRunInCurrentProject(selectedProfile.name)}
                    className="ml-auto px-3 py-1.5 text-xs font-mono bg-app-accent text-[var(--app-bg)]"
                  >
                    在当前项目运行
                  </button>
                </div>
                <div className="flex-1 min-h-0 border border-app-border">
                  <EnvVarTable
                    env={selectedProfile.env}
                    onSet={() => Promise.resolve()}
                    onDelete={() => Promise.resolve()}
                  />
                </div>
              </div>
            ) : (
              <div className="h-full flex items-center justify-center text-xs font-mono text-app-text-muted">
                选择一个 Profile
              </div>
            )}
          </main>
        </div>
      </section>
    </div>
  );
}
