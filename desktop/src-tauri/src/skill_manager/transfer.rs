use std::fs;
use std::path::Path;

use super::types::MoveUndoInfo;

/// Determine the file name (last component) from a path.
/// For directories, returns the directory name; for files, returns the file name.
fn file_name_from_path(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_string())
}

/// Compute a lightweight content fingerprint for a file or directory.
/// For files: first 256 bytes as lossy UTF-8 + file size.
/// For directories: entry count + total size.
fn compute_content_fingerprint(path: &Path) -> String {
    if path.is_dir() {
        match std::fs::read_dir(path) {
            Ok(entries) => {
                let count = entries.count();
                format!("dir:{}", count)
            }
            Err(_) => "dir:err".to_string(),
        }
    } else {
        let size = path.metadata().map(|m| m.len()).unwrap_or(0);
        match std::fs::read(path) {
            Ok(bytes) => {
                let preview = String::from_utf8_lossy(&bytes[..bytes.len().min(256)]);
                format!("{}:{}", size, preview)
            }
            Err(_) => format!("{}:read_err", size),
        }
    }
}

/// Check whether a path is inside a project directory (has `.claude/` structure).
#[allow(dead_code)]
fn is_project_scope(path: &str) -> bool {
    // Project-level resources have ":project-" in their ID
    // But for paths, we check if the path contains "/.claude/" or "/.codex/" or
    // "/.qoder-cn/" (user) or "/.qoder/" (project, for agents/skills)
    path.contains("/.claude/") || path.contains("/.codex/") || path.contains("/.qoder-cn/") || path.contains("/.qoder/")
}

/// Move a file-based resource (skill, agent, command) from source to destination directory.
///
/// Creates a backup (.bak) at the source location for undo support.
/// Returns undo info so the frontend can reverse the operation.
pub(super) fn move_skill_file(
    source_path: String,
    dest_dir: String,
    resource_name: String,
    resource_type: String,
    from_scope: String,
    to_scope: String,
) -> Result<MoveUndoInfo, String> {
    let src = Path::new(&source_path);
    if !src.exists() {
        return Err(format!("源文件不存在: {}", source_path));
    }

    let dest_dir_path = Path::new(&dest_dir);
    // Ensure destination directory exists
    fs::create_dir_all(dest_dir_path)
        .map_err(|e| format!("无法创建目标目录: {}", e))?;

    let file_name = file_name_from_path(src)
        .ok_or_else(|| format!("无法解析文件名: {}", source_path))?;

    // Determine dest path based on resource type
    let dest_path = if src.is_dir() {
        // Codex/Qoder skills are directories
        dest_dir_path.join(&file_name)
    } else if src.extension().is_some_and(|e| e == "md") {
        // Claude skills/agents/commands are .md files
        dest_dir_path.join(format!("{}.md", file_name.trim_end_matches(".md")))
    } else {
        // .toml files (Codex agents) or other
        dest_dir_path.join(&file_name)
    };

    // Check for conflicts
    if dest_path.exists() {
        return Err(format!("目标已存在同名资源: {}", file_name));
    }

    // Compute fingerprint BEFORE any mutation (for undo verification)
    let content_fingerprint = compute_content_fingerprint(src);

    // Copy to destination
    if src.is_dir() {
        copy_dir_recursive(src, &dest_path)?;
    } else {
        fs::copy(src, &dest_path)
            .map_err(|e| format!("复制文件失败: {}", e))?;
    }

    // Build timestamped backup path to avoid overwriting previous .bak files
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let backup_path = if src.is_dir() {
        let mut bak_name = src
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        bak_name.push_str(&format!(".bak.{}", ts));
        src.parent().unwrap_or(Path::new(".")).join(&bak_name)
    } else {
        let stem = src.file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        let ext = src.extension()
            .and_then(|n| n.to_str())
            .map(|e| format!(".{}", e))
            .unwrap_or_default();
        let bak_name = format!("{}{}.bak.{}", stem, ext, ts);
        src.parent().unwrap_or(Path::new(".")).join(&bak_name)
    };

    // Rename source to backup (soft delete)
    if let Err(e) = fs::rename(src, &backup_path) {
        // Transactional rollback: delete the copy we just created at destination
        let _ = if dest_path.is_dir() {
            fs::remove_dir_all(&dest_path)
        } else {
            fs::remove_file(&dest_path)
        };
        return Err(format!("备份源文件失败: {}", e));
    }

    Ok(MoveUndoInfo {
        resource_name,
        resource_type,
        from_scope,
        to_scope,
        backup_path: backup_path.to_string_lossy().to_string(),
        original_path: source_path,
        dest_path: dest_path.to_string_lossy().to_string(),
        content_fingerprint,
    })
}

