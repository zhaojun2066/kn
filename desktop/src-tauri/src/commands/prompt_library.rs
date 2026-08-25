//! Local persistence for the user's prompt library.
//!
//! This deliberately lives outside `config.yaml`: profile writes are performed
//! by more than one client and do not preserve unknown YAML fields.

use crate::{atomic_rename, config_dir, with_write_lock};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::time::{Duration, Instant};

const MAX_PROMPTS: usize = 30;
const MAX_TITLE_CHARS: usize = 80;
const MAX_CONTENT_CHARS: usize = 12_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptTemplate {
    pub uuid: String,
    pub title: String,
    pub content: String,
    pub category: String,
    pub sort_order: i32,
    #[serde(default)]
    pub revision: i64,
    #[serde(default)]
    pub cloud_deleted_locally_retained: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptLibraryState {
    #[serde(default)]
    pub sync_enabled: bool,
    #[serde(default)]
    pub prompts: Vec<PromptTemplate>,
    #[serde(default)]
    pub system_prompts: Vec<PromptTemplate>,
    #[serde(default)]
    pub locally_disabled_uuids: Vec<String>,
}

impl Default for PromptLibraryState {
    fn default() -> Self {
        Self {
            sync_enabled: false,
            prompts: Vec::new(),
            system_prompts: Vec::new(),
            locally_disabled_uuids: Vec::new(),
        }
    }
}

fn library_file() -> std::path::PathBuf {
    config_dir().join("prompt-library.json")
}

fn validate_prompt(prompt: &PromptTemplate) -> Result<(), String> {
    if prompt.uuid.trim().is_empty() || prompt.uuid.len() > 80 {
        return Err("提示词标识无效".into());
    }
    if prompt.title.trim().is_empty() || prompt.title.chars().count() > MAX_TITLE_CHARS {
        return Err(format!("标题不能为空且不能超过 {} 个字符", MAX_TITLE_CHARS));
    }
    if prompt.content.trim().is_empty() || prompt.content.chars().count() > MAX_CONTENT_CHARS {
        return Err(format!(
            "提示词不能为空且不能超过 {} 个字符",
            MAX_CONTENT_CHARS
        ));
    }
    if !matches!(prompt.category.as_str(), "review" | "development" | "other") {
        return Err("提示词分类无效".into());
    }
    Ok(())
}

fn read_state() -> Result<PromptLibraryState, String> {
    let path = library_file();
    if !path.exists() {
        return Ok(PromptLibraryState::default());
    }
    let raw = fs::read_to_string(&path).map_err(|e| format!("读取提示词库失败: {}", e))?;
    let state: PromptLibraryState =
        serde_json::from_str(&raw).map_err(|e| format!("提示词库文件格式无效: {}", e))?;
    if state.prompts.len() > MAX_PROMPTS {
        return Err("提示词库超过最大数量".into());
    }
    for prompt in &state.prompts {
        validate_prompt(prompt)?;
    }
    Ok(state)
}

fn with_library_lock<F, T>(f: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String>,
{
    with_write_lock(|| {
        let lock_path = config_dir().join(".prompt-library.lock");
        fs::create_dir_all(config_dir()).map_err(|e| format!("创建配置目录失败: {}", e))?;
        let lock = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(lock_path)
            .map_err(|e| format!("打开提示词库锁失败: {}", e))?;
        let deadline = Instant::now() + Duration::from_secs(5);
        while lock.try_lock_exclusive().is_err() {
            if Instant::now() >= deadline {
                return Err("无法获取提示词库写入锁 (5s 超时)".into());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let result = f();
        let _ = lock.unlock();
        result
    })
}

fn write_state(state: &PromptLibraryState) -> Result<(), String> {
    if state.prompts.len() > MAX_PROMPTS {
        return Err("自定义提示词最多 30 条".into());
    }
    for prompt in &state.prompts {
        validate_prompt(prompt)?;
    }
    let path = library_file();
    fs::create_dir_all(config_dir()).map_err(|e| format!("创建配置目录失败: {}", e))?;
    // Keep three recoverable generations before atomically replacing the file.
    let _ = fs::remove_file(path.with_extension("json.bak3"));
    for index in (1..=2).rev() {
        let from = path.with_extension(format!("json.bak{}", index));
        let to = path.with_extension(format!("json.bak{}", index + 1));
        if from.exists() {
            let _ = fs::rename(from, to);
        }
    }
    if path.exists() {
        let _ = fs::copy(&path, path.with_extension("json.bak1"));
    }
    let text =
        serde_json::to_vec_pretty(state).map_err(|e| format!("序列化提示词库失败: {}", e))?;
    let tmp = path.with_extension("json.tmp");
    let mut file = File::create(&tmp).map_err(|e| format!("写入提示词库失败: {}", e))?;
    file.write_all(&text)
        .map_err(|e| format!("写入提示词库失败: {}", e))?;
    file.sync_all()
        .map_err(|e| format!("同步提示词库失败: {}", e))?;
    atomic_rename(&tmp, &path)
}

#[tauri::command]
pub fn get_prompt_library() -> Result<PromptLibraryState, String> {
    read_state()
}

#[tauri::command]
pub fn save_prompt_library(state: PromptLibraryState) -> Result<PromptLibraryState, String> {
    with_library_lock(|| {
        write_state(&state)?;
        Ok(state)
    })
}
