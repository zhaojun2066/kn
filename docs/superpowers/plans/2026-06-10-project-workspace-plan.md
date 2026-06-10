# Project Workspace Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a project workspace view (4th ActivityBar entry) with project list, file tree, and real CLI session history (Claude/Codex/Qoder) with one-click resume.

**Architecture:** Rust backend scans 3 CLI storage formats and exposes unified `SessionInfo[]` via Tauri commands. Frontend reuses existing `FileTree` + `FileContentBlock` components, adds `SessionList` for session history, and wires a new `ProjectDetail` view into the ActivityBar system.

**Tech Stack:** Rust (serde, serde_json, serde_yaml, std::fs), TypeScript (React + Tauri invoke), existing xterm.js PTY system

---

### Task 1: Extend Rust data model — ProjectInfo + .ai-profile

**Files:**
- Modify: `desktop/src-tauri/src/project_manager.rs` (full file)
- Modify: `desktop/src-tauri/src/lib.rs` (register new commands)
- Modify: `desktop/src/lib/types.ts` (ProjectInfo extension)

- [ ] **Step 1: Add defaultProfile field to ProjectInfo struct and extend persistence**

Open `desktop/src-tauri/src/project_manager.rs`. Replace the `ProjectInfo` struct and add new types + commands:

```rust
// ── Types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    pub name: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_profile: Option<String>,
}

pub type ProjectList = Vec<ProjectInfo>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub session_id: String,
    pub title: String,
    pub cli: String,              // "claude" | "codex" | "qoder"
    pub profile: Option<String>,  // unknown for external sessions
    pub project_path: String,
    pub work_dir: String,
    pub timestamp: u64,           // unix epoch ms
    pub status: String,           // "active" | "ended"
}
```

After the `remove_project` function, append these new commands:

```rust
// ── Update project ─────────────────────────────────────────────

#[tauri::command]
pub fn update_project(name: String, new_name: Option<String>, new_path: Option<String>, default_profile: Option<String>) -> Result<(), String> {
    let mut projects = load_projects();
    let idx = projects.iter().position(|p| p.name == name)
        .ok_or_else(|| format!("项目 '{}' 不存在", name))?;

    if let Some(ref nn) = new_name {
        if nn != &name && projects.iter().any(|p| p.name == *nn) {
            return Err(format!("项目 '{}' 已存在", nn));
        }
        validate_project_name(nn)?;
        projects[idx].name = nn.clone();
    }
    if let Some(ref np) = new_path {
        validate_project_path(np)?;
        projects[idx].path = np.clone();
    }
    // default_profile: Some("...") = set, Some("") = clear, None = don't change
    if let Some(dp) = default_profile {
        if dp.is_empty() {
            projects[idx].default_profile = None;
        } else {
            projects[idx].default_profile = Some(dp);
        }
    }
    save_projects(&projects)?;

    // Sync .ai-profile file
    if default_profile.is_some() {
        let _ = write_ai_profile_file(&projects[idx].path, projects[idx].default_profile.as_deref());
    }
    Ok(())
}

// ── .ai-profile file helpers ──────────────────────────────────

fn ai_profile_path(project_path: &str) -> PathBuf {
    Path::new(project_path).join(".ai-profile")
}

fn write_ai_profile_file(project_path: &str, default_profile: Option<&str>) -> Result<(), String> {
    let path = ai_profile_path(project_path);
    match default_profile {
        Some(p) if !p.is_empty() => {
            let content = format!("default_profile: {}\n", p);
            fs::write(&path, &content).map_err(|e| format!("写入 .ai-profile 失败: {}", e))?;
        }
        _ => {
            if path.exists() {
                fs::remove_file(&path).map_err(|e| format!("删除 .ai-profile 失败: {}", e))?;
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub fn write_ai_profile(project_path: String, default_profile: String) -> Result<(), String> {
    write_ai_profile_file(&project_path, Some(&default_profile))
}

#[tauri::command]
pub fn read_ai_profile(project_path: String) -> Result<Option<String>, String> {
    let path = ai_profile_path(&project_path);
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&path).map_err(|e| format!("读取 .ai-profile 失败: {}", e))?;
    // Parse the simple YAML: "default_profile: <name>"
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(val) = trimmed.strip_prefix("default_profile:") {
            let profile = val.trim().trim_matches('"').trim_matches('\'');
            if profile.is_empty() { return Ok(None); }
            return Ok(Some(profile.to_string()));
        }
    }
    Ok(None)
}
```

- [ ] **Step 2: Register new commands in lib.rs**

Open `desktop/src-tauri/src/lib.rs`. In the `invoke_handler` macro (around line 286-289), add the new commands after `project_manager::remove_project`:

```rust
            project_manager::list_projects,
            project_manager::add_project,
            project_manager::remove_project,
            project_manager::update_project,
            project_manager::write_ai_profile,
            project_manager::read_ai_profile,
```

- [ ] **Step 3: Update TypeScript types**

Open `desktop/src/lib/types.ts`. Extend `ProjectInfo` and add `SessionInfo`:

```typescript
// ── Project Management ──

export interface ProjectInfo {
  name: string;
  path: string;
  defaultProfile?: string;
}

// ── Session (CLI native) ──

export interface SessionInfo {
  sessionId: string;
  title: string;
  cli: "claude" | "codex" | "qoder";
  profile: string | null;
  projectPath: string;
  workDir: string;
  timestamp: number;
  status: "active" | "ended";
}
```

- [ ] **Step 4: Build and verify Rust compiles**

```bash
cd desktop/src-tauri && cargo check 2>&1
```
Expected: `Finished` with no errors.

- [ ] **Step 5: TypeScript type check**

```bash
cd desktop && npx tsc --noEmit 2>&1
```
Expected: no new type errors (may have pre-existing ones unrelated to our changes).

- [ ] **Step 6: Commit**

```bash
git add desktop/src-tauri/src/project_manager.rs desktop/src-tauri/src/lib.rs desktop/src/lib/types.ts
git commit -m "feat: extend ProjectInfo with defaultProfile, add update/write_ai_profile/read_ai_profile commands"
```

---

### Task 2: Rust — Claude session scanner (filesystem)

**Files:**
- Modify: `desktop/src-tauri/src/project_manager.rs`
- Modify: `desktop/src-tauri/src/lib.rs`

- [ ] **Step 1: Add Claude session scanning function**

In `project_manager.rs`, after the `.ai-profile` commands, add:

