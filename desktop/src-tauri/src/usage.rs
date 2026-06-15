use crate::with_cross_process_lock;
use crate::with_write_lock;
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
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
pub struct ProjectUsage {
    pub project_path: Option<String>,
    pub project_name: Option<String>,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub percentage: f64,
    /// Model breakdown for this project (only populated when drilling in)
    pub models: Vec<ModelUsage>,
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
    m.insert(
        "claude-sonnet-4-6".into(),
        ModelPricing {
            input: 3.0,
            output: 15.0,
            currency: "USD".into(),
        },
    );
    m.insert(
        "claude-opus-4-8".into(),
        ModelPricing {
            input: 15.0,
            output: 75.0,
            currency: "USD".into(),
        },
    );
    m.insert(
        "claude-haiku-4-5".into(),
        ModelPricing {
            input: 0.8,
            output: 4.0,
            currency: "USD".into(),
        },
    );
    m.insert(
        "deepseek-chat".into(),
        ModelPricing {
            input: 1.0,
            output: 2.0,
            currency: "CNY".into(),
        },
    );
    m.insert(
        "deepseek-reasoner".into(),
        ModelPricing {
            input: 4.0,
            output: 16.0,
            currency: "CNY".into(),
        },
    );
    m.insert(
        "deepseek-v4-pro".into(),
        ModelPricing {
            input: 1.0,
            output: 2.0,
            currency: "CNY".into(),
        },
    );
    m.insert(
        "deepseek-v3".into(),
        ModelPricing {
            input: 1.0,
            output: 2.0,
            currency: "CNY".into(),
        },
    );
    m.insert(
        "claude-3.5".into(),
        ModelPricing {
            input: 3.0,
            output: 15.0,
            currency: "USD".into(),
        },
    );
    m.insert(
        "claude-3".into(),
        ModelPricing {
            input: 3.0,
            output: 15.0,
            currency: "USD".into(),
        },
    );
    m.insert(
        "gpt-5.5".into(),
        ModelPricing {
            input: 2.5,
            output: 10.0,
            currency: "USD".into(),
        },
    );
    m.insert(
        "gpt-5".into(),
        ModelPricing {
            input: 2.5,
            output: 10.0,
            currency: "USD".into(),
        },
    );
    m.insert(
        "gpt-4".into(),
        ModelPricing {
            input: 5.0,
            output: 15.0,
            currency: "USD".into(),
        },
    );
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
    crate::with_write_lock_exclusive(|| {
    let dir = crate::config_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("create dir: {}", e))?;
    let json = serde_json::to_string_pretty(pricing).map_err(|e| format!("serialize: {}", e))?;
    fs::write(pricing_file(), json).map_err(|e| format!("write: {}", e))
    })
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
        if line.is_empty() {
            continue;
        }
        if let Ok(rec) = serde_json::from_str::<UsageRecord>(&line) {
            let Some(day) = parse_local_date(&rec.ts) else {
                continue;
            };
            if day < cutoff {
                continue;
            }
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
            let model = if model.is_empty() {
                "unknown".to_string()
            } else {
                model
            };
            ModelUsage {
                model,
                tokens_in: tin,
                tokens_out: tout,
                percentage: if grand_total > 0 {
                    ((tin + tout) as f64 / grand_total as f64) * 100.0
                } else {
                    0.0
                },
            }
        })
        .collect();
    by_model.sort_by(|a, b| (b.tokens_in + b.tokens_out).cmp(&(a.tokens_in + a.tokens_out)));

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
        if line.is_empty() {
            continue;
        }
        if let Ok(rec) = serde_json::from_str::<UsageRecord>(&line) {
            let Some(day) = parse_local_date(&rec.ts) else {
                continue;
            };
            if day < cutoff {
                continue;
            }
            let entry = daily.entry(day).or_insert((0, 0));
            entry.0 += rec.tokens_in;
            entry.1 += rec.tokens_out;
        }
    }

    let mut result: Vec<DailyUsage> = daily
        .into_iter()
        .map(|(date, (tin, tout))| DailyUsage {
            date: date.to_string(),
            tokens_in: tin,
            tokens_out: tout,
        })
        .collect();
    result.sort_by(|a, b| a.date.cmp(&b.date));
    Ok(result)
}

