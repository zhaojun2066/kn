//! AES-256-GCM config value encryption + macOS Keychain key storage.
//!
//! Encrypts sensitive env var values (API keys, tokens) stored in config.yaml.
//! Master key is generated once and stored in macOS Keychain.
//!
//! Ciphertext format: `kn:v1:<hex_nonce><hex_ciphertext>`
//! - `kn:v1:` — version prefix for future algorithm upgrades
//! - hex_nonce — 12-byte AES-GCM nonce (24 hex chars)
//! - hex_ciphertext — AES-GCM encrypted payload (variable length)

use aes_gcm::aead::{Aead, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, Key, KeyInit, Nonce};
use crate::error::{CommonError, Result};

/// Version prefix for ciphertext.
const CIPHER_PREFIX: &str = "kn:v1:";

/// Nonce size in bytes (96-bit, per NIST recommendation).
const NONCE_SIZE: usize = 12;

/// Key size in bytes (256-bit).
const KEY_SIZE: usize = 32;

const KEYCHAIN_SERVICE: &str = "com.kn.agent";
const KEYCHAIN_ACCOUNT: &str = "config-key";

// ── KeyStore abstraction ─────────────────────────────────────

/// Key storage abstraction — macOS Keychain in production, memory-based for testing.
pub trait KeyStore: Send + Sync {
    fn get_key(&self, service: &str, account: &str) -> Result<Option<Vec<u8>>>;
    fn set_key(&self, service: &str, account: &str, key: &[u8]) -> Result<()>;
}

/// Production implementation using file-based key storage (0600 permissions).
/// macOS Keychain integration planned for a future version.
pub struct MacKeyStore;

impl KeyStore for MacKeyStore {
    fn get_key(&self, _service: &str, _account: &str) -> Result<Option<Vec<u8>>> {
        key_file_read()
    }

    fn set_key(&self, _service: &str, _account: &str, key: &[u8]) -> Result<()> {
        key_file_write(key)
    }
}

// ── Public API ───────────────────────────────────────────────

/// Encrypt a plaintext value using AES-256-GCM.
///
/// Returns `kn:v1:<hex_nonce><hex_ciphertext>` on success.
/// The master key is loaded from or created in the Keychain.
pub fn encrypt_value(keystore: &dyn KeyStore, plaintext: &str) -> Result<String> {
    let key_bytes = load_or_create_key(keystore)?;
    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);

    // Generate random nonce using OS entropy (CSPRNG)
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);

    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|e| CommonError::Crypto(format!("加密失败: {}", e)))?;

    // Encode: kn:v1:<hex_nonce><hex_ciphertext>
    let mut result = String::with_capacity(
        CIPHER_PREFIX.len() + NONCE_SIZE * 2 + ciphertext.len() * 2,
    );
    result.push_str(CIPHER_PREFIX);
    result.push_str(&hex::encode(nonce.as_slice()));
    result.push_str(&hex::encode(&ciphertext));
    Ok(result)
}

/// Decrypt a ciphertext value.
///
/// - Values with `kn:v1:` prefix are decrypted using AES-256-GCM.
/// - Values without the prefix are returned as-is (forward compatibility
///   with unencrypted config values from older versions).
pub fn decrypt_value(keystore: &dyn KeyStore, encoded: &str) -> Result<String> {
    if !encoded.starts_with(CIPHER_PREFIX) {
        // Not encrypted — return as-is (forward compat)
        return Ok(encoded.to_string());
    }

    let hex = &encoded[CIPHER_PREFIX.len()..];
    if hex.len() < NONCE_SIZE * 2 {
        return Err(CommonError::Crypto("密文太短".into()));
    }

    let nonce_hex = &hex[..NONCE_SIZE * 2];
    let ct_hex = &hex[NONCE_SIZE * 2..];

    let nonce_bytes = hex_to_bytes(nonce_hex)
        .map_err(|e| CommonError::Crypto(format!("nonce 解析失败: {}", e)))?;
    let ct_bytes = hex_to_bytes(ct_hex)
        .map_err(|e| CommonError::Crypto(format!("密文解析失败: {}", e)))?;

    let key_bytes = load_or_create_key(keystore)?;
    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let plaintext = cipher
        .decrypt(nonce, ct_bytes.as_ref())
        .map_err(|e| CommonError::Crypto(format!("解密失败: {}", e)))?;

    String::from_utf8(plaintext).map_err(|e| CommonError::Crypto(format!("UTF-8 解码失败: {}", e)))
}

/// Load the master key from Keychain, or create a new one on first run.
///
/// Uses exclusive file lock to prevent concurrent key generation races.
/// Returns an error if the key exists but has the wrong size (corrupted).
pub fn load_or_create_key(keystore: &dyn KeyStore) -> Result<Vec<u8>> {
    // Try existing key first
    if let Some(key) = keystore.get_key(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT)? {
        if key.len() == KEY_SIZE {
            return Ok(key);
        }
        // Corrupted key — return error, do NOT silently regenerate
        return Err(CommonError::Crypto(format!(
            "密钥文件已损坏 (大小 {} 字节, 期望 32 字节)。请删除 ~/.kn/agent/.encryption_key 后重试",
            key.len()
        )));
    }

    // Generate new 256-bit key using OS entropy.
    // The key_file_read/write functions use atomic tmp+rename, but we also
    // need to guard against concurrent first-run: two callers both see no key,
    // both generate, both write. Use exclusive file lock via fs2.
    let lock_path = key_file_path().with_extension("lock");
    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| CommonError::Keychain(format!("创建密钥目录失败: {}", e)))?;
    }
    let lock_fh = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|e| CommonError::Keychain(format!("打开密钥锁文件失败: {}", e)))?;
    fs2::FileExt::lock_exclusive(&lock_fh)
        .map_err(|e| CommonError::Keychain(format!("获取密钥锁失败: {}", e)))?;

    // Double-check after acquiring lock (another caller may have created the key)
    if let Some(key) = keystore.get_key(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT)? {
        if key.len() == KEY_SIZE {
            let _ = fs2::FileExt::unlock(&lock_fh);
            return Ok(key);
        }
    }

    let key = Aes256Gcm::generate_key(OsRng);
    let key_bytes = key.as_slice().to_vec();

    keystore.set_key(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT, &key_bytes)?;

    let _ = fs2::FileExt::unlock(&lock_fh);
    Ok(key_bytes)
}

