//! HTTP fetch, file download, SHA256 verification, binary resolution.

use std::time::Duration;
use tauri::command;
use tauri::Emitter;

use super::file_io::is_safe_path;

/// Find a system binary across common macOS paths.
pub(crate) fn find_binary(names: &[&str]) -> Option<String> {
    kn_common::path::find_binary(names)
}

#[command]
pub async fn fetch_url(url: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        reqwest::blocking::Client::new()
            .get(&url)
            .timeout(Duration::from_secs(30))
            .send()
            .map_err(|e| format!("请求失败: {}", e))?
            .error_for_status()
            .map_err(|e| format!("HTTP 错误: {}", e))?
            .text()
            .map_err(|e| format!("读取响应失败: {}", e))
    })
    .await
    .map_err(|e| format!("后台任务失败: {}", e))?
}

#[command]
pub async fn download_file(url: String, path: String, app: tauri::AppHandle) -> Result<(), String> {
    let safe_path = is_safe_path(std::path::Path::new(&path))
        .ok_or_else(|| "不允许下载到此路径".to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        use std::io::{Read, Write};

        let mut response = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(30))
            .timeout(Duration::from_secs(3600))
            .build()
            .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?
            .get(&url)
            .send()
            .map_err(|e| format!("请求失败: {}", e))?
            .error_for_status()
            .map_err(|e| format!("HTTP 错误: {}", e))?;

        let total = response.content_length();
        let mut file = std::fs::File::create(&safe_path).map_err(|e| format!("创建文件失败: {}", e))?;
        let mut downloaded: u64 = 0;
        let mut last_pct: u8 = 0;
        let mut buf = [0u8; 8192];

        loop {
            let n = response.read(&mut buf).map_err(|e| format!("下载失败: {}", e))?;
            if n == 0 { break; }
            file.write_all(&buf[..n]).map_err(|e| format!("写入文件失败: {}", e))?;
            downloaded += n as u64;
            if let Some(total) = total {
                if total > 0 {
                    let pct = ((downloaded as f64 / total as f64) * 100.0).min(99.0) as u8;
                    if pct != last_pct { last_pct = pct; let _ = app.emit("download-progress", pct); }
                }
            }
        }

        file.flush().map_err(|e| format!("刷新文件失败: {}", e))?;
        file.sync_all().map_err(|e| format!("同步文件失败: {}", e))?;
        let _ = app.emit("download-progress", 100u8);
        Ok(())
    })
    .await
    .map_err(|e| format!("后台任务失败: {}", e))?
}

#[command]
pub fn verify_sha256(path: String, expected: String) -> Result<bool, String> {
    kn_common::path::verify_sha256(std::path::Path::new(&path), &expected)
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_binary_known_command() {
        let path = find_binary(&["sh"]);
        assert!(path.is_some());
        assert!(std::path::Path::new(&path.unwrap()).exists());
    }

    #[test]
    fn test_find_binary_nonexistent() {
        let path = find_binary(&["nonexistent_binary_xyz_12345"]);
        if let Some(p) = &path {
            assert!(!std::path::Path::new(p).exists());
        }
    }

    #[test]
    fn test_verify_sha256_correct_hash() {
        let dir = std::env::temp_dir().join(format!("kn-test-sha256-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file_path = dir.join("test.bin");
        std::fs::write(&file_path, b"hello world\n").unwrap();
        let expected = "a948904f2f0f479b8f8197694b30184b0d2ed1c1cd2a1ec0fb85d299a192a447";
        assert!(verify_sha256(file_path.to_string_lossy().to_string(), expected.to_string()).unwrap());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_verify_sha256_wrong_hash() {
        let dir = std::env::temp_dir().join(format!("kn-test-sha256-wrong-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file_path = dir.join("test.bin");
        std::fs::write(&file_path, b"hello world\n").unwrap();
        let result = verify_sha256(file_path.to_string_lossy().to_string(), "0000000000000000000000000000000000000000000000000000000000000000".to_string()).unwrap();
        assert!(!result);
        std::fs::remove_dir_all(&dir).ok();
    }
}