```rust
// ── Session Scanning ───────────────────────────────────────────

/// Encode an absolute path for Claude/Qoder project directory naming:
/// "/Users/xxx/my-project" → "-Users-xxx-my-project"
fn encode_project_path(path: &str) -> String {
    let cleaned = path.trim_start_matches('/'); // strip leading /
    format!("-{}", cleaned.replace('/', "-"))
}

/// Scan Claude Code sessions for a given project path.
fn scan_claude_sessions(project_path: &str) -> Vec<SessionInfo> {
    let home = crate::home_dir();
    let encoded = encode_project_path(project_path);
    let sessions_dir = home.join(".claude").join("projects").join(&encoded);
    if !sessions_dir.is_dir() {
        return Vec::new();
    }

    let mut sessions: Vec<SessionInfo> = Vec::new();
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let day_ms: u64 = 24 * 60 * 60 * 1000;

    if let Ok(entries) = fs::read_dir(&sessions_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            // Look for .jsonl files (transcripts) — skip directories and non-jsonl
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext != "jsonl" {
                continue;
            }
            // Extract UUID from filename stem
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let session_id = stem.to_string();

            // Read metadata from PID.json (same dir, different file pattern)
            // PID files are like "87204.json" — we match by session_id via reading PID.json
            // Actually, PID files are in ~/.claude/sessions/, not in projects/
            // For project-level sessions, status is inferred from file freshness

            // Extract title: read first user prompt from jsonl
            let title = extract_claude_title(&path).unwrap_or_else(|| "无标题".to_string());

            // Get file modification time as timestamp
            let timestamp = path.metadata().ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);

            // Heuristic: if modified within last hour, consider "active"
            let status = if timestamp > 0 && (now_ms - timestamp) < 3600_000 {
                "active"
            } else {
                "ended"
            }.to_string();

            sessions.push(SessionInfo {
                session_id,
                title,
                cli: "claude".to_string(),
                profile: None,
                project_path: project_path.to_string(),
                work_dir: project_path.to_string(),
                timestamp,
                status,
            });
        }
    }
    sessions.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    sessions
}

/// Extract the first user prompt from a Claude jsonl transcript file.
fn extract_claude_title(jsonl_path: &Path) -> Option<String> {
    let content = fs::read_to_string(jsonl_path).ok()?;
    // Claude jsonl: each line is a JSON object with "role" and "content" fields.
    // First user message (role: "user") is the title.
    for line in content.lines().take(50) {
        // Quick substring check to avoid full JSON parse when possible
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            if v.get("role").and_then(|r| r.as_str()) == Some("user") {
                if let Some(content) = v.get("content").and_then(|c| c.as_str()) {
                    let title = content.trim();
                    if !title.is_empty() {
                        // Truncate to 80 chars
                        let short: String = title.chars().take(80).collect();
                        return Some(short);
                    }
                } else if let Some(parts) = v.get("content").and_then(|c| c.as_array()) {
                    // Content might be an array of blocks — get first text block
                    for part in parts {
                        if part.get("type").and_then(|t| t.as_str()) == Some("text") {
                            if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                                let title = text.trim();
                                if !title.is_empty() {
                                    let short: String = title.chars().take(80).collect();
                                    return Some(short);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}
```

- [ ] **Step 2: Add Codex session scanning function**

Append to `project_manager.rs`:

```rust
/// Scan Codex sessions from ~/.codex/session_index.jsonl.
fn scan_codex_sessions(project_path: &str) -> Vec<SessionInfo> {
    let home = crate::home_dir();
    let index_path = home.join(".codex").join("session_index.jsonl");
    if !index_path.exists() {
        return Vec::new();
    }
    let content = match fs::read_to_string(&index_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let mut sessions: Vec<SessionInfo> = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
            let id = v.get("id").and_then(|i| i.as_str()).unwrap_or("");
            let thread_name = v.get("thread_name").and_then(|t| t.as_str()).unwrap_or("无标题");
            let updated_at = v.get("updated_at").and_then(|t| t.as_str()).unwrap_or("");
            // Codex session_index.jsonl doesn't directly store workDir per entry.
            // We include all sessions; frontend can filter if needed.
            // For now, include them since the index is per-user, not per-project.
            let timestamp = parse_iso8601_to_ms(updated_at).unwrap_or(0);
            let title: String = thread_name.chars().take(80).collect();

            sessions.push(SessionInfo {
                session_id: id.to_string(),
                title,
                cli: "codex".to_string(),
                profile: None,
                project_path: project_path.to_string(),
                work_dir: String::new(),
                timestamp,
                status: "ended".to_string(), // Codex doesn't expose active status in index
            });
        }
    }
    sessions.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    sessions
}

fn parse_iso8601_to_ms(s: &str) -> Option<u64> {
    // Simple parse: "2026-05-06T03:59:17.32363Z"
    // Use chrono if available; otherwise rough parse
    let cleaned = s.replace('T', " ").replace('Z', "");
    // Approximate: parse date parts
    let parts: Vec<&str> = cleaned.split(|c: char| c == '-' || c == ':' || c == ' ' || c == '.').collect();
    if parts.len() < 6 { return None; }
    let year: i64 = parts[0].parse().ok()?;
    let month: u32 = parts[1].parse().ok()?;
    let day: u32 = parts[2].parse().ok()?;
    let hour: u32 = parts[3].parse().ok()?;
    let min: u32 = parts[4].parse().ok()?;
    let sec: u32 = parts[5].parse().ok()?;
    // Use a simple day-count approach for Unix timestamp
    let days_before_year = |y: i64| -> i64 {
        let y = y - 1;
        365 * y + y / 4 - y / 100 + y / 400
    };
    let days_in_month = |m: u32, leap: bool| -> i64 {
        match m {
            1 => 31, 2 => if leap { 29 } else { 28 }, 3 => 31, 4 => 30,
            5 => 31, 6 => 30, 7 => 31, 8 => 31, 9 => 30, 10 => 31,
            11 => 30, 12 => 31, _ => 0,
        }
    };
    let is_leap = |y: i64| -> bool { (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 };
    let epoch_days = days_before_year(1970);
    let total_days = days_before_year(year) - epoch_days
        + (1..month).map(|m| days_in_month(m, is_leap(year))).sum::<i64>()
        + (day as i64) - 1;
    let total_secs = total_days * 86400 + hour as i64 * 3600 + min as i64 * 60 + sec as i64;
    Some(total_secs.max(0) as u64 * 1000)
}
```

- [ ] **Step 3: Add Qoder session scanning function**

Append to `project_manager.rs`:

```rust
/// Scan Qoder sessions — CLI command first, filesystem fallback.
fn scan_qoder_sessions(project_path: &str) -> Vec<SessionInfo> {
    // Try CLI command first
    if let Some(sessions) = scan_qoder_cli(project_path) {
        if !sessions.is_empty() {
            return sessions;
        }
    }
    // Fallback: filesystem scan (same format as Claude)
    scan_qoder_filesystem(project_path)
}

fn scan_qoder_cli(project_path: &str) -> Option<Vec<SessionInfo>> {
    let binary = crate::commands::find_binary("qoderclicn");
    let output = std::process::Command::new(binary)
        .args(["--list-sessions"])
        .current_dir(project_path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_qoder_list_output(&stdout, project_path)
}

/// Parse qoderclicn --list-sessions output:
/// "  1. 帮我检查下 终端分屏 (11 hours ago) [f2b8d3d2-...]"
fn parse_qoder_list_output(output: &str, project_path: &str) -> Option<Vec<SessionInfo>> {
    let mut sessions = Vec::new();
    // Regex: index. title (time_ago) [uuid]
    let re = regex_lite::Regex::new(
        r"^\s*(\d+)\.\s+(.+?)\s+\((.+?)\)\s+\[([a-f0-9-]+)\]"
    ).ok()?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    for line in output.lines() {
        if let Some(caps) = re.captures(line) {
            let session_id = caps.get(4).map(|m| m.as_str()).unwrap_or("").to_string();
            let title: String = caps.get(2).map(|m| m.as_str()).unwrap_or("无标题")
                .chars().take(80).collect();
            let time_ago = caps.get(3).map(|m| m.as_str()).unwrap_or("");
            let timestamp = parse_qoder_time_ago(time_ago, now);

            sessions.push(SessionInfo {
                session_id,
                title,
                cli: "qoder".to_string(),
                profile: None,
                project_path: project_path.to_string(),
                work_dir: project_path.to_string(),
                timestamp,
                status: "ended".to_string(),
            });
        }
    }
    if sessions.is_empty() { return None; }
    sessions.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    Some(sessions)
}

fn parse_qoder_time_ago(time_ago: &str, now_ms: u64) -> u64 {
    let ago = time_ago.trim();
    // Parse patterns like "11 hours ago", "3 days ago", "5 minutes ago"
    let parts: Vec<&str> = ago.split_whitespace().collect();
    if parts.len() < 2 { return 0; }
    let num: f64 = parts[0].parse().unwrap_or(0.0);
    let unit = parts[1].to_lowercase();
    let ms_per_unit: f64 = match unit.as_str() {
        "minute" | "minutes" => 60_000.0,
        "hour" | "hours" => 3_600_000.0,
        "day" | "days" => 86_400_000.0,
        "week" | "weeks" => 604_800_000.0,
        "month" | "months" => 2_592_000_000.0,
        _ => 0.0,
    };
    if ms_per_unit == 0.0 { return 0; }
    let elapsed = (num * ms_per_unit) as u64;
    now_ms.saturating_sub(elapsed)
}

fn scan_qoder_filesystem(project_path: &str) -> Vec<SessionInfo> {
    let home = crate::home_dir();
    let encoded = encode_project_path(project_path);
    let sessions_dir = home.join(".qoder-cn").join("projects").join(&encoded);
    if !sessions_dir.is_dir() {
        return Vec::new();
    }
    let mut sessions: Vec<SessionInfo> = Vec::new();
    if let Ok(entries) = fs::read_dir(&sessions_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let session_id = stem.to_string();
            let title = extract_claude_title(&path).unwrap_or_else(|| "无标题".to_string());
            let timestamp = path.metadata().ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            sessions.push(SessionInfo {
                session_id,
                title,
                cli: "qoder".to_string(),
                profile: None,
                project_path: project_path.to_string(),
                work_dir: project_path.to_string(),
                timestamp,
                status: "ended".to_string(),
            });
        }
    }
    sessions.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    sessions
}
```

- [ ] **Step 4: Add unified scan_project_sessions command**

Append to `project_manager.rs`:

```rust
#[tauri::command]
pub fn scan_project_sessions(project_path: String, cli: Option<String>) -> Vec<SessionInfo> {
    let cli_filter = cli.unwrap_or_default();
    let mut all: Vec<SessionInfo> = Vec::new();

    if cli_filter.is_empty() || cli_filter == "claude" {
        all.extend(scan_claude_sessions(&project_path));
    }
    if cli_filter.is_empty() || cli_filter == "codex" {
        all.extend(scan_codex_sessions(&project_path));
    }
    if cli_filter.is_empty() || cli_filter == "qoder" {
        all.extend(scan_qoder_sessions(&project_path));
    }

    all.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    all
}
```

- [ ] **Step 5: Add regex_lite dependency**

```bash
cd desktop/src-tauri && cargo add regex-lite 2>&1
```
Expected: adds to Cargo.toml.

- [ ] **Step 6: Register scan_project_sessions in lib.rs**

In `desktop/src-tauri/src/lib.rs`, add after the other project_manager commands:

```rust
            project_manager::update_project,
            project_manager::write_ai_profile,
            project_manager::read_ai_profile,
            project_manager::scan_project_sessions,
```

- [ ] **Step 7: Make find_binary public in commands.rs**

Open `desktop/src-tauri/src/commands.rs`. Find the `find_binary` function and change its visibility:

```rust
pub(crate) fn find_binary(name: &str) -> String {
```

- [ ] **Step 8: Build and fix errors**

```bash
cd desktop/src-tauri && cargo check 2>&1
```
Expected: `Finished` with no errors. If regex_lite API differs, adjust.

- [ ] **Step 9: Commit**

```bash
git add desktop/src-tauri/src/project_manager.rs desktop/src-tauri/src/lib.rs desktop/src-tauri/src/commands.rs desktop/src-tauri/Cargo.toml desktop/src-tauri/Cargo.lock
git commit -m "feat: add scan_project_sessions with Claude/Codex/Qoder support"
```

---

### Task 3: Frontend — ActivityBar + useProjects extension

**Files:**
- Modify: `desktop/src/components/ActivityBar.tsx`
- Modify: `desktop/src/hooks/useProjects.ts`
- Modify: `desktop/src/lib/types.ts` (already done in Task 1)

- [ ] **Step 1: Add "project" to ActivityBar**

Open `desktop/src/components/ActivityBar.tsx`. Update the type and activities:

```typescript
import React from "react";
import { Layers, Blocks, Webhook, Folder, type LucideIcon } from "lucide-react";

export type ActivityKey = "profile" | "skills" | "hooks" | "projects";

interface ActivityItem {
  key: ActivityKey;
  icon: LucideIcon;
  label: string;
}

const ACTIVITIES: ActivityItem[] = [
  { key: "profile", icon: Layers, label: "Profiles" },
  { key: "skills", icon: Blocks, label: "Skills & Plugins" },
  { key: "hooks", icon: Webhook, label: "Hooks" },
  { key: "projects", icon: Folder, label: "Projects" },
];
```

- [ ] **Step 2: Extend useProjects hook**

Open `desktop/src/hooks/useProjects.ts`. Add new methods:

```typescript
import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ProjectInfo } from "../lib/types";

const STORAGE_KEY = "kn-active-project";

export interface UseProjectsReturn {
  projects: ProjectInfo[];
  activeProject: ProjectInfo | null;
  loading: boolean;
  setActiveProject: (project: ProjectInfo | null) => void;
  loadProjects: () => Promise<void>;
  addProject: (name: string, path: string) => Promise<void>;
  removeProject: (name: string) => Promise<void>;
  updateProject: (name: string, newName?: string, newPath?: string, defaultProfile?: string) => Promise<void>;
  setDefaultProfile: (projectName: string, profile: string | null) => Promise<void>;
}

export function useProjects(): UseProjectsReturn {
  // ... keep existing code ...

  const updateProject = useCallback(async (name: string, newName?: string, newPath?: string, defaultProfile?: string) => {
    await invoke("update_project", {
      name,
      newName: newName ?? null,
      newPath: newPath ?? null,
      defaultProfile: defaultProfile ?? null,
    });
    await loadProjects();
  }, [loadProjects]);

  const setDefaultProfile = useCallback(async (projectName: string, profile: string | null) => {
    await invoke("update_project", {
      name: projectName,
      newName: null,
      newPath: null,
      defaultProfile: profile ?? "",
    });
    await loadProjects();
  }, [loadProjects]);

  return {
    projects,
    activeProject,
    loading,
    setActiveProject,
    loadProjects,
    addProject,
    removeProject,
    updateProject,
    setDefaultProfile,
  };
}
```

- [ ] **Step 3: TypeScript check**

```bash
cd desktop && npx tsc --noEmit 2>&1
```
Expected: no new errors.

- [ ] **Step 4: Commit**

```bash
git add desktop/src/components/ActivityBar.tsx desktop/src/hooks/useProjects.ts
git commit -m "feat: add projects entry to ActivityBar, extend useProjects with update/setDefaultProfile"
```

---

### Task 4: Frontend — ProjectSidebar component

**Files:**
- Create: `desktop/src/components/ProjectSidebar.tsx`

- [ ] **Step 1: Create ProjectSidebar component**

Write `desktop/src/components/ProjectSidebar.tsx`:

```typescript
import React, { useState, useCallback, useRef, useEffect } from "react";
import { SearchInput } from "./common/SearchInput";
import { ContextMenu } from "./ContextMenu";
import { Folder, Trash2, Pencil, FolderOpen } from "lucide-react";
import type { ProjectInfo } from "../lib/types";

interface ProjectSidebarProps {
  projects: ProjectInfo[];
  selectedProject: ProjectInfo | null;
  onSelect: (project: ProjectInfo | null) => void;
  onAddProject: () => void;
  onDeleteProject: (name: string) => void;
  onRenameProject: (name: string) => void;
  /** Called when user wants to change project path */
  onChangePath: (name: string) => void;
}

export function ProjectSidebar({
  projects,
  selectedProject,
  onSelect,
  onAddProject,
  onDeleteProject,
  onRenameProject,
  onChangePath,
}: ProjectSidebarProps) {
  const [search, setSearch] = useState("");
  const [ctxMenu, setCtxMenu] = useState<{ x: number; y: number; name: string } | null>(null);

  const onContextMenu = useCallback((e: React.MouseEvent, name: string) => {
    e.preventDefault();
    setCtxMenu({ x: e.clientX, y: e.clientY, name });
  }, []);

  const filtered = search
    ? projects.filter((p) => p.name.toLowerCase().includes(search.toLowerCase()))
    : projects;

  return (
    <div className="w-[300px] shrink-0 flex flex-col bg-app-sidebar border-r border-app-border select-none">
      {/* Header */}
      <div className="px-2.5 pt-2.5 pb-2">
        <div className="flex items-center gap-1.5 mb-2.5">
          <Folder size={13} className="text-[var(--app-amber)] shrink-0" />
          <span className="text-2xs text-[var(--app-text)] font-mono tracking-[0.15em] uppercase flex-1">
            项目
          </span>
        </div>
        <SearchInput value={search} onChange={setSearch} placeholder="搜索项目..." />
      </div>
      <div className="mx-2.5 border-b border-app-border-light" />

      {/* Project list */}
      <div className="flex-1 overflow-y-auto overflow-x-hidden py-0.5">
        {filtered.length === 0 && (
          <div className="flex flex-col items-center justify-center h-full gap-3 px-4 text-center">
            <Folder size={22} className="text-app-text-muted opacity-25" />
            <div className="text-xs text-app-text-dim">暂无项目</div>
            <div className="text-2xs text-app-text-muted leading-relaxed">
              注册一个项目目录开始
            </div>
          </div>
        )}
        {filtered.map((p) => {
          const isSelected = p.name === selectedProject?.name;
          return (
            <div
              key={p.name}
              onClick={() => onSelect(isSelected ? null : p)}
              onContextMenu={(e) => onContextMenu(e, p.name)}
              className={`group flex flex-col mx-1 my-px px-2.5 py-1.5 cursor-pointer
                transition-all duration-fast
                ${isSelected
                  ? "bg-app-selected text-app-text border-l-[3px] border-l-app-amber shadow-[inset_0_0_8px_var(--app-glow)]"
                  : "text-app-text border-l-[3px] border-l-transparent hover:bg-app-hover active:bg-app-active"
                }`}
            >
              <div className="flex items-center gap-2">
                {isSelected
                  ? <FolderOpen size={14} className="text-[var(--app-amber)] shrink-0" />
                  : <Folder size={14} className="text-[var(--app-text-muted)] shrink-0 group-hover:text-[var(--app-amber)]" />
                }
                <span className={`truncate text-sm font-mono ${isSelected ? "font-medium" : "font-normal"}`}>
                  {p.name}
                </span>
              </div>
              {p.defaultProfile && (
                <div className="flex items-center gap-1 mt-0.5 ml-6">
                  <span className="text-3xs text-[var(--app-text-muted)] font-mono">默认:</span>
                  <span className="text-3xs text-[var(--app-amber)] font-mono bg-[var(--app-amber-bg)] px-1 rounded">
                    {p.defaultProfile}
                  </span>
                </div>
              )}
            </div>
          );
        })}
      </div>

      {/* Add project button */}
      <div className="p-2 border-t border-app-border">
        <button
          onClick={onAddProject}
          className="w-full flex items-center justify-center gap-1.5 px-2 py-1.5
            text-2xs font-mono text-[var(--app-accent-dim)] hover:text-[var(--app-accent)]
            hover:bg-[var(--app-hover)] border border-dashed border-[var(--app-border)]
            hover:border-[var(--app-accent)] transition-all duration-100 cursor-pointer"
        >
          <span>+</span>
          <span>注册新项目...</span>
        </button>
      </div>

      {/* Context menu */}
      {ctxMenu && (
        <ContextMenu
          x={ctxMenu.x} y={ctxMenu.y}
          onClose={() => setCtxMenu(null)}
          items={[
            {
              label: "修改名称",
              icon: <Pencil size={13} />,
              onClick: () => { onRenameProject(ctxMenu.name); setCtxMenu(null); },
            },
            {
              label: "修改路径",
              icon: <FolderOpen size={13} />,
              onClick: () => { onChangePath(ctxMenu.name); setCtxMenu(null); },
            },
            {
              label: "删除项目",
              icon: <Trash2 size={13} />,
              onClick: () => { onDeleteProject(ctxMenu.name); setCtxMenu(null); },
              danger: true,
            },
          ]}
        />
      )}
    </div>
  );
}
```