#[tauri::command]
pub fn get_usage_by_project(days: u32) -> Result<Vec<ProjectUsage>, String> {
    let cutoff = cutoff_date(days);

    let file = match fs::File::open(usage_file()) {
        Ok(f) => f,
        Err(_) => return Ok(Vec::new()),
    };
    let reader = BufReader::new(file);

    // Aggregate: project_path -> (tokens_in, tokens_out, project_name, models map)
    struct Agg {
        tokens_in: u64,
        tokens_out: u64,
        project_name: Option<String>,
        models: HashMap<String, (u64, u64)>,
    }

    let mut projects: HashMap<String, Agg> = HashMap::new();
    let mut unlinked_in: u64 = 0;
    let mut unlinked_out: u64 = 0;
    let mut unlinked_models: HashMap<String, (u64, u64)> = HashMap::new();
    let mut grand_total: u64 = 0;

    for line in reader.lines() {
        let line = line.unwrap_or_default();
        if line.is_empty() {
            continue;
        }
        if let Ok(rec) = serde_json::from_str::<UsageRecord>(&line) {
            let Some(day) = parse_local_date(&rec.ts) else {
                continue;
            };
            if day < cutoff {
                continue;
            }
            let tin = rec.tokens_in;
            let tout = rec.tokens_out;
            grand_total += tin + tout;
            let model = if rec.model.is_empty() { "unknown".to_string() } else { rec.model.clone() };

            if let Some(ref pp) = rec.project_path {
                let entry = projects.entry(pp.clone()).or_insert_with(|| Agg {
                    tokens_in: 0,
                    tokens_out: 0,
                    project_name: rec.project_name.clone(),
                    models: HashMap::new(),
                });
                entry.tokens_in += tin;
                entry.tokens_out += tout;
                let me = entry.models.entry(model).or_insert((0, 0));
                me.0 += tin;
                me.1 += tout;
            } else {
                unlinked_in += tin;
                unlinked_out += tout;
                let me = unlinked_models.entry(model).or_insert((0, 0));
                me.0 += tin;
                me.1 += tout;
            }
        }
    }

    let mut result: Vec<ProjectUsage> = projects
        .into_iter()
        .map(|(path, agg)| {
            let total = agg.tokens_in + agg.tokens_out;
            let mut models: Vec<ModelUsage> = agg
                .models
                .into_iter()
                .map(|(model, (tin, tout))| ModelUsage {
                    model,
                    tokens_in: tin,
                    tokens_out: tout,
                    percentage: if total > 0 {
                        ((tin + tout) as f64 / total as f64) * 100.0
                    } else {
                        0.0
                    },
                })
                .collect();
            models.sort_by(|a, b| (b.tokens_in + b.tokens_out).cmp(&(a.tokens_in + a.tokens_out)));
            ProjectUsage {
                project_path: Some(path),
                project_name: agg.project_name.filter(|n| !n.is_empty()),
                tokens_in: agg.tokens_in,
                tokens_out: agg.tokens_out,
                percentage: if grand_total > 0 {
                    ((agg.tokens_in + agg.tokens_out) as f64 / grand_total as f64) * 100.0
                } else {
                    0.0
                },
                models,
            }
        })
        .collect();

    // Sort by total tokens descending
    result.sort_by(|a, b| {
        (b.tokens_in + b.tokens_out)
            .cmp(&(a.tokens_in + a.tokens_out))
            .then_with(|| a.project_path.cmp(&b.project_path))
    });
    if unlinked_in > 0 || unlinked_out > 0 {
        let total_unlinked = unlinked_in + unlinked_out;
        let mut unlinked_model_list: Vec<ModelUsage> = unlinked_models
            .into_iter()
            .map(|(model, (tin, tout))| ModelUsage {
                model,
                tokens_in: tin,
                tokens_out: tout,
                percentage: if total_unlinked > 0 {
                    ((tin + tout) as f64 / total_unlinked as f64) * 100.0
                } else {
                    0.0
                },
            })
            .collect();
        unlinked_model_list.sort_by(|a, b| {
            (b.tokens_in + b.tokens_out).cmp(&(a.tokens_in + a.tokens_out))
        });
        result.push(ProjectUsage {
            project_path: None,
            project_name: Some("未关联项目".to_string()),
            tokens_in: unlinked_in,
            tokens_out: unlinked_out,
            percentage: if grand_total > 0 {
                (total_unlinked as f64 / grand_total as f64) * 100.0
            } else {
                0.0
            },
            models: unlinked_model_list,
        });
    }

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
    let home = crate::home_dir().to_string_lossy().to_string();
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
    with_write_lock(|| {
    with_cross_process_lock(|| {
    let home = crate::home_dir().to_string_lossy().to_string();
    let hooks_dir = crate::config_dir().join("hooks");
    fs::create_dir_all(&hooks_dir).map_err(|e| format!("create hooks dir: {}", e))?;

    let python = if cfg!(target_os = "windows") {
        "python"
    } else {
        "python3"
    };
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
    }) // with_cross_process_lock
    }) // with_write_lock
}

