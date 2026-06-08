# Token 用量追踪 + 费用仪表盘 — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a token usage tracking + cost dashboard using Claude Code/Codex hooks, storing data as JSONL, displayed in a lightweight CSS-only dashboard panel.

**Architecture:** Hook scripts (embedded Rust consts, written to disk by `ensure_shell_rc`) capture token data on `Stop`/`SessionEnd` events. Shell wrapper exports `KN_PROFILE`/`KN_CLI_TOOL` env vars for the hook to consume. A new Rust module `usage.rs` reads JSONL and aggregates. A new React `UsagePanel` renders the dashboard. Settings toggle controls hook registration.

**Tech Stack:** Tauri v2, React + TypeScript, CSS (no chart library), JSONL file, Python 3 (hook script)

---

### Task 1: Create Rust usage module

**Files:**
- Create: `desktop/src-tauri/src/usage.rs`

- [ ] **Step 1: Write the module**

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

// ── Data structures ─────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecord {
    pub ts: String,
    pub profile: String,
    pub tool: String,
    pub model: String,
    pub tokens_in: u64,
    pub tokens_out: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageSummary {
    pub total_tokens_in: u64,
    pub total_tokens_out: u64,
    pub total_cost: f64,
    pub currency: String,
    pub by_profile: Vec<ProfileUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileUsage {
    pub profile: String,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub cost: f64,
    pub currency: String,
    pub percentage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyUsage {
    pub date: String,
    pub tokens_in: u64,
    pub tokens_out: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPricing {
    pub input: f64,
    pub output: f64,
    pub currency: String, // "USD" or "CNY"
}

// ── Helpers ──────────────────────────────────────────────────

fn config_dir() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into());
    PathBuf::from(&home).join(".claude-profiles")
}

fn usage_file() -> PathBuf {
    config_dir().join("usage.jsonl")
}

fn pricing_file() -> PathBuf {
    config_dir().join("pricing.json")
}

// ── Default pricing table ────────────────────────────────────

pub fn default_pricing() -> HashMap<String, ModelPricing> {
    let mut m = HashMap::new();
    m.insert("claude-sonnet-4-6".into(), ModelPricing { input: 3.0, output: 15.0, currency: "USD".into() });
    m.insert("claude-opus-4-8".into(), ModelPricing { input: 15.0, output: 75.0, currency: "USD".into() });
    m.insert("claude-haiku-4-5".into(), ModelPricing { input: 0.8, output: 4.0, currency: "USD".into() });
    m.insert("deepseek-chat".into(), ModelPricing { input: 1.0, output: 2.0, currency: "CNY".into() });
    m.insert("deepseek-reasoner".into(), ModelPricing { input: 4.0, output: 16.0, currency: "CNY".into() });
    m.insert("gpt-5".into(), ModelPricing { input: 2.5, output: 10.0, currency: "USD".into() });
    m
}

// ── Read pricing (file or default) ───────────────────────────

pub fn load_pricing() -> HashMap<String, ModelPricing> {
    if let Ok(content) = fs::read_to_string(pricing_file()) {
        if let Ok(parsed) = serde_json::from_str::<HashMap<String, ModelPricing>>(&content) {
            return parsed;
        }
    }
    default_pricing()
}

pub fn save_pricing(pricing: &HashMap<String, ModelPricing>) -> Result<(), String> {
    let dir = config_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("create dir: {}", e))?;
    let json = serde_json::to_string_pretty(pricing).map_err(|e| format!("serialize: {}", e))?;
    fs::write(pricing_file(), json).map_err(|e| format!("write: {}", e))
}

// ── Parse ISO 8601 ts to day string "MM-DD" ──────────────────

fn ts_to_day(ts: &str) -> String {
    // ts format: "2026-06-03T14:30:00+08:00" or "2026-06-03T14:30:00Z"
    if ts.len() >= 10 {
        ts[5..10].to_string() // "06-03"
    } else {
        String::new()
    }
}