- [ ] **Step 2: TypeScript check**

```bash
cd desktop && npx tsc --noEmit 2>&1
```
Expected: no new errors.

- [ ] **Step 3: Commit**

```bash
git add desktop/src/components/ProjectSidebar.tsx
git commit -m "feat: add ProjectSidebar component with search, right-click menu"
```

---

### Task 5: Frontend — useSessionScanner hook

**Files:**
- Create: `desktop/src/hooks/useSessionScanner.ts`

- [ ] **Step 1: Create useSessionScanner hook**

Write `desktop/src/hooks/useSessionScanner.ts`:

```typescript
import { useState, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { SessionInfo } from "../lib/types";

const CACHE_TTL_MS = 30_000; // 30s cache

interface CacheEntry {
  data: SessionInfo[];
  projectPath: string;
  timestamp: number;
}

export interface UseSessionScannerReturn {
  sessions: SessionInfo[];
  loading: boolean;
  scanSessions: (projectPath: string) => Promise<void>;
}

export function useSessionScanner(): UseSessionScannerReturn {
  const [sessions, setSessions] = useState<SessionInfo[]>([]);
  const [loading, setLoading] = useState(false);
  const cacheRef = useRef<CacheEntry | null>(null);

  const scanSessions = useCallback(async (projectPath: string) => {
    // Check cache
    const cache = cacheRef.current;
    if (cache && cache.projectPath === projectPath &&
        Date.now() - cache.timestamp < CACHE_TTL_MS) {
      setSessions(cache.data);
      return;
    }

    setLoading(true);
    try {
      const results = await invoke<SessionInfo[]>("scan_project_sessions", {
        projectPath,
        cli: null, // scan all CLIs
      });
      setSessions(results);
      cacheRef.current = { data: results, projectPath, timestamp: Date.now() };
    } catch (e) {
      console.error("[useSessionScanner] scan failed:", e);
      setSessions([]);
    } finally {
      setLoading(false);
    }
  }, []);

  return { sessions, loading, scanSessions };
}
```

- [ ] **Step 2: TypeScript check**

```bash
cd desktop && npx tsc --noEmit 2>&1
```
Expected: no new errors.

- [ ] **Step 3: Commit**

```bash
git add desktop/src/hooks/useSessionScanner.ts
git commit -m "feat: add useSessionScanner hook with 30s cache"
```

---

### Task 6: Frontend — SessionList component

**Files:**
- Create: `desktop/src/components/SessionList.tsx`

- [ ] **Step 1: Create SessionList component**

Write `desktop/src/components/SessionList.tsx`:

```typescript
import React from "react";
import { CLIIcon } from "./common/CLIIcon";
import { Circle, Loader } from "lucide-react";
import type { SessionInfo } from "../lib/types";

interface SessionListProps {
  sessions: SessionInfo[];
  loading: boolean;
  onResume: (session: SessionInfo) => void;
}

function relativeTime(ts: number): string {
  const now = Date.now();
  const diff = now - ts;
  const seconds = Math.floor(diff / 1000);
  if (seconds < 60) return "刚刚";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}分钟前`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) {
    const date = new Date(ts);
    const hh = date.getHours().toString().padStart(2, "0");
    const mm = date.getMinutes().toString().padStart(2, "0");
    return `今天 ${hh}:${mm}`;
  }
  const days = Math.floor(hours / 24);
  if (days === 1) return "昨天";
  if (days < 7) return `${days}天前`;
  const date = new Date(ts);
  const m = (date.getMonth() + 1).toString().padStart(2, "0");
  const d = date.getDate().toString().padStart(2, "0");
  return `${m}-${d}`;
}