#[tauri::command]
pub fn ensure_usage_hooks() -> Result<(), String> {
    // Always remove and re-inject to guarantee the hook command uses the
    // correct current path (~/.kn/hooks/record-usage.py).  This handles
    // both first-time setup (no hooks → remove is a no-op) and migration
    // from the old ~/.claude-profiles/hooks/ path.
    set_usage_tracking_enabled(false)?; // clean up any old-path hooks
    set_usage_tracking_enabled(true)?;  // inject with correct current path
    Ok(())
}

fn inject_json_hook(path: &Path, event_name: &str, hook_cmd: &str) -> Result<(), String> {
    let content = if path.exists() {
        fs::read_to_string(path).unwrap_or_else(|_| "{}".into())
    } else {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).ok();
        }
        "{}".into()
    };

    let mut settings: serde_json::Value =
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}));

    let hook_entry = serde_json::json!({
        "matcher": "",
        "hooks": [{"type": "command", "command": hook_cmd}]
    });

    let hooks = settings
        .as_object_mut()
        .ok_or("invalid settings.json")?
        .entry("hooks")
        .or_insert(serde_json::json!({}));
    let event_hooks = hooks
        .as_object_mut()
        .ok_or("invalid hooks section")?
        .entry(event_name)
        .or_insert(serde_json::json!([]));

    if let Some(arr) = event_hooks.as_array_mut() {
        // Clean up any old-path hooks left over from previous installs
        arr.retain(|h| {
            !h["hooks"][0]["command"]
                .as_str()
                .unwrap_or("")
                .contains(".claude-profiles/record-usage.py")
        });

        let cmd_str = hook_cmd.to_string();
        if !arr
            .iter()
            .any(|h| h["hooks"][0]["command"].as_str() == Some(&cmd_str))
        {
            arr.push(hook_entry);
        }
    }

    let new_content = serde_json::to_string_pretty(&settings)
        .map_err(|e| format!("serialize settings: {}", e))?;
    fs::write(path, new_content).map_err(|e| format!("write settings.json: {}", e))
}

fn remove_json_hook(path: &Path, event_name: &str) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let content = fs::read_to_string(path).unwrap_or_default();
    let mut settings: serde_json::Value =
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}));

    if let Some(hooks) = settings.get_mut("hooks") {
        if let Some(event) = hooks.get_mut(event_name) {
            if let Some(arr) = event.as_array_mut() {
                arr.retain(|h| {
                    !h["hooks"][0]["command"]
                        .as_str()
                        .unwrap_or("")
                        .contains("record-usage.py")
                });
            }
        }
    }

    let new_content = serde_json::to_string_pretty(&settings)
        .map_err(|e| format!("serialize settings: {}", e))?;
    fs::write(path, new_content).map_err(|e| format!("write settings.json: {}", e))
}

fn inject_claude_hook(path: &Path, hook_cmd: &str) -> Result<(), String> {
    inject_json_hook(path, "Stop", hook_cmd)
}
fn remove_claude_hook(path: &Path) -> Result<(), String> {
    remove_json_hook(path, "Stop")
}

fn inject_codex_hook(path: &Path, hook_cmd: &str) -> Result<(), String> {
    let mut doc: toml_edit::DocumentMut = if path.exists() {
        let content = fs::read_to_string(path).map_err(|e| format!("read codex config: {}", e))?;
        if content.trim().is_empty() {
            toml_edit::DocumentMut::new()
        } else {
            content
                .parse()
                .map_err(|e| format!("parse codex config: {}", e))?
        }
    } else {
        toml_edit::DocumentMut::new()
    };

    ensure_codex_features_hooks(&mut doc)?;

    if !doc.contains_key("hooks") {
        doc.insert("hooks", toml_edit::table());
    }
    let hooks = doc["hooks"]
        .as_table_mut()
        .ok_or("hooks field is not a table")?;
    if !hooks.contains_key("Stop") {
        hooks.insert(
            "Stop",
            toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new()),
        );
    }
    let stop = hooks["Stop"]
        .as_array_of_tables_mut()
        .ok_or("hooks.Stop is not an array")?;

    if upsert_codex_usage_hook_in_place(stop, hook_cmd) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("create codex config dir: {}", e))?;
        }
        return fs::write(path, doc.to_string()).map_err(|e| format!("write codex config: {}", e));
    }

    let mut group = toml_edit::Table::new();
    let mut inner = toml_edit::ArrayOfTables::new();
    let mut item = toml_edit::Table::new();
    item.insert("type", toml_edit::value("command"));
    item.insert("command", toml_edit::value(hook_cmd));
    inner.push(item);
    group.insert("hooks", toml_edit::Item::ArrayOfTables(inner));
    stop.push(group);

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create codex config dir: {}", e))?;
    }
    fs::write(path, doc.to_string()).map_err(|e| format!("write codex config: {}", e))
}