fn ts_to_date(ts: &str) -> String {
    if ts.len() >= 10 {
        ts[..10].to_string() // "2026-06-03"
    } else {
        String::new()
    }
}

// ── Compute cost ─────────────────────────────────────────────

fn compute_cost(model: &str, tokens_in: u64, tokens_out: u64, pricing: &HashMap<String, ModelPricing>) -> (f64, String) {
    // Try exact match first, then prefix match
    let price = pricing.get(model).or_else(|| {
        pricing.iter().find(|(k, _)| model.starts_with(k.as_str())).map(|(_, v)| v)
    });
    match price {
        Some(p) => {
            let cost = (tokens_in as f64 / 1_000_000.0) * p.input
                     + (tokens_out as f64 / 1_000_000.0) * p.output;
            (cost, p.currency.clone())
        }
        None => (0.0, "USD".into()),
    }
}

// ── Tauri commands ───────────────────────────────────────────

#[tauri::command]
pub fn get_usage(days: u32) -> Result<UsageSummary, String> {
    let pricing = load_pricing();
    let cutoff = chrono::Utc::now() - chrono::Duration::days(days as i64);
    let cutoff_str = cutoff.format("%Y-%m-%d").to_string();

    let file = fs::File::open(usage_file()).map_err(|e| format!("open usage.jsonl: {}", e))?;
    let reader = BufReader::new(file);

    let mut total_in: u64 = 0;
    let mut total_out: u64 = 0;
    let mut total_cost: f64 = 0.0;
    let mut dominant_currency = "USD".to_string();
    let mut profile_stats: HashMap<String, (u64, u64, f64, String)> = HashMap::new();

    for line in reader.lines() {
        let line = line.unwrap_or_default();
        if line.is_empty() { continue; }
        if let Ok(rec) = serde_json::from_str::<UsageRecord>(&line) {
            if ts_to_date(&rec.ts) < cutoff_str { continue; }
            total_in += rec.tokens_in;
            total_out += rec.tokens_out;
            let (cost, curr) = compute_cost(&rec.model, rec.tokens_in, rec.tokens_out, &pricing);
            total_cost += cost;
            dominant_currency = curr;
            let entry = profile_stats.entry(rec.profile.clone()).or_insert((0, 0, 0.0, curr.clone()));
            entry.0 += rec.tokens_in;
            entry.1 += rec.tokens_out;
            entry.2 += cost;
        }
    }

    let grand_total = total_in + total_out;
    let mut by_profile: Vec<ProfileUsage> = profile_stats
        .into_iter()
        .map(|(profile, (tin, tout, cost, curr))| ProfileUsage {
            profile,
            tokens_in: tin,
            tokens_out: tout,
            cost,
            currency: curr,
            percentage: if grand_total > 0 {
                ((tin + tout) as f64 / grand_total as f64) * 100.0
            } else { 0.0 },
        })
        .collect();
    by_profile.sort_by(|a, b| b.tokens_in.cmp(&a.tokens_in).then_with(|| b.tokens_out.cmp(&a.tokens_out)));

    Ok(UsageSummary {
        total_tokens_in: total_in,
        total_tokens_out: total_out,
        total_cost,
        currency: dominant_currency,
        by_profile,
    })
}

#[tauri::command]
pub fn get_daily_usage(days: u32) -> Result<Vec<DailyUsage>, String> {
    let cutoff = chrono::Utc::now() - chrono::Duration::days(days as i64);
    let cutoff_str = cutoff.format("%Y-%m-%d").to_string();

    let file = match fs::File::open(usage_file()) {
        Ok(f) => f,
        Err(_) => return Ok(Vec::new()),
    };
    let reader = BufReader::new(file);

    let mut daily: HashMap<String, (u64, u64)> = HashMap::new();
    for line in reader.lines() {
        let line = line.unwrap_or_default();
        if line.is_empty() { continue; }
        if let Ok(rec) = serde_json::from_str::<UsageRecord>(&line) {
            if ts_to_date(&rec.ts) < cutoff_str { continue; }
            let day = ts_to_day(&rec.ts);
            let entry = daily.entry(day).or_insert((0, 0));
            entry.0 += rec.tokens_in;
            entry.1 += rec.tokens_out;
        }
    }

    let mut result: Vec<DailyUsage> = daily
        .into_iter()
        .map(|(date, (tin, tout))| DailyUsage { date, tokens_in: tin, tokens_out: tout })
        .collect();
    result.sort_by(|a, b| a.date.cmp(&b.date));
    Ok(result)
}

