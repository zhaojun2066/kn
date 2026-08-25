import React, { useEffect, useMemo, useRef, useState } from "react";
import { ChevronDown, ChevronUp, Copy, Edit3, MessageSquareText, Plus, Settings2, Trash2, X } from "lucide-react";
import {
  MAX_CUSTOM_PROMPTS,
  buildPromptLibraryChanges,
  canAddPrompt,
  isPromptCategory,
  orderPromptsByCategory,
  promptCategories,
  promptCategoryLabels,
  systemPrompts,
  type PromptCategory,
  type PromptTemplate,
} from "../lib/promptLibrary";
import { getPromptLibrary, savePromptLibrary, type PromptLibraryState } from "../lib/tauri-api";
import { invoke } from "@tauri-apps/api/core";
import { ConfirmDialog } from "./ConfirmDialog";

const emptyDraft = (): PromptTemplate => ({
  uuid: crypto.randomUUID(), title: "", content: "", category: "other", sortOrder: 0, revision: 0,
});

type CloudSyncState = { systemPrompts: PromptTemplate[]; customPrompts: PromptTemplate[]; tombstones: { uuid: string; revision: number }[] };

function mergeCloudState(local: PromptLibraryState, remote: CloudSyncState): PromptLibraryState {
  const prompts = new Map(local.prompts.map((prompt) => [prompt.uuid, prompt]));
  for (const prompt of remote.customPrompts) {
    const current = prompts.get(prompt.uuid);
    if (!current || (prompt.revision ?? 0) >= (current.revision ?? 0)) prompts.set(prompt.uuid, prompt);
  }
  for (const tombstone of remote.tombstones) {
    const current = prompts.get(tombstone.uuid);
    if (current && (tombstone.revision ?? 0) >= (current.revision ?? 0)) prompts.set(tombstone.uuid, { ...current, revision: tombstone.revision, cloudDeletedLocallyRetained: true });
  }
  // An empty Cloud custom list is never an instruction to erase local work.
  return { ...local, prompts: [...prompts.values()], systemPrompts: remote.systemPrompts };
}

function PromptEditor({ draft, onChange, onSave, onCancel }: {
  draft: PromptTemplate; onChange: (value: PromptTemplate) => void; onSave: () => void; onCancel: () => void;
}) {
  return <div className="border border-app-border bg-[var(--app-subtle)] p-3 space-y-3">
    <input autoFocus value={draft.title} maxLength={80} placeholder="提示词名称"
      onChange={(e) => onChange({ ...draft, title: e.target.value })}
      className="w-full bg-[var(--app-input)] border border-app-border px-2.5 py-1.5 text-sm text-app-text outline-none focus:border-app-accent" />
    <select value={draft.category} onChange={(e) => isPromptCategory(e.target.value) && onChange({ ...draft, category: e.target.value })}
      className="bg-[var(--app-input)] border border-app-border px-2 py-1.5 text-xs text-app-text outline-none">
      {promptCategories.map((category) => <option key={category} value={category}>{promptCategoryLabels[category]}</option>)}
    </select>
    <textarea value={draft.content} maxLength={12000} placeholder="输入纯文本提示词"
      onChange={(e) => onChange({ ...draft, content: e.target.value })} rows={7}
      className="w-full resize-y bg-[var(--app-input)] border border-app-border p-2.5 text-sm leading-6 text-app-text outline-none focus:border-app-accent" />
    <div className="flex justify-end gap-2"><button onClick={onCancel} className="px-3 py-1.5 text-xs text-app-text-dim hover:text-app-text">取消</button><button disabled={!draft.title.trim() || !draft.content.trim()} onClick={onSave} className="px-3 py-1.5 text-xs bg-app-accent text-white disabled:opacity-40">保存</button></div>
  </div>;
}

