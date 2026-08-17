//! File I/O with path safety (TOCTOU-safe resolution) and directory tree operations.

use std::path::PathBuf;
use tauri::command;

use super::system_scan::home_dir;

/// Resolve `..` and symlinks, verify the resolved path is under home or temp dir,
/// and return the canonical path for safe use (eliminates TOCTOU race).
pub(crate) fn is_safe_path(path: &std::path::Path) -> Option<PathBuf> {
    let resolved = match path.canonicalize() {
        Ok(r) => r,
        Err(_) => match path.parent().filter(|p| !p.as_os_str().is_empty()) {
            Some(parent) => match parent.canonicalize() {
                Ok(parent_resolved) => {
                    let name = path.file_name().unwrap_or(std::ffi::OsStr::new("unnamed"));
                    parent_resolved.join(name)
                }
                Err(_) => return None,
            },
            None => return None,
        },
    };
    let home = home_dir();
    let tmp = std::env::temp_dir();
    let home_resolved = home.canonicalize().ok()?;
    let tmp_resolved = tmp.canonicalize().ok()?;
    if resolved.starts_with(&home_resolved) || resolved.starts_with(&tmp_resolved) {
        Some(resolved)
    } else {
        None
    }
}

#[command]
pub fn write_file(path: String, content: String) -> Result<(), String> {
    let safe_path =
        is_safe_path(std::path::Path::new(&path)).ok_or_else(|| "不允许访问此路径".to_string())?;
    std::fs::write(&safe_path, &content).map_err(|e| format!("写入文件失败: {}", e))
}

#[command]
pub fn read_file(path: String) -> Result<String, String> {
    let safe_path =
        is_safe_path(std::path::Path::new(&path)).ok_or_else(|| "不允许访问此路径".to_string())?;
    std::fs::read_to_string(&safe_path).map_err(|e| format!("读取文件失败: {}", e))
}

#[command]
pub fn read_file_base64(path: String) -> Result<String, String> {
    let safe_path =
        is_safe_path(std::path::Path::new(&path)).ok_or_else(|| "不允许访问此路径".to_string())?;
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    let bytes = std::fs::read(&safe_path).map_err(|e| format!("读取文件失败: {}", e))?;
    Ok(STANDARD.encode(&bytes))
}

// ── Directory tree ──

#[derive(Debug, serde::Serialize, Clone)]
pub struct FileTreeNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<FileTreeNode>>,
}

const SKIP_NAMES: &[&str] = &[".git", "node_modules", "__pycache__", ".DS_Store"];
const MAX_TREE_DEPTH: u32 = 20;

fn build_tree(root: &std::path::Path) -> Result<FileTreeNode, String> {
    let mut visited = std::collections::HashSet::new();
    build_tree_inner(root, &mut visited, 0)
}

fn build_tree_inner(
    root: &std::path::Path,
    visited: &mut std::collections::HashSet<PathBuf>,
    depth: u32,
) -> Result<FileTreeNode, String> {
    let name = root
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let path = root.display().to_string();
    let is_dir = root.is_dir();

    if !is_dir {
        return Ok(FileTreeNode {
            name,
            path,
            is_dir: false,
            children: None,
        });
    }

    let canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    if !visited.insert(canonical) {
        return Ok(FileTreeNode {
            name,
            path,
            is_dir: true,
            children: Some(vec![]),
        });
    }
    if depth >= MAX_TREE_DEPTH {
        return Ok(FileTreeNode {
            name,
            path,
            is_dir: true,
            children: Some(vec![]),
        });
    }

    let mut children: Vec<FileTreeNode> = Vec::new();
    let entries = std::fs::read_dir(root).map_err(|e| format!("读取目录失败: {}", e))?;

    let mut dirs: Vec<(String, PathBuf)> = Vec::new();
    let mut files: Vec<(String, PathBuf)> = Vec::new();

    for entry in entries.flatten() {
        let fname = entry.file_name().to_string_lossy().to_string();
        if SKIP_NAMES.contains(&fname.as_str()) {
            continue;
        }
        let fpath = entry.path();
        if fpath.is_dir() {
            dirs.push((fname, fpath));
        } else {
            files.push((fname, fpath));
        }
    }

    dirs.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    files.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));

    for (_name, dir_path) in &dirs {
        match build_tree_inner(dir_path, visited, depth + 1) {
            Ok(node) => children.push(node),
            Err(_) => continue,
        }
    }
    for (fname, fpath) in &files {
        children.push(FileTreeNode {
            name: fname.clone(),
            path: fpath.display().to_string(),
            is_dir: false,
            children: None,
        });
    }

    Ok(FileTreeNode {
        name,
        path,
        is_dir: true,
        children: Some(children),
    })
}