#[tauri::command]
pub fn get_pricing() -> Result<HashMap<String, ModelPricing>, String> {
    Ok(load_pricing())
}

#[tauri::command]
pub fn set_pricing(model: String, pricing: ModelPricing) -> Result<(), String> {
    let mut p = load_pricing();
    p.insert(model, pricing);
    save_pricing(&p)
}

#[tauri::command]
pub fn get_usage_tracking_enabled() -> Result<bool, String> {
    let config_path = config_dir().join("config.yaml");
    if !config_path.exists() { return Ok(false); }
    let content = fs::read_to_string(&config_path).unwrap_or_default();
    Ok(content.contains("usage_tracking: true"))
}

#[tauri::command]
pub fn set_usage_tracking_enabled(enabled: bool) -> Result<String, String> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into());
    let hooks_dir = config_dir().join("hooks");
    fs::create_dir_all(&hooks_dir).map_err(|e| format!("create hooks dir: {}", e))?;

    // Determine python command per platform
    let python = if cfg!(target_os = "windows") {
        "python".to_string()
    } else {
        "python3".to_string()
    };
    let hook_cmd = format!("{} {}/record-usage.py", python, hooks_dir.display());

    if enabled {
        // Inject hook into Claude Code settings
        let claude_settings = PathBuf::from(&home).join(".claude").join("settings.json");
        inject_claude_hook(&claude_settings, &hook_cmd)?;

        // Inject hook into Codex config
        let codex_config = PathBuf::from(&home).join(".codex").join("config.toml");
        inject_codex_hook(&codex_config, &hook_cmd)?;
    } else {
        // Remove hooks from both configs
        let claude_settings = PathBuf::from(&home).join(".claude").join("settings.json");
        remove_claude_hook(&claude_settings)?;

        let codex_config = PathBuf::from(&home).join(".codex").join("config.toml");
        remove_codex_hook(&codex_config)?;
    }

    Ok("ok".into())
}

// ── Hook injection helpers ────────────────────────────────────

fn inject_claude_hook(path: &Path, hook_cmd: &str) -> Result<(), String> {
    let content = if path.exists() {
        fs::read_to_string(path).unwrap_or_else(|_| "{}".into())
    } else {
        // Ensure parent dir exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).ok();
        }
        "{}".into()
    };

    let mut settings: serde_json::Value = serde_json::from_str(&content)
        .unwrap_or(serde_json::json!({}));

    let hook_entry = serde_json::json!({
        "matcher": "",
        "hooks": [{
            "type": "command",
            "command": hook_cmd
        }]
    });

    let hooks = settings
        .as_object_mut()
        .ok_or("invalid settings.json")?
        .entry("hooks")
        .or_insert(serde_json::json!({}));

    let stop_hooks = hooks
        .as_object_mut()
        .ok_or("invalid hooks section")?
        .entry("Stop")
        .or_insert(serde_json::json!([]));

    if let Some(arr) = stop_hooks.as_array_mut() {
        // Check if our hook is already there
        let cmd_str = hook_cmd.to_string();
        if !arr.iter().any(|h| h["hooks"][0]["command"].as_str() == Some(&cmd_str)) {
            arr.push(hook_entry);
        }
    }

    let new_content = serde_json::to_string_pretty(&settings)
        .map_err(|e| format!("serialize settings: {}", e))?;
    fs::write(path, new_content).map_err(|e| format!("write settings.json: {}", e))
}