/// Copy a file-based resource without deleting the source.
pub(super) fn copy_skill_file(
    source_path: String,
    dest_dir: String,
    resource_name: String,
) -> Result<(), String> {
    let src = Path::new(&source_path);
    if !src.exists() {
        return Err(format!("源文件不存在: {}", source_path));
    }

    let dest_dir_path = Path::new(&dest_dir);
    fs::create_dir_all(dest_dir_path)
        .map_err(|e| format!("无法创建目标目录: {}", e))?;

    let file_name = file_name_from_path(src)
        .ok_or_else(|| format!("无法解析文件名: {}", source_path))?;

    let dest_path = if src.is_dir() {
        dest_dir_path.join(&file_name)
    } else if src.extension().is_some_and(|e| e == "md") {
        dest_dir_path.join(format!("{}.md", file_name.trim_end_matches(".md")))
    } else {
        dest_dir_path.join(&file_name)
    };

    if dest_path.exists() {
        return Err(format!("目标已存在同名资源: {}", resource_name));
    }

    if src.is_dir() {
        copy_dir_recursive(src, &dest_path)?;
    } else {
        fs::copy(src, &dest_path)
            .map_err(|e| format!("复制文件失败: {}", e))?;
    }

    Ok(())
}

/// Undo a move operation: delete the destination and restore the backup to original location.
///
/// Verifies the destination content hasn't been modified since the move (using a lightweight
/// fingerprint) before deleting, to avoid destroying user changes.
pub(super) fn undo_move_skill(backup_path: String, original_path: String, dest_path: String, content_fingerprint: String) -> Result<(), String> {
    let bak = Path::new(&backup_path);
    let orig = Path::new(&original_path);
    let dest = Path::new(&dest_path);

    // Verify destination content matches the fingerprint saved at move time.
    // If the user modified the destination after the move, refuse to delete it.
    if dest.exists() {
        let current_fp = compute_content_fingerprint(dest);
        if current_fp != content_fingerprint {
            return Err("目标文件已被修改，撤销取消。请手动处理".into());
        }
        if dest.is_dir() {
            fs::remove_dir_all(dest).map_err(|e| format!("删除目标失败: {}", e))?;
        } else {
            fs::remove_file(dest).map_err(|e| format!("删除目标失败: {}", e))?;
        }
    }

    // Restore backup to original location
    if bak.exists() {
        fs::rename(bak, orig).map_err(|e| format!("恢复备份失败: {}", e))?;
    } else {
        return Err("备份文件不存在，无法撤销".into());
    }

    Ok(())
}

/// Recursively copy a directory (used for Codex/Qoder skill directories).
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| format!("创建目录失败: {}", e))?;
    for entry in fs::read_dir(src).map_err(|e| format!("读取目录失败: {}", e))? {
        let entry = entry.map_err(|e| format!("读取条目失败: {}", e))?;
        let path = entry.path();
        if path.is_symlink() {
            continue; // Skip symlinks for safety
        }
        let dest_path = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir_recursive(&path, &dest_path)?;
        } else {
            fs::copy(&path, &dest_path).map_err(|e| format!("复制文件失败: {}", e))?;
        }
    }
    Ok(())
}
