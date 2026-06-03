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
    pub by_model: Vec<ModelUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelUsage {
    pub model: String,
    pub tokens_in: u64,
    pub tokens_out: u64,
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
    pub currency: String,
}

// ── Helpers ──────────────────────────────────────────────────

fn usage_file() -> PathBuf {
    crate::config_dir().join("usage.jsonl")
}

fn pricing_file() -> PathBuf {
    crate::config_dir().join("pricing.json")
}

fn parse_local_date(ts: &str) -> Option<chrono::NaiveDate> {
    chrono::DateTime::parse_from_rfc3339(ts)
        .ok()
        .map(|dt| dt.with_timezone(&chrono::Local).date_naive())
}

fn cutoff_date(days: u32) -> chrono::NaiveDate {
    let days = days.max(1) as i64;
    chrono::Local::now().date_naive() - chrono::Duration::days(days - 1)
}

// ── Default pricing ─────────────────────────────────────────

pub fn default_pricing() -> HashMap<String, ModelPricing> {
    let mut m = HashMap::new();
    m.insert("claude-sonnet-4-6".into(), ModelPricing { input: 3.0, output: 15.0, currency: "USD".into() });
    m.insert("claude-opus-4-8".into(), ModelPricing { input: 15.0, output: 75.0, currency: "USD".into() });
    m.insert("claude-haiku-4-5".into(), ModelPricing { input: 0.8, output: 4.0, currency: "USD".into() });
    m.insert("deepseek-chat".into(), ModelPricing { input: 1.0, output: 2.0, currency: "CNY".into() });
    m.insert("deepseek-reasoner".into(), ModelPricing { input: 4.0, output: 16.0, currency: "CNY".into() });
    m.insert("deepseek-v4-pro".into(), ModelPricing { input: 1.0, output: 2.0, currency: "CNY".into() });
    m.insert("deepseek-v3".into(), ModelPricing { input: 1.0, output: 2.0, currency: "CNY".into() });
    m.insert("claude-3.5".into(), ModelPricing { input: 3.0, output: 15.0, currency: "USD".into() });
    m.insert("claude-3".into(), ModelPricing { input: 3.0, output: 15.0, currency: "USD".into() });
    m.insert("gpt-5.5".into(), ModelPricing { input: 2.5, output: 10.0, currency: "USD".into() });
    m.insert("gpt-5".into(), ModelPricing { input: 2.5, output: 10.0, currency: "USD".into() });
    m.insert("gpt-4".into(), ModelPricing { input: 5.0, output: 15.0, currency: "USD".into() });
    m
}

pub fn load_pricing() -> HashMap<String, ModelPricing> {
    if let Ok(content) = fs::read_to_string(pricing_file()) {
        if let Ok(parsed) = serde_json::from_str::<HashMap<String, ModelPricing>>(&content) {
            return parsed;
        }
    }
    default_pricing()
}

pub fn save_pricing(pricing: &HashMap<String, ModelPricing>) -> Result<(), String> {
    let dir = crate::config_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("create dir: {}", e))?;
    let json = serde_json::to_string_pretty(pricing).map_err(|e| format!("serialize: {}", e))?;
    fs::write(pricing_file(), json).map_err(|e| format!("write: {}", e))
}

// ── Tauri commands ───────────────────────────────────────────

#[tauri::command]
pub fn get_usage(days: u32) -> Result<UsageSummary, String> {
    let cutoff = cutoff_date(days);

    let file = fs::File::open(usage_file()).map_err(|e| format!("open usage.jsonl: {}", e))?;
    let reader = BufReader::new(file);

    let mut total_in: u64 = 0;
    let mut total_out: u64 = 0;
    let mut model_stats: HashMap<String, (u64, u64)> = HashMap::new();

    for line in reader.lines() {
        let line = line.unwrap_or_default();
        if line.is_empty() { continue; }
        if let Ok(rec) = serde_json::from_str::<UsageRecord>(&line) {
            let Some(day) = parse_local_date(&rec.ts) else { continue };
            if day < cutoff { continue; }
            total_in += rec.tokens_in;
            total_out += rec.tokens_out;
            let entry = model_stats.entry(rec.model.clone()).or_insert((0, 0));
            entry.0 += rec.tokens_in;
            entry.1 += rec.tokens_out;
        }
    }

    let grand_total = total_in + total_out;
    let mut by_model: Vec<ModelUsage> = model_stats
        .into_iter()
        .map(|(model, (tin, tout))| {
            let model = if model.is_empty() { "unknown".to_string() } else { model };
            ModelUsage {
                model,
                tokens_in: tin,
                tokens_out: tout,
                percentage: if grand_total > 0 {
                    ((tin + tout) as f64 / grand_total as f64) * 100.0
                } else { 0.0 },
            }
        })
        .collect();
    by_model.sort_by(|a, b| {
        (b.tokens_in + b.tokens_out)
            .cmp(&(a.tokens_in + a.tokens_out))
    });

    Ok(UsageSummary {
        total_tokens_in: total_in,
        total_tokens_out: total_out,
        by_model,
    })
}

