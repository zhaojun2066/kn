//! Device fingerprint — macOS IOPlatformUUID via ioreg.
//!
//! Uses `/usr/sbin/ioreg` to query the hardware UUID stored in NVRAM.
//! This UUID only changes on full disk wipe (macOS reinstall).

use crate::error::{CommonError, Result};

/// Get the machine's unique hardware identifier (IOPlatformUUID).
///
/// On macOS, this is stored in NVRAM and persists across reboots.
/// Uses `/usr/sbin/ioreg` with a 5-second timeout.
///
/// On non-macOS platforms, falls back to a hash of the hostname.
pub fn machine_id() -> Result<String> {
    #[cfg(not(target_os = "macos"))]
    {
        // Fallback: use hostname as machine identifier on non-macOS
        let host = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string());
        use sha2::Digest;
        let mut h = sha2::Sha256::new();
        h.update(host.as_bytes());
        return Ok(format!("{:x}", h.finalize())[..36].to_string());
    }

    #[cfg(target_os = "macos")]
    {
    let mut child = std::process::Command::new("/usr/sbin/ioreg")
        .args(["-rd1", "-c", "IOPlatformExpertDevice"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| CommonError::Fingerprint(format!("ioreg 启动失败: {}", e)))?;

    // 5-second timeout via wait_timeout or polling
    let timeout = std::time::Duration::from_secs(5);
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return Err(CommonError::Fingerprint(format!(
                        "ioreg 退出码: {}",
                        status.code().unwrap_or(-1)
                    )));
                }
                break;
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(CommonError::Fingerprint("ioreg 超时 (5s)".into()));
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(e) => {
                return Err(CommonError::Fingerprint(format!("ioreg wait 失败: {}", e)));
            }
        }
    }

    let output = child
        .wait_with_output()
        .map_err(|e| CommonError::Fingerprint(format!("ioreg 读取输出失败: {}", e)))?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse: find "IOPlatformUUID" = "XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX"
    for line in stdout.lines() {
        if let Some(pos) = line.find("IOPlatformUUID") {
            // Find the value after '='
            if let Some(eq_pos) = line[pos..].find('=') {
                let after_eq = &line[pos + eq_pos + 1..];
                // Extract quoted string
                let trimmed = after_eq.trim();
                if let Some(start) = trimmed.find('"') {
                    let after_quote = &trimmed[start + 1..];
                    if let Some(end) = after_quote.find('"') {
                        return Ok(after_quote[..end].to_string());
                    }
                }
            }
        }
    }

    Err(CommonError::Fingerprint(
        "IOPlatformUUID 未在 ioreg 输出中找到".into(),
    ))
    } // end #[cfg(target_os = "macos")]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn test_machine_id_returns_uuid_format() {
        let id = machine_id();
        assert!(id.is_ok(), "machine_id should return Ok on macOS");
        let id = id.unwrap();
        // UUID format: 8-4-4-4-12 hex digits with dashes
        assert_eq!(id.len(), 36, "UUID should be 36 chars");
        assert_eq!(id.chars().filter(|&c| c == '-').count(), 4, "UUID should have 4 dashes");
    }
}