export function PromptLibraryDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const [state, setState] = useState<PromptLibraryState>({ syncEnabled: false, prompts: [] });
  const [draft, setDraft] = useState<PromptTemplate | null>(null);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [remoteSystemPrompts, setRemoteSystemPrompts] = useState<readonly PromptTemplate[]>(systemPrompts);
  const [customExpanded, setCustomExpanded] = useState(true);
  const [systemExpanded, setSystemExpanded] = useState(true);
  const [deleteTarget, setDeleteTarget] = useState<PromptTemplate | null>(null);
  const [conflictDraft, setConflictDraft] = useState<PromptTemplate | null>(null);

  useEffect(() => {
    if (!open) return;
    setLoading(true); setError(null); setNotice(null); setDraft(null); setConflictDraft(null);
    getPromptLibrary().then(async (local) => {
      // A second Desktop treats Cloud as the source of truth once sync has
      // been enabled. This avoids uploading an empty local cache over it.
      try {
          const remote = await invoke<CloudSyncState>("agent_ipc", { method: "get_prompt_library_sync_state", params: {} });
          if (remote && Array.isArray(remote.customPrompts)) {
            // System data is authoritative, including an intentionally empty list.
            setRemoteSystemPrompts(remote.systemPrompts);
            const next = mergeCloudState(local, remote);
            setState(await savePromptLibrary(next));
            return;
          }
        } catch (e) {
          // Local data remains available offline; syncing again will reconcile it.
          setError(`Cloud 提示词暂时不可用：${String(e)}`);
        }
      setState(local);
    }).catch((e) => setError(String(e))).finally(() => setLoading(false));
  }, [open]);

  const persist = async (next: PromptLibraryState) => {
    setSaving(true); setError(null);
    try {
      // The remote mutation happens first. In particular, a failed deletion
      // leaves the switch on so the UI never claims Cloud data was removed.
      let finalState = next;
      let newlyEnabledLocalUuids: Set<string> | null = null;
      if (!state.syncEnabled && next.syncEnabled) {
        // First enable must not erase a library created on another Desktop.
        // Merge by UUID, with this explicit user action winning collisions.
        const remote = await invoke<CloudSyncState>("agent_ipc", { method: "get_prompt_library_sync_state", params: {} });
        const merged = new Map(remote.customPrompts.map((prompt) => [prompt.uuid, prompt]));
        const enabledLocalUuids = new Set<string>();
        newlyEnabledLocalUuids = enabledLocalUuids;
        // Local items might have been tombstoned by a previous "disable sync".
        // Assign them new IDs; Cloud entries retain their identity and win any
        // UUID collision from an older local cache.
        next.prompts.forEach((prompt) => {
          const uuid = crypto.randomUUID();
          enabledLocalUuids.add(uuid);
          merged.set(uuid, { ...prompt, uuid, cloudDeletedLocallyRetained: false, revision: 0 });
        });
        if (merged.size > MAX_CUSTOM_PROMPTS) {
          throw new Error(`合并后的自定义提示词超过 ${MAX_CUSTOM_PROMPTS} 条，请先整理 Cloud 提示词库`);
        }
        // Disabling sync leaves Cloud tombstones behind so stale devices cannot
        // resurrect old data. Re-enabling deliberately creates fresh records.
        finalState = { ...next, locallyDisabledUuids: [], prompts: [...merged.values()].map((prompt, sortOrder) => ({ ...prompt, sortOrder })) };
      }
      if (state.syncEnabled && !finalState.syncEnabled) {
        await invoke("agent_ipc", { method: "delete_prompt_library", params: {} });
        finalState = { ...finalState, locallyDisabledUuids: state.prompts.map((prompt) => prompt.uuid) };
        setNotice("已关闭同步：Cloud 中的自定义提示词已删除，本机内容已保留。");
      } else if (finalState.syncEnabled) {
        // When enabling from an off state every local UUID is intentionally
        // fresh.  The old cache may already be tombstoned by the preceding
        // disable, so never send delete operations for it.
        // Cloud records merged during enabling already exist remotely. Only
        // publish the local records created above; do not update or delete
        // either the old local cache or another device's Cloud records.
        const operations = newlyEnabledLocalUuids
          ? buildPromptLibraryChanges([], finalState.prompts.filter((prompt) => newlyEnabledLocalUuids!.has(prompt.uuid)), false)
          : buildPromptLibraryChanges(state.prompts, finalState.prompts);
        if (operations.length) {
          const result = await invoke<{ results: { uuid: string; status: string; prompt?: PromptTemplate; message?: string }[] }>("agent_ipc", { method: "change_prompt_library", params: { operations } });
          const prompts = new Map(finalState.prompts.map((prompt) => [prompt.uuid, prompt]));
          for (const item of result.results) {
            if (item.status === "applied" && item.prompt) prompts.set(item.uuid, item.prompt);
            else if (item.status === "deleted") prompts.delete(item.uuid);
            else if (item.status === "conflict" && item.prompt) { const stale = prompts.get(item.uuid); if (stale) setConflictDraft(stale); prompts.set(item.uuid, item.prompt); setError(`“${item.prompt.title}”已在其他设备修改，已刷新为最新版本，请重新编辑。`); }
            else if (item.status === "conflict") { const local = prompts.get(item.uuid); if (local) prompts.set(item.uuid, { ...local, cloudDeletedLocallyRetained: true }); setError("该提示词已在其他设备删除，已保留为仅本机内容。"); }
            else if (item.status !== "applied") throw new Error(item.message ?? "提示词同步失败");
          }
          finalState = { ...finalState, prompts: [...prompts.values()] };
        }
      }
      setState(await savePromptLibrary(finalState));
    }
    catch (e) { setError(String(e)); }
    finally { setSaving(false); }
  };
  const movePrompt = (uuid: string, direction: -1 | 1) => {
    const index = state.prompts.findIndex((prompt) => prompt.uuid === uuid);
    const target = index + direction;
    if (index < 0 || target < 0 || target >= state.prompts.length) return;
    const prompts = [...state.prompts];
    [prompts[index], prompts[target]] = [prompts[target], prompts[index]];
    void persist({ ...state, prompts: prompts.map((prompt, sortOrder) => ({ ...prompt, sortOrder })) });
  };
  if (!open) return null;
  return <div className="fixed inset-0 z-[130] flex items-center justify-center app-dialog-backdrop" onClick={onClose}>
    <div className="app-dialog-panel bg-app-panel border border-app-border w-[760px] max-w-[calc(100vw-3rem)] max-h-[86vh] flex flex-col" onClick={(e) => e.stopPropagation()}>
      <header className="px-4 py-3 border-b border-app-border flex items-center justify-between"><div className="flex items-center gap-2"><Settings2 size={16} className="text-app-accent"/><div><h2 className="text-sm font-semibold text-app-text">常用提示词</h2><p className="text-2xs text-app-text-muted mt-0.5">终端选择后只填入输入框，不会自动发送</p></div></div><button onClick={onClose} className="text-app-text-dim hover:text-app-text"><X size={16}/></button></header>
      <div className="px-4 py-3 border-b border-app-border flex items-center justify-between gap-4"><div><p className="text-sm text-app-text font-medium">同步到 Cloud</p><p className="text-2xs text-app-text-muted mt-0.5">开启后可在 iOS 使用；关闭时会删除 Cloud 中的自定义提示词。</p></div><button aria-label="同步到 Cloud" disabled={saving} onClick={() => persist({ ...state, syncEnabled: !state.syncEnabled })} className={`relative w-9 h-5 rounded-full ${state.syncEnabled ? "bg-app-accent" : "bg-app-border"}`}><span className={`absolute top-0.5 w-4 h-4 rounded-full bg-app-bg transition-all ${state.syncEnabled ? "left-4" : "left-0.5"}`}/></button></div>
      <main className="p-4 overflow-hidden space-y-3 flex flex-col min-h-0">{notice && <p className="text-xs text-app-green shrink-0">{notice}</p>}{error && <div className="flex items-center justify-between gap-2 text-xs text-app-red shrink-0"><p>{error}</p>{conflictDraft && <button type="button" onClick={() => void navigator.clipboard.writeText(conflictDraft.content)} className="shrink-0 text-app-accent hover:underline">复制未保存内容</button>}</div>}{loading ? <p className="text-sm text-app-text-muted">正在加载提示词…</p> : <>
        <section className="flex flex-col min-h-0 shrink"><div className="flex items-center justify-between shrink-0"><button onClick={() => setCustomExpanded((value) => !value)} className="flex items-center gap-2 text-sm text-app-text font-medium"><ChevronDown size={15} className={customExpanded ? "" : "-rotate-90"}/>我的提示词 <span className="text-app-text-muted font-normal">{state.prompts.length}/{MAX_CUSTOM_PROMPTS}</span></button><button disabled={!canAddPrompt(state.prompts.length) || saving} onClick={() => { setCustomExpanded(true); setDraft(emptyDraft()); }} className="flex items-center gap-1 px-2.5 py-1.5 text-xs bg-app-accent text-white disabled:opacity-40"><Plus size={13}/>新建</button></div>{customExpanded && <div className="mt-2 max-h-[260px] overflow-y-auto space-y-2 pr-1">{draft && <PromptEditor draft={draft} onChange={setDraft} onCancel={() => setDraft(null)} onSave={async () => { const existing = state.prompts.findIndex((p) => p.uuid === draft.uuid); const prompts = existing < 0 ? [...state.prompts, { ...draft, sortOrder: state.prompts.length }] : state.prompts.map((p, index) => index === existing ? draft : p); await persist({ ...state, prompts }); setDraft(null); }}/>} 
        {state.prompts.length === 0 && !draft ? <p className="border border-dashed border-app-border p-5 text-center text-xs text-app-text-muted">还没有自定义提示词。可复制系统预置后再按需修改。</p> : state.prompts.map((prompt, index) => <div key={prompt.uuid} className="border border-app-border px-3 py-2 flex gap-3"><div className="min-w-0 flex-1"><div className="flex gap-2 items-center"><span className="text-sm text-app-text truncate">{prompt.title}</span><span className="text-2xs text-app-text-muted">{promptCategoryLabels[prompt.category]}</span>{prompt.cloudDeletedLocallyRetained && <span className="text-2xs text-app-amber">仅本机保留</span>}</div><p className="text-2xs text-app-text-muted mt-1 line-clamp-2 whitespace-pre-wrap">{prompt.content}</p></div><div className="flex flex-col justify-center"><button aria-label="上移" disabled={saving || index === 0} onClick={() => movePrompt(prompt.uuid, -1)} className="text-app-text-dim hover:text-app-text disabled:opacity-30"><ChevronUp size={13}/></button><button aria-label="下移" disabled={saving || index === state.prompts.length - 1} onClick={() => movePrompt(prompt.uuid, 1)} className="text-app-text-dim hover:text-app-text disabled:opacity-30"><ChevronDown size={13}/></button></div><button onClick={() => setDraft(prompt)} className="text-app-text-dim hover:text-app-text"><Edit3 size={14}/></button><button title="删除提示词" onClick={() => setDeleteTarget(prompt)} className="text-app-text-dim hover:text-app-red"><Trash2 size={14}/></button></div>)}</div>}</section>
        <section className="flex flex-col min-h-0 shrink"><button onClick={() => setSystemExpanded((value) => !value)} className="flex items-center gap-2 text-sm text-app-text font-medium shrink-0"><ChevronDown size={15} className={systemExpanded ? "" : "-rotate-90"}/>系统预置 <span className="text-app-text-muted font-normal">{remoteSystemPrompts.length}</span></button>{systemExpanded && <div className="mt-2 max-h-[260px] overflow-y-auto space-y-2 pr-1">{remoteSystemPrompts.map((prompt) => <div key={prompt.uuid} className="border border-app-border px-3 py-2 flex gap-3 opacity-90"><div className="min-w-0 flex-1"><div className="flex gap-2"><span className="text-sm text-app-text">{prompt.title}</span><span className="text-2xs text-app-text-muted">{promptCategoryLabels[prompt.category]}</span></div><p className="text-2xs text-app-text-muted mt-1 line-clamp-2">{prompt.content}</p></div><button disabled={!canAddPrompt(state.prompts.length)} title="复制为自定义" onClick={() => { setCustomExpanded(true); setDraft({ ...prompt, uuid: crypto.randomUUID(), system: undefined, title: `${prompt.title}（副本）`, sortOrder: state.prompts.length }); }} className="text-app-text-dim hover:text-app-text disabled:opacity-40"><Copy size={14}/></button></div>)}</div>}</section>
      </>}</main>
      <ConfirmDialog open={deleteTarget !== null} title="删除提示词" message={`确定删除“${deleteTarget?.title ?? ""}”吗？${state.syncEnabled ? "此操作会同步到 Cloud。" : "此操作只会从本机删除。"}`} confirmLabel="删除" loading={saving} onCancel={() => setDeleteTarget(null)} onConfirm={async () => { if (!deleteTarget) return; await persist({ ...state, prompts: state.prompts.filter((prompt) => prompt.uuid !== deleteTarget.uuid).map((prompt, sortOrder) => ({ ...prompt, sortOrder })) }); setDeleteTarget(null); }} />
    </div>
  </div>;
}