fn remove_claude_hook(path: &Path) -> Result<(), String> {
    if !path.exists() { return Ok(()); }
    let content = fs::read_to_string(path).unwrap_or_default();
    let mut settings: serde_json::Value = serde_json::from_str(&content)
        .unwrap_or(serde_json::json!({}));

    if let Some(hooks) = settings.get_mut("hooks") {
        if let Some(stop) = hooks.get_mut("Stop") {
            if let Some(arr) = stop.as_array_mut() {
                arr.retain(|h| !h["hooks"][0]["command"].as_str().unwrap_or("").contains("record-usage.py"));
            }
        }
    }

    let new_content = serde_json::to_string_pretty(&settings)
        .map_err(|e| format!("serialize settings: {}", e))?;
    fs::write(path, new_content).map_err(|e| format!("write settings.json: {}", e))
}

fn inject_codex_hook(path: &Path, hook_cmd: &str) -> Result<(), String> {
    if !path.exists() {
        // Create parent dir and empty config
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).ok();
        }
        let content = format!(
            "[features]\nhooks = true\n\n[[hooks.Stop]]\ntype = \"command\"\ncommand = \"{}\"\n",
            hook_cmd
        );
        return fs::write(path, &content).map_err(|e| format!("write codex config: {}", e));
    }

    let content = fs::read_to_string(path).unwrap_or_default();
    if content.contains("record-usage.py") { return Ok(()); }

    // Append hook entry
    let new_content = if content.ends_with('\n') {
        format!("{}\n[[hooks.Stop]]\ntype = \"command\"\ncommand = \"{}\"\n", content, hook_cmd)
    } else {
        format!("{}\n\n[[hooks.Stop]]\ntype = \"command\"\ncommand = \"{}\"\n", content, hook_cmd)
    };

    // Ensure hooks feature is enabled
    let new_content = if !new_content.contains("hooks = true") {
        if new_content.contains("[features]") {
            new_content.replacen("[features]", "[features]\nhooks = true", 1)
        } else {
            format!("[features]\nhooks = true\n\n{}", new_content)
        }
    } else {
        new_content
    };

    fs::write(path, &new_content).map_err(|e| format!("write codex config: {}", e))
}

fn remove_codex_hook(path: &Path) -> Result<(), String> {
    if !path.exists() { return Ok(()); }
    let content = fs::read_to_string(path).unwrap_or_default();
    let lines: Vec<&str> = content.lines().collect();
    let mut result: Vec<&str> = Vec::new();
    let mut skip_block = false;
    for line in &lines {
        if line.contains("record-usage.py") {
            skip_block = true;
            // Remove the preceding [[hooks.Stop]] line if it's the last one added
            if result.last().map_or(false, |l| l.trim() == "[[hooks.Stop]]") {
                result.pop();
            }
            // Also remove the "type" line immediately before [[hooks.Stop]]
            if result.last().map_or(false, |l| l.contains("type = \"command\"")) {
                result.pop();
            }
            continue;
        }
        if skip_block && line.trim().is_empty() {
            skip_block = false;
            continue;
        }
        if skip_block {
            continue;
        }
        result.push(line);
    }

    // Remove trailing blank lines
    while result.last().map_or(false, |l| l.trim().is_empty()) {
        result.pop();
    }

    let new_content = result.join("\n") + "\n";
    fs::write(path, &new_content).map_err(|e| format!("write codex config: {}", e))
}
```

- [ ] **Step 2: Add chrono dependency to Cargo.toml**

Modify `desktop/src-tauri/Cargo.toml`, add under `[dependencies]`:
```toml
chrono = { version = "0.4", features = ["serde"] }
```

- [ ] **Step 3: Build check**

Run: `cd desktop/src-tauri && cargo check 2>&1`
Expected: compile succeeds

---

### Task 2: Register usage commands + module in lib.rs

**Files:**
- Modify: `desktop/src-tauri/src/lib.rs`

- [ ] **Step 1: Add module declaration and register commands**

Replace the module declaration section (line 1-3) + the `invoke_handler` array:

At line 1-3, change:
```rust
mod commands;
mod profile_cmd;
mod pty;
```
To:
```rust
mod commands;
mod profile_cmd;
mod pty;
mod usage;
```

In the `invoke_handler` array (after `commands::batch_delete_profiles,`), add:
```rust
            usage::get_usage,
            usage::get_daily_usage,
            usage::get_pricing,
            usage::set_pricing,
            usage::get_usage_tracking_enabled,
            usage::set_usage_tracking_enabled,