fn upsert_codex_usage_hook_in_place(
    stop: &mut toml_edit::ArrayOfTables,
    hook_cmd: &str,
) -> bool {
    for group in stop.iter_mut() {
        let flat_usage_hook = group
            .get("command")
            .and_then(|v| v.as_str())
            .is_some_and(|cmd| cmd.contains("record-usage.py"));
        if flat_usage_hook {
            group.clear();
            let mut inner = toml_edit::ArrayOfTables::new();
            let mut item = toml_edit::Table::new();
            item.insert("type", toml_edit::value("command"));
            item.insert("command", toml_edit::value(hook_cmd));
            inner.push(item);
            group.insert("hooks", toml_edit::Item::ArrayOfTables(inner));
            return true;
        }

        let Some(inner) = group.get_mut("hooks").and_then(|v| v.as_array_of_tables_mut()) else {
            continue;
        };
        for hook in inner.iter_mut() {
            let nested_usage_hook = hook
                .get("command")
                .and_then(|v| v.as_str())
                .is_some_and(|cmd| cmd.contains("record-usage.py"));
            if nested_usage_hook {
                hook.insert("type", toml_edit::value("command"));
                hook.insert("command", toml_edit::value(hook_cmd));
                return true;
            }
        }
    }
    false
}

fn remove_codex_hook(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let content = fs::read_to_string(path).map_err(|e| format!("read codex config: {}", e))?;
    if content.trim().is_empty() {
        return Ok(());
    }
    let mut doc: toml_edit::DocumentMut = content
        .parse()
        .map_err(|e| format!("parse codex config: {}", e))?;
    remove_codex_usage_hook_from_doc(&mut doc);
    fs::write(path, doc.to_string()).map_err(|e| format!("write codex config: {}", e))
}

fn ensure_codex_features_hooks(doc: &mut toml_edit::DocumentMut) -> Result<(), String> {
    if !doc.contains_key("features") {
        doc.insert("features", toml_edit::table());
    }
    let features = doc["features"]
        .as_table_mut()
        .ok_or("features field is not a table")?;
    features.insert("hooks", toml_edit::value(true));
    Ok(())
}

fn remove_codex_usage_hook_from_doc(doc: &mut toml_edit::DocumentMut) {
    let Some(hooks) = doc.get_mut("hooks").and_then(|v| v.as_table_mut()) else {
        return;
    };
    let Some(stop) = hooks
        .get_mut("Stop")
        .and_then(|v| v.as_array_of_tables_mut())
    else {
        return;
    };

    let mut idx = 0;
    while idx < stop.len() {
        let Some(group) = stop.get_mut(idx) else {
            break;
        };

        let mut remove_group = false;
        if group
            .get("command")
            .and_then(|v| v.as_str())
            .is_some_and(|cmd| cmd.contains("record-usage.py"))
        {
            remove_group = true;
        } else if let Some(inner) = group
            .get_mut("hooks")
            .and_then(|v| v.as_array_of_tables_mut())
        {
            let mut hook_idx = 0;
            while hook_idx < inner.len() {
                let should_remove = inner
                    .get(hook_idx)
                    .and_then(|hook| hook.get("command"))
                    .and_then(|v| v.as_str())
                    .is_some_and(|cmd| cmd.contains("record-usage.py"));
                if should_remove {
                    inner.remove(hook_idx);
                } else {
                    hook_idx += 1;
                }
            }
            remove_group = inner.is_empty() && !group.contains_key("matcher");
        }

        if remove_group {
            stop.remove(idx);
        } else {
            idx += 1;
        }
    }

    if stop.is_empty() {
        hooks.remove("Stop");
    }
}