export function SessionList({ sessions, loading, onResume }: SessionListProps) {
  if (loading) {
    return (
      <div className="flex items-center justify-center py-12 gap-2 text-[var(--app-text-muted)]">
        <Loader size={14} className="animate-spin" />
        <span className="text-2xs font-mono">扫描会话...</span>
      </div>
    );
  }

  if (sessions.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center py-12 gap-2 text-[var(--app-text-muted)]">
        <span className="text-xs font-mono">暂无会话</span>
        <span className="text-3xs font-mono">在终端中运行 AI CLI 后刷新</span>
      </div>
    );
  }

  return (
    <div className="py-1">
      {sessions.map((s) => (
        <div
          key={s.sessionId}
          onClick={() => onResume(s)}
          className="group flex items-center gap-2.5 mx-1 my-px px-2.5 py-2
            cursor-pointer text-[var(--app-text)] hover:bg-[var(--app-hover)]
            border-l-[3px] border-l-transparent hover:border-l-[var(--app-accent)]
            transition-all duration-fast"
        >
          {/* Status indicator */}
          <Circle
            size={7}
            className={`shrink-0 ${
              s.status === "active"
                ? "fill-[var(--app-green)] text-[var(--app-green)]"
                : "fill-transparent text-[var(--app-text-muted)]"
            }`}
          />

          {/* CLI type icon */}
          <CLIIcon type={s.cli} size={14} />

          {/* Title + meta */}
          <div className="flex-1 min-w-0">
            <div className="text-xs font-mono truncate">
              {s.title}
            </div>
            <div className="flex items-center gap-1.5 mt-0.5">
              <span className="text-3xs text-[var(--app-text-muted)] font-mono">
                {s.cli}
              </span>
              {s.profile && (
                <>
                  <span className="text-3xs text-[var(--app-border)]">·</span>
                  <span className="text-3xs text-[var(--app-amber)] font-mono">
                    {s.profile}
                  </span>
                </>
              )}
              <span className="text-3xs text-[var(--app-border)]">·</span>
              <span className="text-3xs text-[var(--app-text-muted)] font-mono">
                {relativeTime(s.timestamp)}
              </span>
            </div>
          </div>

          {/* Resume hint on hover */}
          <span className="text-3xs text-[var(--app-accent-dim)] opacity-0 group-hover:opacity-100 transition-opacity font-mono shrink-0">
            恢复 →
          </span>
        </div>
      ))}
    </div>
  );
}
```

- [ ] **Step 2: TypeScript check**

```bash
cd desktop && npx tsc --noEmit 2>&1
```
Expected: no new errors.

- [ ] **Step 3: Commit**

```bash
git add desktop/src/components/SessionList.tsx
git commit -m "feat: add SessionList component with status indicator and relative time"
```

---

### Task 7: Frontend — ProjectDetail component

**Files:**
- Create: `desktop/src/components/ProjectDetail.tsx`

- [ ] **Step 1: Create ProjectDetail component**

Write `desktop/src/components/ProjectDetail.tsx`:

```typescript
import React, { useState, useCallback, useEffect } from "react";
import { FileTree, type FileTreeNode } from "./FileTree";
import { FileContentBlock } from "./common/FileContentBlock";
import { SessionList } from "./SessionList";
import { Folder, FolderOpen, Terminal, ExternalLink, ChevronDown, Play } from "lucide-react";
import type { ProjectInfo, SessionInfo, ProfileSummary } from "../lib/types";
import { invoke } from "@tauri-apps/api/core";

interface ProjectDetailProps {
  project: ProjectInfo;
  profiles: ProfileSummary[];
  sessions: SessionInfo[];
  sessionsLoading: boolean;
  onResumeSession: (session: SessionInfo) => void;
  onRunProfile: (profileName: string, cliType: string) => void;
  onScanSessions: (projectPath: string) => void;
}

type DetailTab = "files" | "sessions";