```

- [ ] **Step 2: Build check**

Run: `cd desktop/src-tauri && cargo check 2>&1`
Expected: compile succeeds

---

### Task 3: Embed hook script + install in ensure_shell_rc

**Files:**
- Modify: `desktop/src-tauri/src/profile_cmd.rs`

- [ ] **Step 1: Add HOOK_RECORDER constant**

After the `SHELL_RC_PS1` constant definition (around line 448), add:

```rust
const HOOK_RECORDER: &str = r##"#!/usr/bin/env python3
"""Token usage recorder — called by Claude Code / Codex Stop hooks.
Reads structured JSON from stdin, extracts token usage, appends to usage.jsonl.
"""

import sys, json, os
from datetime import datetime, timezone

USAGE_FILE = os.path.join(
    os.path.expanduser("~"), ".claude-profiles", "usage.jsonl"
)


def main():
    try:
        data = json.load(sys.stdin)
    except (json.JSONDecodeError, OSError):
        sys.exit(0)

    profile = os.environ.get("KN_PROFILE", "")
    tool = os.environ.get("KN_CLI_TOOL", "")

    usage = extract(data)
    if not usage:
        sys.exit(0)

    record = {
        "ts": datetime.now(timezone.utc).isoformat(),
        "profile": profile,
        "tool": tool,
        **usage,
    }

    try:
        os.makedirs(os.path.dirname(USAGE_FILE), exist_ok=True)
        with open(USAGE_FILE, "a", encoding="utf-8") as f:
            f.write(json.dumps(record, ensure_ascii=False) + "\n")
    except OSError:
        pass

    sys.exit(0)


def extract(data):
    """Extract token usage from hook payload. Returns dict or None."""
    # Codex: TurnComplete carries token_usage
    if "token_usage" in data:
        u = data["token_usage"]
        return {
            "model": str(u.get("model", "")),
            "tokens_in": int(u.get("input", u.get("input_tokens", 0))),
            "tokens_out": int(u.get("output", u.get("output_tokens", 0))),
        }
    # Claude Code: usage field in Stop/SessionEnd
    if "usage" in data:
        u = data["usage"]
        return {
            "model": str(u.get("model", "")),
            "tokens_in": int(u.get("input_tokens", u.get("input", 0))),
            "tokens_out": int(u.get("output_tokens", u.get("output", 0))),
        }
    # Generic: top-level tokens_in / tokens_out
    if "tokens_in" in data or "tokens_out" in data:
        return {
            "model": str(data.get("model", "")),
            "tokens_in": int(data.get("tokens_in", 0)),
            "tokens_out": int(data.get("tokens_out", 0)),
        }
    return None


if __name__ == "__main__":
    main()
"##;
```

- [ ] **Step 2: Write hook script in ensure_shell_rc**

In the `ensure_shell_rc` function, after the shell-rc write block (around line 486, after `fs::write(dir.join("shell-rc.ps1"), SHELL_RC_PS1).ok();`), add:

```rust
    // Write token usage hook recorder script
    let hooks_dir = dir.join("hooks");
    fs::create_dir_all(&hooks_dir).ok();
    fs::write(hooks_dir.join("record-usage.py"), HOOK_RECORDER).ok();