export function PromptPicker({ disabled, mode, onInsert }: { disabled?: boolean; mode: "right" | "bottom"; onInsert: (content: string) => void }) {
  const [open, setOpen] = useState(false); const [state, setState] = useState<PromptLibraryState>({ syncEnabled: false, prompts: [] }); const [search, setSearch] = useState(""); const [catalog, setCatalog] = useState<readonly PromptTemplate[]>(systemPrompts); const [selectedIndex, setSelectedIndex] = useState(0); const rootRef = useRef<HTMLDivElement>(null);
  useEffect(() => { if (open) { getPromptLibrary().then((local) => { setState(local); invoke<CloudSyncState>("agent_ipc", { method: "get_prompt_library_sync_state", params: {} }).then((remote) => { if (Array.isArray(remote.customPrompts)) { setCatalog(remote.systemPrompts); setState(mergeCloudState(local, remote)); } }).catch(() => setCatalog(local.systemPrompts ?? systemPrompts)); }).catch(() => {}); } }, [open]);
  useEffect(() => {
    const openForTerminal = (event: Event) => {
      const target = (event as CustomEvent<"right" | "bottom">).detail;
      if (target === mode && !disabled) { setSelectedIndex(0); setOpen(true); }
    };
    window.addEventListener("kn-open-prompt-picker", openForTerminal);
    return () => window.removeEventListener("kn-open-prompt-picker", openForTerminal);
  }, [disabled, mode]);
  useEffect(() => {
    if (!open) return;
    const closeOutside = (event: MouseEvent) => { if (rootRef.current && !rootRef.current.contains(event.target as Node)) setOpen(false); };
    document.addEventListener("mousedown", closeOutside);
    return () => document.removeEventListener("mousedown", closeOutside);
  }, [open]);
  const prompts = useMemo(() => [...catalog, ...state.prompts].filter((p) => `${p.title}\n${p.content}`.toLowerCase().includes(search.toLowerCase())), [catalog, state.prompts, search]);
  // The keyboard index must use the exact same category-ordered list as rendering.
  const displayPrompts = useMemo(() => orderPromptsByCategory(prompts), [prompts]);
  const choose = (prompt: PromptTemplate) => { onInsert(prompt.content); setOpen(false); };
  const onKeyDown = (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "ArrowDown") { event.preventDefault(); setSelectedIndex((value) => Math.min(value + 1, Math.max(0, displayPrompts.length - 1))); }
    else if (event.key === "ArrowUp") { event.preventDefault(); setSelectedIndex((value) => Math.max(0, value - 1)); }
    else if (event.key === "Enter" && displayPrompts[selectedIndex]) { event.preventDefault(); choose(displayPrompts[selectedIndex]); }
    else if (event.key === "Escape") { event.preventDefault(); setOpen(false); }
  };
  return <div ref={rootRef} className="relative shrink-0"><button disabled={disabled} onClick={() => { setSelectedIndex(0); setOpen((value) => !value); }} title="常用提示词（⌘⇧L）" className="px-2 h-[32px] text-app-text-muted hover:text-app-text hover:bg-[var(--app-hover)] disabled:opacity-35"><MessageSquareText size={14}/></button>{open && <div className="absolute right-0 top-[36px] z-50 w-[360px] max-w-[calc(100vw-2rem)] border border-app-border bg-app-panel shadow-xl p-2"><div className="flex gap-1 mb-2"><input autoFocus value={search} onKeyDown={onKeyDown} onChange={(e) => { setSearch(e.target.value); setSelectedIndex(0); }} placeholder="搜索提示词（↑↓ 选择，↵ 填入）" className="min-w-0 flex-1 bg-[var(--app-input)] border border-app-border px-2 py-1.5 text-xs outline-none"/><button onClick={() => setOpen(false)} title="关闭（Esc）" className="px-1.5 text-app-text-dim hover:text-app-text"><X size={14}/></button></div>{promptCategories.map((category) => { const group = displayPrompts.filter((p) => p.category === category); return group.length ? <section key={category} className="mb-2"><p className="px-1 pb-1 text-2xs text-app-text-muted">{promptCategoryLabels[category]}</p>{group.map((prompt) => { const currentIndex = displayPrompts.indexOf(prompt); return <button key={prompt.uuid} title={prompt.content} onMouseEnter={() => setSelectedIndex(currentIndex)} onClick={() => choose(prompt)} className={`block w-full text-left px-2 py-1.5 ${selectedIndex === currentIndex ? "bg-[var(--app-hover)]" : "hover:bg-[var(--app-hover)]"}`}><span className="block text-xs text-app-text">{prompt.title}</span><span className="block mt-0.5 text-2xs text-app-text-muted truncate">{prompt.content}</span></button>; })}</section> : null; })}</div>}</div>;
}