export function ProjectDetail({
  project,
  profiles,
  sessions,
  sessionsLoading,
  onResumeSession,
  onRunProfile,
  onScanSessions,
}: ProjectDetailProps) {
  const [tab, setTab] = useState<DetailTab>("files");
  const [selectedFile, setSelectedFile] = useState<FileTreeNode | null>(null);
  const [fileContent, setFileContent] = useState<string>("");
  const [showProfilePicker, setShowProfilePicker] = useState(false);
  const [pickerPosition, setPickerPosition] = useState({ x: 0, y: 0 });

  // Scan sessions when project changes
  useEffect(() => {
    onScanSessions(project.path);
  }, [project.path]);

  // Load file content when selected file changes
  useEffect(() => {
    if (!selectedFile || selectedFile.is_dir) {
      setFileContent("");
      return;
    }
    invoke<string>("read_file", { path: selectedFile.path })
      .then(setFileContent)
      .catch(() => setFileContent(""));
  }, [selectedFile]);

  const handleRunClick = useCallback((e: React.MouseEvent) => {
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    setPickerPosition({ x: rect.left, y: rect.bottom + 4 });
    setShowProfilePicker((v) => !v);
  }, []);

  const handleSelectProfile = useCallback((profile: ProfileSummary) => {
    const cliType = profile.cli_type || "claude";
    onRunProfile(profile.name, cliType);
    setShowProfilePicker(false);
  }, [onRunProfile]);

  const handleOpenInTerminal = useCallback(() => {
    invoke("open_in_terminal", { path: project.path }).catch((e) =>
      console.error("open_in_terminal failed:", e)
    );
  }, [project.path]);

  const handleOpenInFinder = useCallback(() => {
    invoke("open_file", { path: project.path }).catch((e) =>
      console.error("open_file failed:", e)
    );
  }, [project.path]);

  const handleCopyPath = useCallback(() => {
    navigator.clipboard.writeText(project.path).catch(() => {});
  }, [project.path]);

  return (
    <div className="flex-1 flex flex-col bg-[var(--app-bg)] overflow-hidden">
      {/* Header */}
      <div className="flex items-center gap-2 px-3 py-2 border-b border-[var(--app-border)] shrink-0">
        <FolderOpen size={16} className="text-[var(--app-amber)] shrink-0" />
        <span className="text-sm font-mono font-medium text-[var(--app-text)] truncate">
          {project.name}
        </span>
        <span
          onClick={handleCopyPath}
          className="text-3xs font-mono text-[var(--app-text-muted)] truncate cursor-pointer hover:text-[var(--app-text-dim)] ml-1"
          title={project.path}
        >
          {project.path}
        </span>

        <div className="flex-1" />

        {/* Run button — split button style */}
        <div className="flex items-stretch">
          {/* Main run button */}
          <button
            onClick={handleRunClick}
            className="flex items-center gap-1 px-2 py-1 text-2xs font-mono
              bg-[var(--app-accent)] text-white hover:opacity-90
              border-none outline-none cursor-pointer
              transition-opacity duration-100"
          >
            <Play size={11} className="shrink-0" />
            <span>运行</span>
          </button>
          {/* Dropdown arrow */}
          <button
            onClick={handleRunClick}
            className="flex items-center px-1 py-1 text-2xs font-mono
              bg-[var(--app-accent)] text-white hover:opacity-90
              border-l border-white/20 border-none outline-none cursor-pointer
              transition-opacity duration-100"
          >
            <ChevronDown size={11} />
          </button>
        </div>

        <button
          onClick={handleOpenInTerminal}
          className="p-1 text-[var(--app-text-muted)] hover:text-[var(--app-text)] transition-colors"
          title="在终端中打开"
        >
          <Terminal size={14} />
        </button>
        <button
          onClick={handleOpenInFinder}
          className="p-1 text-[var(--app-text-muted)] hover:text-[var(--app-text)] transition-colors"
          title="在 Finder 中打开"
        >
          <ExternalLink size={14} />
        </button>
      </div>

      {/* Tabs */}
      <div className="flex items-center border-b border-[var(--app-border)] shrink-0 px-2">
        {(["files", "sessions"] as DetailTab[]).map((t) => (
          <button
            key={t}
            onClick={() => setTab(t)}
            className={`px-3 py-1.5 text-2xs font-mono transition-colors
              ${tab === t
                ? "text-[var(--app-accent)] border-b border-[var(--app-accent)]"
                : "text-[var(--app-text-muted)] hover:text-[var(--app-text)]"
              }`}
          >
            {t === "files" ? "文件" : "会话"}
          </button>
        ))}
      </div>

      {/* Tab content */}
      {tab === "files" ? (
        <div className="flex-1 flex overflow-hidden min-h-0">
          <div className="w-[220px] shrink-0 border-r border-[var(--app-border)] overflow-y-auto">
            <FileTree
              rootPath={project.path}
              onSelect={setSelectedFile}
              activePath={selectedFile?.path}
            />
          </div>
          <div className="flex-1 overflow-y-auto">
            {selectedFile && !selectedFile.is_dir ? (
              <FileContentBlock
                content={fileContent}
                filePath={selectedFile.path}
              />
            ) : (
              <div className="flex items-center justify-center h-full text-xs text-[var(--app-text-muted)] font-mono">
                选择文件以预览
              </div>
            )}
          </div>
        </div>
      ) : (
        <div className="flex-1 overflow-y-auto">
          <SessionList
            sessions={sessions}
            loading={sessionsLoading}
            onResume={onResumeSession}
          />
        </div>
      )}

      {/* Profile picker dropdown */}
      {showProfilePicker && (
        <div
          className="fixed z-50 bg-[var(--app-panel)] border border-[var(--app-border)] shadow-lg min-w-[200px] max-h-[300px] overflow-y-auto"
          style={{ left: pickerPosition.x, top: pickerPosition.y }}
        >
          <div className="px-2 py-1 text-3xs text-[var(--app-text-muted)] font-mono uppercase">
            选择 Profile
          </div>
          <div className="border-t border-[var(--app-border)]" />
          {project.defaultProfile && (
            <>
              <button
                onClick={() => {
                  const p = profiles.find((pr) => pr.name === project.defaultProfile);
                  if (p) handleSelectProfile(p);
                }}
                className="w-full text-left px-3 py-1.5 text-2xs font-mono text-[var(--app-amber)] hover:bg-[var(--app-hover)] transition-colors flex items-center gap-2"
              >
                <span>⭐</span>
                <span>{project.defaultProfile}</span>
                <span className="text-3xs text-[var(--app-text-muted)]">默认</span>
              </button>
              <div className="border-t border-[var(--app-border)]" />
            </>
          )}
          {profiles.map((p) => (
            <button
              key={p.name}
              onClick={() => handleSelectProfile(p)}
              className="w-full text-left px-3 py-1.5 text-2xs font-mono text-[var(--app-text)] hover:bg-[var(--app-hover)] transition-colors flex items-center gap-2"
            >
              <span className={p.is_default ? "text-[var(--app-accent)]" : ""}>
                {p.name}
              </span>
              {p.is_default && (
                <span className="text-3xs text-[var(--app-text-muted)]">全局默认</span>
              )}
            </button>
          ))}
        </div>
      )}

      {/* Click-outside to close picker */}
      {showProfilePicker && (
        <div
          className="fixed inset-0 z-40"
          onClick={() => setShowProfilePicker(false)}
        />
      )}
    </div>
  );
}
```

- [ ] **Step 2: Check FileContentBlock props**

The existing `FileContentBlock` component uses a `path` prop (from `common/FileContentBlock.tsx`). Verify with:

```bash
grep -n "export function FileContentBlock" desktop/src/components/common/FileContentBlock.tsx
```
Read the function signature to confirm the prop name. Adjust if needed.

- [ ] **Step 3: TypeScript check**

```bash
cd desktop && npx tsc --noEmit 2>&1
```
Expected: no new errors (may need minor prop adjustments for FileContentBlock).

- [ ] **Step 4: Commit**

```bash
git add desktop/src/components/ProjectDetail.tsx
git commit -m "feat: add ProjectDetail with file tree, session list, run button and profile picker"
```

---

### Task 8: Frontend — App.tsx integration

**Files:**
- Modify: `desktop/src/App.tsx`

- [ ] **Step 1: Import new components and hook**

At the top of `App.tsx`, add imports:

```typescript
import { ProjectSidebar } from "./components/ProjectSidebar";
import { ProjectDetail } from "./components/ProjectDetail";
import { useSessionScanner } from "./hooks/useSessionScanner";
```

- [ ] **Step 2: Initialize useSessionScanner**

After the `useProjects()` line (`const { projects, activeProject, ... } = useProjects();`), add:

```typescript
const sessionScanner = useSessionScanner();
```

- [ ] **Step 3: Add project run handler**

After `handleAddProject`, add:

```typescript
const handleRunProjectProfile = useCallback((profileName: string, cliType: string) => {
  if (!activeProject) return;
  const cmd = `ai ${cliType} ${profileName}`;
  rightTerminal.runInNewTab(cmd, activeProject.path, `${activeProject.name} · ${profileName}`);
}, [activeProject, rightTerminal]);

const handleResumeSession = useCallback((session: SessionInfo) => {
  const cmdMap: Record<string, string> = {
    claude: `ai claude ${session.profile || ""} --resume ${session.sessionId}`,
    codex: `ai codex ${session.profile || ""} resume ${session.sessionId}`,
    qoder: `ai qoder ${session.profile || ""} -r ${session.sessionId}`,
  };
  const cmd = cmdMap[session.cli] || `ai claude --resume ${session.sessionId}`;
  const label = session.title.slice(0, 30);
  rightTerminal.runInNewTab(cmd.trim(), session.projectPath, label);
}, [rightTerminal]);

const handleRenameProjectFromSidebar = useCallback((name: string) => {
  setNameDialogTitle("修改项目名称");
  setNameDialogInitial(name);
  setNameDialogOnConfirm(() => async (newName: string) => {
    if (newName === name) return;
    await updateProject(name, newName);
    addToast("success", `项目已重命名为 "${newName}"`);
  });
  setShowNameDialog(true);
}, [updateProject, addToast]);