```

- [ ] **Step 3: Build check**

Run: `cd desktop/src-tauri && cargo check 2>&1`
Expected: compile succeeds

---

### Task 4: Export KN_PROFILE / KN_CLI_TOOL from shell wrapper

**Files:**
- Modify: `desktop/src-tauri/src/profile_cmd.rs`

- [ ] **Step 1: Update bash shell wrapper**

In the `SHELL_RC` constant, inside the `ai()` function's `claude|codex)` case, after `local env_output=$(_profile_env "$1")` and before `(eval "$env_output" && command "$tool" "$@")`, add the env var exports.

Find this block in `SHELL_RC` (inside the `claude|codex)` case):
```bash
                local env_output=$(_profile_env "$1")
                if [ -n "$env_output" ]; then
                    local profile_name="$1"; shift
                    echo "-> Using profile: $profile_name"
                    (eval "$env_output" && command "$tool" "$@")
                    return
                fi
```

Replace with:
```bash
                local env_output=$(_profile_env "$1")
                if [ -n "$env_output" ]; then
                    local profile_name="$1"; shift
                    echo "-> Using profile: $profile_name"
                    (eval "$env_output" && export KN_PROFILE="$profile_name" && export KN_CLI_TOOL="$tool" && command "$tool" "$@")
                    return
                fi
```

- [ ] **Step 2: Update PowerShell shell wrapper**

In the `SHELL_RC_PS1` constant, inside the `ai` function, after `$envs | ForEach-Object { Invoke-Expression $_ }` and before `& $tool @args`, add env var sets.

Find:
```powershell
            $envs | ForEach-Object { Invoke-Expression $_ }
        }
    }
    & $tool @args
```

Replace with:
```powershell
            $envs | ForEach-Object { Invoke-Expression $_ }
        }
    }
    $env:KN_PROFILE = $profile
    $env:KN_CLI_TOOL = $tool
    & $tool @args
```

- [ ] **Step 3: Build check**

Run: `cd desktop/src-tauri && cargo check 2>&1`
Expected: compile succeeds

---

### Task 5: Add usage API functions to tauri-api.ts

**Files:**
- Modify: `desktop/src/lib/tauri-api.ts`

- [ ] **Step 1: Add usage invoke wrappers**

At the end of the file, append:

```typescript
export interface UsageSummary {
  total_tokens_in: number;
  total_tokens_out: number;
  total_cost: number;
  currency: string;
  by_profile: ProfileUsage[];
}

export interface ProfileUsage {
  profile: string;
  tokens_in: number;
  tokens_out: number;
  cost: number;
  currency: string;
  percentage: number;
}

export interface DailyUsage {
  date: string;
  tokens_in: number;
  tokens_out: number;
}

export interface ModelPricing {
  input: number;
  output: number;
  currency: string;
}

export async function getUsage(days: number): Promise<UsageSummary> {
  return invoke("get_usage", { days });
}

export async function getDailyUsage(days: number): Promise<DailyUsage[]> {
  return invoke("get_daily_usage", { days });
}

export async function getPricing(): Promise<Record<string, ModelPricing>> {
  return invoke("get_pricing");
}

export async function setPricing(model: string, pricing: ModelPricing): Promise<void> {
  return invoke("set_pricing", { model, pricing });
}

export async function getUsageTrackingEnabled(): Promise<boolean> {
  return invoke("get_usage_tracking_enabled");
}

export async function setUsageTrackingEnabled(enabled: boolean): Promise<void> {
  return invoke("set_usage_tracking_enabled", { enabled });
}
```

---

### Task 6: Create useUsage hook

**Files:**
- Create: `desktop/src/hooks/useUsage.ts`

- [ ] **Step 1: Write the hook**

```typescript
import { useState, useCallback, useEffect, useRef } from "react";
import {
  getUsage,
  getDailyUsage,
  type UsageSummary,
  type DailyUsage,
} from "../lib/tauri-api";