#[tauri::command]
pub fn get_daily_usage(days: u32) -> Result<Vec<DailyUsage>, String> {
    let cutoff = cutoff_date(days);

    let file = match fs::File::open(usage_file()) {
        Ok(f) => f,
        Err(_) => return Ok(Vec::new()),
    };
    let reader = BufReader::new(file);

    let mut daily: HashMap<chrono::NaiveDate, (u64, u64)> = HashMap::new();
    for line in reader.lines() {
        let line = line.unwrap_or_default();
        if line.is_empty() { continue; }
        if let Ok(rec) = serde_json::from_str::<UsageRecord>(&line) {
            let Some(day) = parse_local_date(&rec.ts) else { continue };
            if day < cutoff { continue; }
            let entry = daily.entry(day).or_insert((0, 0));
            entry.0 += rec.tokens_in;
            entry.1 += rec.tokens_out;
        }
    }

    let mut result: Vec<DailyUsage> = daily
        .into_iter()
        .map(|(date, (tin, tout))| DailyUsage { date: date.to_string(), tokens_in: tin, tokens_out: tout })
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
pub fn replace_pricing(pricing: HashMap<String, ModelPricing>) -> Result<(), String> {
    save_pricing(&pricing)
}

#[tauri::command]
pub fn get_usage_tracking_enabled() -> Result<bool, String> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into());
    let claude_settings = PathBuf::from(&home).join(".claude").join("settings.json");
    if claude_settings.exists() {
        let content = fs::read_to_string(&claude_settings).unwrap_or_default();
        if content.contains("record-usage.py") {
            return Ok(true);
        }
    }
    let codex_config = PathBuf::from(&home).join(".codex").join("config.toml");
    if codex_config.exists() {
        let content = fs::read_to_string(&codex_config).unwrap_or_default();
        if content.contains("record-usage.py") {
            return Ok(true);
        }
    }
    Ok(false)
}

#[tauri::command]
pub fn set_usage_tracking_enabled(enabled: bool) -> Result<String, String> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into());
    let hooks_dir = crate::config_dir().join("hooks");
    fs::create_dir_all(&hooks_dir).map_err(|e| format!("create hooks dir: {}", e))?;

    let python = if cfg!(target_os = "windows") { "python" } else { "python3" };
    let hooks_dir_str = hooks_dir.to_string_lossy().replace('\\', "/");
    let hook_cmd = format!("{} {}/record-usage.py", python, hooks_dir_str);

    if enabled {
        let claude_settings = PathBuf::from(&home).join(".claude").join("settings.json");
        inject_claude_hook(&claude_settings, &hook_cmd)?;
        let codex_config = PathBuf::from(&home).join(".codex").join("config.toml");
        inject_codex_hook(&codex_config, &hook_cmd)?;
    } else {
        let claude_settings = PathBuf::from(&home).join(".claude").join("settings.json");
        remove_claude_hook(&claude_settings)?;
        let codex_config = PathBuf::from(&home).join(".codex").join("config.toml");
        remove_codex_hook(&codex_config)?;
    }

    Ok("ok".into())
}

fn inject_claude_hook(path: &Path, hook_cmd: &str) -> Result<(), String> {
    let content = if path.exists() {
        fs::read_to_string(path).unwrap_or_else(|_| "{}".into())
    } else {
        if let Some(parent) = path.parent() { fs::create_dir_all(parent).ok(); }
        "{}".into()
    };

    let mut settings: serde_json::Value = serde_json::from_str(&content).unwrap_or(serde_json::json!({}));

    let hook_entry = serde_json::json!({
        "matcher": "",
        "hooks": [{"type": "command", "command": hook_cmd}]
    });

    let hooks = settings.as_object_mut().ok_or("invalid settings.json")?
        .entry("hooks").or_insert(serde_json::json!({}));
    let stop_hooks = hooks.as_object_mut().ok_or("invalid hooks section")?
        .entry("Stop").or_insert(serde_json::json!([]));

    if let Some(arr) = stop_hooks.as_array_mut() {
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
    let mut settings: serde_json::Value = serde_json::from_str(&content).unwrap_or(serde_json::json!({}));

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
        if let Some(parent) = path.parent() { fs::create_dir_all(parent).ok(); }
        let content = format!(
            "[features]\nhooks = true\n\n[[hooks.Stop]]\ntype = \"command\"\ncommand = \"{}\"\n",
            hook_cmd
        );
        return fs::write(path, &content).map_err(|e| format!("write codex config: {}", e));
    }

    let content = fs::read_to_string(path).unwrap_or_default();
    if content.contains("record-usage.py") { return Ok(()); }

    let new_content = if content.ends_with('\n') {
        format!("{}\n[[hooks.Stop]]\ntype = \"command\"\ncommand = \"{}\"\n", content, hook_cmd)
    } else {
        format!("{}\n\n[[hooks.Stop]]\ntype = \"command\"\ncommand = \"{}\"\n", content, hook_cmd)
    };

    let new_content = if !new_content.contains("hooks = true") {
        if new_content.contains("[features]") {
            new_content.replacen("[features]", "[features]\nhooks = true", 1)
        } else {
            format!("[features]\nhooks = true\n\n{}", new_content)
        }
    } else { new_content };

    fs::write(path, &new_content).map_err(|e| format!("write codex config: {}", e))
}

fn remove_codex_hook(path: &Path) -> Result<(), String> {
    if !path.exists() { return Ok(()); }
    let content = fs::read_to_string(path).unwrap_or_default();
    let lines: Vec<&str> = content.lines().collect();
    let mut result: Vec<&str> = Vec::new();
    for line in &lines {
        if line.contains("record-usage.py") {
            if result.last().map_or(false, |l| l.trim() == "[[hooks.Stop]]") { result.pop(); }
            if result.last().map_or(false, |l| l.contains("type = \"command\"")) { result.pop(); }
            continue;
        }
        result.push(line);
    }
    while result.last().map_or(false, |l| l.trim().is_empty()) { result.pop(); }
    let new_content = result.join("\n") + "\n";
    fs::write(path, &new_content).map_err(|e| format!("write codex config: {}", e))
}