const handleChangeProjectPath = useCallback(async (name: string) => {
  try {
    const path = await tauriOpen({ directory: true, multiple: false, title: "选择新项目路径" });
    if (!path || typeof path !== "string") return;
    await updateProject(name, undefined, path);
    addToast("success", `项目路径已更新`);
  } catch (e) {
    addToast("error", `修改路径失败: ${String(e).slice(0, 120)}`);
  }
}, [updateProject, addToast]);

const handleDeleteProjectFromSidebar = useCallback(async (name: string) => {
  try {
    await removeProject(name);
    addToast("success", `项目 "${name}" 已删除`);
  } catch (e) {
    addToast("error", `删除失败: ${String(e).slice(0, 120)}`);
  }
}, [removeProject, addToast]);
```

- [ ] **Step 4: Extend App.tsx destructuring of useProjects**

Update the destructuring line (which reads `const { projects, activeProject, setActiveProject, addProject, loadProjects } = useProjects();`) to include new methods:

```typescript
const { projects, activeProject, setActiveProject, addProject, loadProjects, removeProject, updateProject } = useProjects();
```

- [ ] **Step 5: Add projects sidebar and detail to JSX**

In the render section, after the hooks sidebar condition block (`activeActivity === "hooks"`), add:

```tsx
          {sidebarVisible && !rightMaximized && !bottomMaximized && activeActivity === "projects" && (
            <ProjectSidebar
              projects={projects}
              selectedProject={activeProject}
              onSelect={(p) => setActiveProject(p)}
              onAddProject={handleAddProject}
              onDeleteProject={handleDeleteProjectFromSidebar}
              onRenameProject={handleRenameProjectFromSidebar}
              onChangePath={handleChangeProjectPath}
            />
          )}
```

And in the middle column section (where MainPanel renders for non-skills/hooks activities), add a condition for projects:

```tsx
            {!rightMaximized && !bottomMaximized && activeActivity === "projects" && activeProject && (
              <ProjectDetail
                project={activeProject}
                profiles={ctx.profiles}
                sessions={sessionScanner.sessions}
                sessionsLoading={sessionScanner.loading}
                onResumeSession={handleResumeSession}
                onRunProfile={handleRunProjectProfile}
                onScanSessions={(path) => sessionScanner.scanSessions(path)}
              />
            )}
            {!rightMaximized && !bottomMaximized && activeActivity === "projects" && !activeProject && (
              <div className="flex-1 flex items-center justify-center bg-[var(--app-bg)]">
                <div className="text-xs text-[var(--app-text-muted)] font-mono">
                  选择一个项目
                </div>
              </div>
            )}
```

- [ ] **Step 6: TypeScript build check**

```bash
cd desktop && npx tsc --noEmit 2>&1
```
Expected: no new errors. Fix any type mismatches.

- [ ] **Step 7: Full build check**

```bash
cd desktop && npx tsc --noEmit && npx vite build && cd src-tauri && cargo check 2>&1
```
Expected: all three pass.

- [ ] **Step 8: Commit**

```bash
git add desktop/src/App.tsx
git commit -m "feat: wire ProjectSidebar and ProjectDetail into App via ActivityBar"
```

---

### Task 9: Shell wrapper — .ai-profile support

**Files:**
- Modify: `shell/ai-profile.sh`

- [ ] **Step 1: Add .ai-profile reading to ai() function**

Open `shell/ai-profile.sh`. In the `_profile_env()` or `ai()` function, add logic to check for `.ai-profile` in the current working directory for default profile resolution:

```bash
# In ai() function, before parsing $1 as profile name:
# Check for project-level .ai-profile
_ai_profile_project_default() {
    local dir="$PWD"
    while [ "$dir" != "/" ]; do
        if [ -f "$dir/.ai-profile" ]; then
            # Parse "default_profile: <name>" from the file
            local dp=$(sed -n 's/^default_profile:[[:space:]]*//p' "$dir/.ai-profile" | head -1 | tr -d '"'"'"' | xargs)
            if [ -n "$dp" ]; then
                echo "$dp"
                return 0
            fi
        fi
        dir="$(dirname "$dir")"
    done
    return 1
}

# When profile name is omitted (ai claude or ai codex without profile):
if [ -z "$2" ]; then
    # Try project-level default first, then global default
    local project_default=$(_ai_profile_project_default)
    if [ -n "$project_default" ]; then
        set -- "$1" "$project_default"
    fi
fi
```

- [ ] **Step 2: Validate shell syntax**

```bash
bash -n shell/ai-profile.sh
```
Expected: no output (no syntax errors).

- [ ] **Step 3: Commit**

```bash
git add shell/ai-profile.sh
git commit -m "feat: support .ai-profile file for project-level default profile in shell wrapper"
```

---

### Task 10: Integration test — smoke test the full flow

- [ ] **Step 1: Run the desktop app**

```bash
cd desktop && npm run tauri dev 2>&1 &
```

- [ ] **Step 2: Manual verification checklist**

1. Click "projects" in ActivityBar → project sidebar appears
2. Register a new project → select directory → project appears in list
3. Select project → ProjectDetail shows with file tree tab
4. Click "sessions" tab → session scan runs
5. Click "run" button → profile picker appears → select a profile → terminal opens at project dir
6. Click a session → terminal opens with resume command

- [ ] **Step 3: Fix issues as found, then commit any fixes**

---

### Plan Summary

| Task | Component | New/Modify | Estimated Time |
|------|-----------|------------|---------------|
| 1 | Rust data model + .ai-profile | Modify `project_manager.rs`, `lib.rs`, `types.ts` | 20 min |
| 2 | Rust session scanners | Modify `project_manager.rs`, `Cargo.toml` | 30 min |
| 3 | ActivityBar + useProjects | Modify `ActivityBar.tsx`, `useProjects.ts` | 10 min |
| 4 | ProjectSidebar | Create `ProjectSidebar.tsx` | 15 min |
| 5 | useSessionScanner | Create `useSessionScanner.ts` | 10 min |
| 6 | SessionList | Create `SessionList.tsx` | 15 min |
| 7 | ProjectDetail | Create `ProjectDetail.tsx` | 20 min |
| 8 | App.tsx integration | Modify `App.tsx` | 20 min |
| 9 | Shell wrapper | Modify `ai-profile.sh` | 10 min |
| 10 | Smoke test | Manual | 20 min |
| **Total** | | | **~3 hours** |