export function useUsage() {
  const [summary, setSummary] = useState<UsageSummary | null>(null);
  const [daily, setDaily] = useState<DailyUsage[]>([]);
  const [loading, setLoading] = useState(false);
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const refresh = useCallback(async () => {
    try {
      const [s, d] = await Promise.all([
        getUsage(30),
        getDailyUsage(7),
      ]);
      setSummary(s);
      setDaily(d);
    } catch {
      // silently fail — usage.jsonl might not exist yet
    } finally {
      setLoading(false);
    }
  }, []);

  // Poll every 30s while mounted
  useEffect(() => {
    refresh();
    intervalRef.current = setInterval(refresh, 30_000);
    return () => {
      if (intervalRef.current) clearInterval(intervalRef.current);
    };
  }, [refresh]);

  // Today's tokens (same as summary total when days=1, but we compute from daily)
  const todayTokens = daily.length > 0
    ? daily[daily.length - 1].tokens_in + daily[daily.length - 1].tokens_out
    : 0;

  return { summary, daily, todayTokens, loading, refresh };
}
```

---

### Task 7: Create UsagePanel component

**Files:**
- Create: `desktop/src/components/UsagePanel.tsx`

- [ ] **Step 1: Write the component**

```typescript
import React, { useState, useEffect } from "react";
import { X, BarChart3 } from "lucide-react";
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

  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
      onClick={onClose}
    >
      <div
        className="bg-app-panel border border-app-border shadow-dialog w-[520px] max-h-[80vh] overflow-y-auto select-none animate-[scaleIn_150ms_ease-out]"
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
            {([
              ["today", "今日"],
              ["week", "本周"],
              ["month", "本月"],
            ] as const).map(([k, label]) => (
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

          {/* Summary cards */}
          {summary && summary.total_tokens_in + summary.total_tokens_out > 0 ? (
            <>
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
```

---

### Task 8: Integrate UsagePanel + StatusBar entry into App.tsx

**Files:**
- Modify: `desktop/src/App.tsx`

- [ ] **Step 1: Add imports**

At top of file, add:
```typescript
import { UsagePanel } from "./components/UsagePanel";
import { useUsage } from "./hooks/useUsage";
```

- [ ] **Step 2: Add state and hook**

After `const [showBatchDeleteConfirm, setShowBatchDeleteConfirm] = useState(false);` (around line 49), add:
```typescript
  const [showUsage, setShowUsage] = useState(false);
  const usage = useUsage();
```

- [ ] **Step 3: Add usage entry to StatusBar**

In the StatusBar section (line 690 div), after `{isAnyTerminalOpen && (...)}`, add a clickable usage entry:

```typescript
            {usage.todayTokens > 0 && (
              <span
                className="text-2xs text-app-amber font-mono mr-3 cursor-pointer hover:text-app-amber-glow transition-colors"
                onClick={() => setShowUsage(true)}
              >
                ◉ {usage.todayTokens >= 1000 ? `${(usage.todayTokens / 1000).toFixed(1)}K` : usage.todayTokens} 今天
              </span>
            )}
            {usage.todayTokens === 0 && !usage.loading && (
              <span
                className="text-2xs text-app-text-dim font-mono mr-3 cursor-pointer hover:text-app-text-muted transition-colors"
                onClick={() => setShowUsage(true)}
              >
                ◉ 用量
              </span>
            )}
```

- [ ] **Step 4: Add UsagePanel to render tree**

After the `<ShortcutsPanel ... />` line, add:
```typescript
      <UsagePanel open={showUsage} onClose={() => setShowUsage(false)} />
```

---

### Task 9: Add tracking toggle to SettingsDialog

**Files:**
- Modify: `desktop/src/components/SettingsDialog.tsx`

- [ ] **Step 1: Add imports and state**

At top, add:
```typescript
import { getUsageTrackingEnabled, setUsageTrackingEnabled } from "../lib/tauri-api";
```

Inside the component, after `const { scale, setScale } = useFontScale();`, add:
```typescript
  const [trackingEnabled, setTrackingEnabled] = React.useState(false);
  React.useEffect(() => {
    if (open) {
      getUsageTrackingEnabled().then(setTrackingEnabled).catch(() => {});
    }
  }, [open]);
```

- [ ] **Step 2: Add toggle UI**

After the "Terminal note" div (the `💡` tip block around line 86-89), add:

```typescript
          {/* Token usage tracking toggle */}
          <div className="space-y-2">
            <div className="flex items-center justify-between">
              <div>
                <span className="text-sm text-app-text font-mono">Token 用量追踪</span>
                <p className="text-2xs text-app-text-muted font-mono mt-0.5">
                  自动记录每次 AI 会话的 token 消耗和费用
                </p>
              </div>
              <button
                onClick={async () => {
                  const next = !trackingEnabled;
                  try {
                    await setUsageTrackingEnabled(next);
                    setTrackingEnabled(next);
                  } catch (e) {
                    console.error("Failed to toggle usage tracking:", e);
                  }
                }}
                className={`relative w-9 h-5 rounded-full transition-colors duration-200 ${
                  trackingEnabled ? "bg-app-accent" : "bg-[var(--app-border)]"
                }`}
              >
                <span
                  className={`absolute top-0.5 w-4 h-4 rounded-full bg-[var(--app-bg)] border border-app-border transition-all duration-200 ${
                    trackingEnabled ? "left-4" : "left-0.5"
                  }`}
                />
              </button>
            </div>
            {trackingEnabled && (
              <div className="text-2xs text-app-text-muted font-mono bg-[var(--app-subtle)] border border-app-border px-3 py-1.5">
                数据保存在 ~/.claude-profiles/usage.jsonl，完全本地，不会上传。
              </div>
            )}
          </div>
```

---

### Task 10: Final build and verification

**Files:**
- None (verification only)

- [ ] **Step 1: Full build check**

Run: `cd desktop && npx tsc --noEmit 2>&1`
Expected: no type errors

Run: `cd desktop/src-tauri && cargo check 2>&1`
Expected: compile succeeds

- [ ] **Step 2: Verify hook script is embedded**

Run: `cd desktop/src-tauri && cargo build 2>&1`
Then: `strings target/debug/ai-profile-manager | grep "Token usage recorder" | head -1`
Expected: finds the docstring

- [ ] **Step 3: Commit all changes**

```bash
git add desktop/src-tauri/src/usage.rs \
        desktop/src-tauri/src/lib.rs \
        desktop/src-tauri/src/profile_cmd.rs \
        desktop/src-tauri/Cargo.toml \
        desktop/src/components/UsagePanel.tsx \
        desktop/src/hooks/useUsage.ts \
        desktop/src/lib/tauri-api.ts \
        desktop/src/App.tsx \
        desktop/src/components/SettingsDialog.tsx
git commit -m "feat: add token usage tracking + cost dashboard

- New usage.rs Rust module: JSONL read/aggregate, pricing, hook inject/remove
- Hook recorder Python script embedded in profile_cmd.rs, installed by ensure_shell_rc
- Shell wrapper exports KN_PROFILE/KN_CLI_TOOL for hook consumption
- UsagePanel component: summary cards, per-profile breakdown, 7-day CSS bar chart
- StatusBar usage entry with live today-token count
- Settings toggle to enable/disable hook registration
- Zero new dependencies beyond chrono (already used in project)

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Verification

After implementation, test manually:

1. Launch App → Settings → toggle "Token 用量追踪" ON
2. Check `~/.claude/settings.json` has the Stop hook entry
3. Check `~/.codex/config.toml` has the Stop hook entry  
4. Run `ai claude <profile>` in terminal → make a real Claude Code query → exit
5. Check `~/.claude-profiles/usage.jsonl` has a new line with correct profile/tool/model/tokens
6. Open UsagePanel → see today's tokens and cost
7. Toggle tracking OFF → check hooks removed from configs
8. Test on macOS and Windows (Git Bash PTY path)
