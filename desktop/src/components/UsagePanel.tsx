import React, { useState, useEffect, useCallback } from "react";
import { X, BarChart3, ChevronDown, ChevronRight, Plus, Trash2 } from "lucide-react";
import { useUsage } from "../hooks/useUsage";
import { getPricing, setPricing, type ModelPricing } from "../lib/tauri-api";

type Period = "today" | "week" | "month";

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

function formatCost(n: number, currency: string): string {
  const symbol = currency === "CNY" ? "¥" : "$";
  if (n < 0.01) return `${symbol}<0.01`;
  return `${symbol}${n.toFixed(2)}`;
}

interface UsagePanelProps {
  open: boolean;
  onClose: () => void;
}

export function UsagePanel({ open, onClose }: UsagePanelProps) {
  const { summary, daily, todayTokens, loading, refresh } = useUsage();
  const [period, setPeriod] = useState<Period>("week");
  const [showPricing, setShowPricing] = useState(false);

  // ── Pricing editor state ─────────────────────────────────
  const [pricing, setPricingState] = useState<Record<string, ModelPricing>>({});
  const [pricingLoaded, setPricingLoaded] = useState(false);
  const [editedPricing, setEditedPricing] = useState<Record<string, ModelPricing>>({});
  const [newModel, setNewModel] = useState("");
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (open && showPricing && !pricingLoaded) {
      getPricing().then((p) => {
        setPricingState(p);
        setEditedPricing(JSON.parse(JSON.stringify(p)));
        setPricingLoaded(true);
      }).catch(() => {});
    }
  }, [open, showPricing, pricingLoaded]);

  const updateModel = useCallback((oldName: string, field: string, value: string | number) => {
    setEditedPricing((prev) => {
      const next = { ...prev };
      if (field === "model") {
        const newName = String(value);
        if (newName !== oldName && next[newName]) return prev; // name taken
        const data = { ...next[oldName] };
        delete next[oldName];
        next[newName] = data;
      } else {
        next[oldName] = { ...next[oldName], [field]: field === "currency" ? value : Number(value) };
      }
      return next;
    });
  }, []);

  const addModel = useCallback(() => {
    const name = newModel.trim();
    if (!name || editedPricing[name]) return;
    setEditedPricing((prev) => ({
      ...prev,
      [name]: { input: 1, output: 5, currency: "USD" },
    }));
    setNewModel("");
  }, [newModel, editedPricing]);

  const removeModel = useCallback((name: string) => {
    setEditedPricing((prev) => {
      const next = { ...prev };
      delete next[name];
      return next;
    });
  }, []);

  const savePricing = useCallback(async () => {
    setSaving(true);
    try {
      for (const [model, p] of Object.entries(editedPricing)) {
        await setPricing(model, p);
      }
      setPricingState(editedPricing);
    } catch { /* */ }
    setSaving(false);
  }, [editedPricing]);

  const resetPricing = useCallback(() => {
    setEditedPricing(JSON.parse(JSON.stringify(pricing)));
  }, [pricing]);

  if (!open) return null;

  const hasData = summary && (summary.total_tokens_in + summary.total_tokens_out) > 0;
  const pricingDirty = JSON.stringify(editedPricing) !== JSON.stringify(pricing);

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
      onClick={onClose}
    >
      <div
        className="bg-app-panel border border-app-border shadow-dialog w-[560px] max-h-[85vh] overflow-y-auto select-none animate-[scaleIn_150ms_ease-out]"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-app-border sticky top-0 bg-app-panel z-10">
          <div className="flex items-center gap-2">
            <BarChart3 size={15} className="text-app-accent" />
            <span className="text-sm font-mono text-app-text font-semibold">Token 用量</span>
          </div>
          <button onClick={onClose} className="p-0.5 text-app-text-dim hover:text-app-text transition-colors">
            <X size={14} />
          </button>
        </div>

        {/* Body */}
        <div className="px-4 py-4 space-y-5">
          {/* Period tabs */}
          <div className="flex gap-1 bg-[var(--app-cmd-bg)] border border-app-border p-0.5 w-fit">
            {([["today", "今天"], ["week", "近 7 天"], ["month", "近 30 天"]] as const).map(([k, label]) => (
              <button
                key={k}
                onClick={() => setPeriod(k as Period)}
                className={`px-3 py-1 text-xs font-mono transition-colors ${
                  period === k
                    ? "bg-app-accent text-[var(--app-bg)]"
                    : "text-app-text-dim hover:text-app-text"
                }`}
              >
                {label}
              </button>
            ))}
          </div>

          {hasData ? (
            <>
              {/* Summary cards */}
              <div className="grid grid-cols-2 gap-3">
                <div className="border border-app-border bg-[var(--app-cmd-bg)] px-4 py-3 text-center">
                  <div className="text-2xs text-app-text-muted font-mono uppercase tracking-wider mb-1">
                    Token 消耗
                  </div>
                  <div className="text-lg font-mono font-bold text-app-text">
                    {formatTokens(summary.total_tokens_in + summary.total_tokens_out)}
                  </div>
                  <div className="text-2xs text-app-text-muted font-mono mt-0.5">
                    入 {formatTokens(summary.total_tokens_in)} · 出 {formatTokens(summary.total_tokens_out)}
                  </div>
                </div>
                <div className="border border-app-border bg-[var(--app-cmd-bg)] px-4 py-3 text-center">
                  <div className="text-2xs text-app-text-muted font-mono uppercase tracking-wider mb-1">
                    预估费用
                  </div>
                  <div className="text-lg font-mono font-bold text-app-amber">
                    {formatCost(summary.total_cost, summary.currency)}
                  </div>
                  <div className="text-2xs text-app-text-muted font-mono mt-0.5">
                    {summary.currency}
                  </div>
                </div>
              </div>

              {/* Per-profile breakdown */}
              {summary.by_profile.length > 0 && (
                <div className="space-y-2">
                  <div className="text-2xs text-app-text-muted font-mono uppercase tracking-wider">
                    按 Profile 拆分
                  </div>
                  <div className="space-y-1.5">
                    {summary.by_profile.map((p) => (
                      <div key={p.profile} className="flex items-center gap-2">
                        <span className="text-xs text-app-text font-mono w-24 truncate shrink-0">
                          {p.profile}
                        </span>
                        <div className="flex-1 h-3 bg-[var(--app-cmd-bg)] border border-app-border relative">
                          <div
                            className="absolute inset-y-0 left-0 bg-app-accent/30 border-r border-app-accent/50 transition-all duration-300"
                            style={{ width: `${Math.max(p.percentage, 2)}%` }}
                          />
                        </div>
                        <span className="text-2xs text-app-text-dim font-mono w-10 text-right shrink-0">
                          {p.percentage.toFixed(0)}%
                        </span>
                        <span className="text-2xs text-app-amber font-mono w-16 text-right shrink-0">
                          {formatCost(p.cost, p.currency)}
                        </span>
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {/* Daily trend bar chart */}
              {daily.length > 0 && (
                <div className="space-y-2">
                  <div className="text-2xs text-app-text-muted font-mono uppercase tracking-wider">
                    近 7 天趋势
                  </div>
                  <div className="flex items-end gap-1 h-24 px-1">
                    {daily.slice(-7).map((d, i) => {
                      const maxVal = Math.max(...daily.map((x) => x.tokens_in + x.tokens_out), 1);
                      const h = ((d.tokens_in + d.tokens_out) / maxVal) * 100;
                      return (
                        <div key={i} className="flex-1 flex flex-col items-center gap-1 h-full justify-end">
                          <span className="text-2xs text-app-text-muted font-mono">
                            {formatTokens(d.tokens_in + d.tokens_out)}
                          </span>
                          <div
                            className="w-full bg-app-accent/40 hover:bg-app-accent/60 transition-colors min-h-[2px]"
                            style={{ height: `${Math.max(h, 2)}%` }}
                            title={`${d.date}: ${d.tokens_in + d.tokens_out} tokens`}
                          />
                          <span className="text-2xs text-app-text-muted font-mono">{d.date}</span>
                        </div>
                      );
                    })}
                  </div>
                </div>
              )}
            </>
          ) : (
            <div className="text-center py-8 text-sm text-app-text-muted font-mono">
              {loading ? "加载中..." : "暂无用量数据"}
              <br />
              <span className="text-2xs text-app-text-dim mt-1 block">
                在设置中开启 Token 用量追踪，使用 AI CLI 后数据自动记录
              </span>
            </div>
          )}

          {/* ── Pricing editor ────────────────────────────────── */}
          <div className="border-t border-app-border pt-4">
            <button
              onClick={() => setShowPricing(!showPricing)}
              className="flex items-center gap-2 text-xs text-app-text-dim hover:text-app-text font-mono transition-colors w-full text-left"
            >
              {showPricing ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
              定价设置（每百万 Token）
            </button>

            {showPricing && (
              <div className="mt-3 space-y-3">
                {/* Table header */}
                <div className="grid grid-cols-[2fr_1fr_1fr_1fr_auto] gap-1.5 items-center">
                  <span className="text-2xs text-app-text-muted font-mono uppercase">模型</span>
                  <span className="text-2xs text-app-text-muted font-mono text-center uppercase">输入</span>
                  <span className="text-2xs text-app-text-muted font-mono text-center uppercase">输出</span>
                  <span className="text-2xs text-app-text-muted font-mono text-center uppercase">币种</span>
                  <span className="w-5" />
                </div>

                {/* Editable rows */}
                {Object.entries(editedPricing).sort(([a], [b]) => a.localeCompare(b)).map(([model, p]) => (
                  <div key={model} className="grid grid-cols-[2fr_1fr_1fr_1fr_auto] gap-1.5 items-center">
                    <input
                      value={model}
                      onChange={(e) => updateModel(model, "model", e.target.value)}
                      className="h-[26px] px-1.5 text-xs font-mono bg-[var(--app-input)] border border-app-border text-app-text"
                      placeholder="model-name"
                    />
                    <input
                      type="number"
                      min="0"
                      step="0.01"
                      value={p.input}
                      onChange={(e) => updateModel(model, "input", e.target.value)}
                      className="h-[26px] px-1 text-xs font-mono bg-[var(--app-input)] border border-app-border text-app-text text-center"
                    />
                    <input
                      type="number"
                      min="0"
                      step="0.01"
                      value={p.output}
                      onChange={(e) => updateModel(model, "output", e.target.value)}
                      className="h-[26px] px-1 text-xs font-mono bg-[var(--app-input)] border border-app-border text-app-text text-center"
                    />
                    <select
                      value={p.currency}
                      onChange={(e) => updateModel(model, "currency", e.target.value)}
                      className="h-[26px] px-1 text-xs font-mono bg-[var(--app-input)] border border-app-border text-app-text text-center"
                    >
                      <option value="USD">USD</option>
                      <option value="CNY">CNY</option>
                    </select>
                    <button
                      onClick={() => removeModel(model)}
                      className="p-0.5 text-app-text-dim hover:text-app-red transition-colors"
                      title="删除"
                    >
                      <Trash2 size={12} />
                    </button>
                  </div>
                ))}

                {/* Add new model */}
                <div className="flex gap-1.5">
                  <input
                    value={newModel}
                    onChange={(e) => setNewModel(e.target.value)}
                    onKeyDown={(e) => { if (e.key === "Enter") addModel(); }}
                    placeholder="新模型名称..."
                    className="flex-1 h-[26px] px-1.5 text-xs font-mono bg-[var(--app-input)] border border-app-border text-app-text-dim"
                  />
                  <button
                    onClick={addModel}
                    disabled={!newModel.trim() || !!editedPricing[newModel.trim()]}
                    className="flex items-center gap-1 px-2 h-[26px] text-xs font-mono text-app-text-dim
                      border border-app-border bg-[var(--app-input)] hover:bg-[var(--app-hover)]
                      disabled:opacity-30 transition-colors"
                  >
                    <Plus size={11} /> 添加
                  </button>
                </div>

                {/* Action buttons */}
                <div className="flex justify-end gap-2 pt-1">
                  <button
                    onClick={resetPricing}
                    disabled={!pricingDirty}
                    className="px-3 py-1 text-xs font-mono text-app-text-dim hover:text-app-text
                      border border-app-border hover:bg-[var(--app-hover)]
                      disabled:opacity-30 transition-colors"
                  >
                    撤销
                  </button>
                  <button
                    onClick={savePricing}
                    disabled={!pricingDirty || saving}
                    className="px-3 py-1 text-xs font-mono bg-app-accent text-[var(--app-bg)]
                      hover:bg-app-accent/80 disabled:opacity-30 transition-colors"
                  >
                    {saving ? "保存中..." : "保存"}
                  </button>
                </div>
              </div>
            )}
          </div>
        </div>

        {/* Footer */}
        <div className="px-4 py-2.5 border-t border-app-border bg-[var(--app-subtle)] flex items-center justify-between">
          <button
            onClick={refresh}
            className="text-xs text-app-text-dim hover:text-app-text font-mono transition-colors"
          >
            刷新
          </button>
          <button
            onClick={onClose}
            className="px-4 py-1 text-xs font-mono text-app-text-dim hover:text-app-text
              border border-app-border bg-[var(--app-input)] hover:bg-[var(--app-hover)]
              transition-colors"
          >
            关闭
          </button>
        </div>
      </div>
    </div>
  );
}