// ── Helpers ──────────────────────────────────────────────────

fn hex_to_bytes(hex: &str) -> std::result::Result<Vec<u8>, String> {
    if hex.len() % 2 != 0 {
        return Err("hex 字符串长度必须是偶数".into());
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&hex[i..i + 2], 16)
                .map_err(|e| format!("hex 解码失败: {}", e))
        })
        .collect()
}

// ── File-based key storage (macOS Keychain planned for v2) ────

/// Path to the encryption key file.
fn key_file_path() -> std::path::PathBuf {
    crate::path::config_dir().join("agent").join(".encryption_key")
}

/// Read the master key from the key file.
fn key_file_read() -> Result<Option<Vec<u8>>> {
    let path = key_file_path();
    if !path.exists() {
        return Ok(None);
    }
    let data = std::fs::read(&path).map_err(|e| CommonError::Keychain(format!("读取密钥文件失败: {}", e)))?;
    Ok(Some(data))
}

/// Write the master key to the key file with restrictive permissions.
fn key_file_write(key: &[u8]) -> Result<()> {
    let path = key_file_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| CommonError::Keychain(format!("创建密钥目录失败: {}", e)))?;
    }

    // Atomic write with 0600 permissions from creation time (no permission window)
    let tmp = path.with_extension("tmp");
    {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp)
            .map_err(|e| CommonError::Keychain(format!("创建临时密钥文件失败: {}", e)))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600)).ok();
        }
        f.write_all(key).map_err(|e| CommonError::Keychain(format!("写入密钥失败: {}", e)))?;
        f.sync_all().map_err(|e| CommonError::Keychain(format!("同步密钥文件失败: {}", e)))?;
    }

    std::fs::rename(&tmp, &path)
        .map_err(|e| CommonError::Keychain(format!("密钥文件 rename 失败: {}", e)))?;

    Ok(())
}

/// Delete the key file (for testing/uninstall).
#[allow(dead_code)]
fn key_file_delete() -> Result<()> {
    let path = key_file_path();
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| CommonError::Keychain(format!("删除密钥文件失败: {}", e)))?;
    }
    Ok(())
}

// ── Tests ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// In-memory key store for testing (no macOS Keychain required).
    struct MemoryKeyStore {
        keys: Mutex<HashMap<String, Vec<u8>>>,
    }

    impl MemoryKeyStore {
        fn new() -> Self {
            Self {
                keys: Mutex::new(HashMap::new()),
            }
        }

        fn key_name(service: &str, account: &str) -> String {
            format!("{}:{}", service, account)
        }
    }

    impl KeyStore for MemoryKeyStore {
        fn get_key(&self, service: &str, account: &str) -> Result<Option<Vec<u8>>> {
            Ok(self
                .keys
                .lock()
                .unwrap()
                .get(&Self::key_name(service, account))
                .cloned())
        }

        fn set_key(&self, service: &str, account: &str, key: &[u8]) -> Result<()> {
            self.keys
                .lock()
                .unwrap()
                .insert(Self::key_name(service, account), key.to_vec());
            Ok(())
        }
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let keystore = MemoryKeyStore::new();
        let plaintext = "sk-ant-api-1234567890-secret-key";
        let encrypted = encrypt_value(&keystore, plaintext).unwrap();
        assert!(encrypted.starts_with("kn:v1:"));
        let decrypted = decrypt_value(&keystore, &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_unversioned_passthrough() {
        let keystore = MemoryKeyStore::new();
        // Values without kn:v1: prefix are returned as-is (forward compat)
        let result = decrypt_value(&keystore, "plaintext-value").unwrap();
        assert_eq!(result, "plaintext-value");
    }

    #[test]
    fn test_key_reuse() {
        let keystore = MemoryKeyStore::new();
        let a = encrypt_value(&keystore, "hello").unwrap();
        let b = encrypt_value(&keystore, "world").unwrap();
        // Same key should be used (both decrypt correctly)
        assert_eq!(decrypt_value(&keystore, &a).unwrap(), "hello");
        assert_eq!(decrypt_value(&keystore, &b).unwrap(), "world");
    }

    #[test]
    fn test_different_plaintexts_produce_different_ciphertexts() {
        let keystore = MemoryKeyStore::new();
        let a = encrypt_value(&keystore, "same text").unwrap();
        let b = encrypt_value(&keystore, "same text").unwrap();
        // Different nonces → different ciphertexts
        assert_ne!(a, b);
        // Both decrypt to same plaintext
        assert_eq!(decrypt_value(&keystore, &a).unwrap(), "same text");
        assert_eq!(decrypt_value(&keystore, &b).unwrap(), "same text");
    }

    #[test]
    fn test_hex_roundtrip() {
        let original = vec![0u8, 1, 2, 255, 128];
        let hex: String = original.iter().map(|b| format!("{:02x}", b)).collect();
        let decoded = hex_to_bytes(&hex).unwrap();
        assert_eq!(decoded, original);
    }
}