#[command]
pub fn list_directory_tree(path: String) -> Result<FileTreeNode, String> {
    let p = std::path::Path::new(&path);
    if p.is_file() {
        let name = p
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        let safe_path = is_safe_path(p).ok_or_else(|| "不允许访问此路径".to_string())?;
        return Ok(FileTreeNode {
            name: name.clone(),
            path: safe_path.to_string_lossy().to_string(),
            is_dir: false,
            children: None,
        });
    }
    let root = p.to_path_buf();
    if !root.exists() {
        return Err(format!("路径不存在: {}", root.display()));
    }
    let safe_root = is_safe_path(&root).ok_or_else(|| "不允许访问此路径".to_string())?;
    build_tree(&safe_root)
}

#[command]
pub fn list_directory_children(path: String) -> Result<Vec<FileTreeNode>, String> {
    let p = std::path::Path::new(&path);
    if !p.is_dir() {
        return Err("路径不是目录".into());
    }
    if !p.exists() {
        return Err(format!("路径不存在: {}", p.display()));
    }
    let safe_p = is_safe_path(p).ok_or_else(|| "不允许访问此路径".to_string())?;

    let mut dirs: Vec<(String, PathBuf)> = Vec::new();
    let mut files: Vec<(String, PathBuf)> = Vec::new();

    let entries = std::fs::read_dir(&safe_p).map_err(|e| format!("读取目录失败: {}", e))?;
    for entry in entries.flatten() {
        let fname = entry.file_name().to_string_lossy().to_string();
        if SKIP_NAMES.contains(&fname.as_str()) {
            continue;
        }
        let fpath = entry.path();
        if fpath.is_dir() {
            dirs.push((fname, fpath));
        } else {
            files.push((fname, fpath));
        }
    }

    dirs.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    files.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));

    let mut children: Vec<FileTreeNode> = Vec::with_capacity(dirs.len() + files.len());
    for (name, fpath) in &dirs {
        children.push(FileTreeNode {
            name: name.clone(),
            path: fpath.display().to_string(),
            is_dir: true,
            children: None,
        });
    }
    for (fname, fpath) in &files {
        children.push(FileTreeNode {
            name: fname.clone(),
            path: fpath.display().to_string(),
            is_dir: false,
            children: None,
        });
    }
    Ok(children)
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_safe_path_rejects_parent_dir_traversal() {
        let bad = std::path::Path::new("../etc/passwd");
        assert!(is_safe_path(bad).is_none());
    }

    #[test]
    fn test_is_safe_path_rejects_non_existent_path_outside_safe_dirs() {
        let bad = std::path::Path::new("/nonexistent-xyz-kn-test-file");
        assert!(is_safe_path(bad).is_none());
    }

    #[test]
    fn test_is_safe_path_allows_non_existent_path_in_temp() {
        let tmp = std::env::temp_dir().join("kn-test-temp-nonexistent-file.dmg");
        let _ = std::fs::remove_file(&tmp);
        let resolved = is_safe_path(&tmp);
        assert!(resolved.is_some());
        assert!(resolved
            .unwrap()
            .ends_with("kn-test-temp-nonexistent-file.dmg"));
    }

    #[test]
    fn test_is_safe_path_allows_non_existent_path_in_home() {
        let home = home_dir();
        let safe = home.join(".kn-test-home-nonexistent-file.tmp");
        let _ = std::fs::remove_file(&safe);
        let resolved = is_safe_path(&safe);
        assert!(resolved.is_some());
        assert!(resolved
            .unwrap()
            .ends_with(".kn-test-home-nonexistent-file.tmp"));
    }

    #[test]
    fn test_is_safe_path_allows_home_subdir() {
        let _guard = crate::TEST_ENV_LOCK.lock().unwrap();
        let old_home = std::env::var_os("HOME");
        let tmp_home = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", tmp_home.path());
        let home = home_dir();
        let safe = home.join(".kn-test-safe-path");
        std::fs::create_dir_all(&safe).ok();
        let resolved = is_safe_path(&safe);
        assert!(resolved.is_some());
        std::fs::remove_dir_all(&safe).ok();
        if let Some(value) = old_home {
            std::env::set_var("HOME", value);
        } else {
            std::env::remove_var("HOME");
        }
    }

    #[test]
    fn test_is_safe_path_allows_temp_dir() {
        let tmp = std::env::temp_dir().join("kn-test-temp-safe");
        std::fs::create_dir_all(&tmp).ok();
        assert!(is_safe_path(&tmp).is_some());
        std::fs::remove_dir_all(&tmp).ok();
    }
}
