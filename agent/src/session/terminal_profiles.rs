//! Restricted parser-profile storage.
//!
//! Profiles describe tool grammar; they never become a global keyword matcher.
//! The built-in parser remains the fallback when a profile is absent, invalid,
//! too large, or incompatible with the Agent schema.

use regex_lite::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;

const MAX_PROFILE_BYTES: usize = 512 * 1024;
const MAX_PROFILES: usize = 128;
const MAX_PATTERNS: usize = 64;
const MAX_PATTERN_BYTES: usize = 1024;
const SUPPORTED_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TerminalParserProfiles {
    pub schema_version: u32,
    pub profiles_version: String,
    #[serde(default)]
    pub updated_at: Option<String>,
    pub profiles: Vec<ParserProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ParserProfile {
    pub id: String,
    pub priority: u32,
    #[serde(default)]
    pub command_matchers: Vec<String>,
    #[serde(default)]
    pub summary_patterns: Vec<String>,
    #[serde(default)]
    pub operation_rules: serde_json::Value,
    #[serde(default)]
    pub diagnostic_rules: serde_json::Value,
    #[serde(default)]
    pub exclusions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileError {
    TooLarge,
    InvalidJson,
    UnsupportedSchema(u32),
    MissingGeneric,
    Invalid(String),
    HashMismatch,
    Io(String),
}

#[derive(Debug, Clone)]
pub struct TerminalProfileStore {
    cache_path: PathBuf,
    profiles: Option<TerminalParserProfiles>,
}

impl TerminalProfileStore {
    pub fn new(cache_path: impl Into<PathBuf>) -> Self {
        Self { cache_path: cache_path.into(), profiles: None }
    }

    pub fn load_cached(&mut self) -> Result<Option<TerminalParserProfiles>, ProfileError> {
        let bytes = match fs::read(&self.cache_path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(ProfileError::Io(error.to_string())),
        };
        let profiles = Self::parse(&bytes, None)?;
        self.profiles = Some(profiles.clone());
        Ok(Some(profiles))
    }

    pub fn accept_remote(&mut self, bytes: &[u8], expected_sha256: &str) -> Result<(), ProfileError> {
        let actual = sha256_hex(bytes);
        if !actual.eq_ignore_ascii_case(expected_sha256) {
            return Err(ProfileError::HashMismatch);
        }
        let profiles = Self::parse(bytes, Some(expected_sha256))?;
        if let Some(parent) = self.cache_path.parent() {
            fs::create_dir_all(parent).map_err(|error| ProfileError::Io(error.to_string()))?;
        }
        let temp = self.cache_path.with_extension("tmp");
        fs::write(&temp, bytes).map_err(|error| ProfileError::Io(error.to_string()))?;
        fs::rename(&temp, &self.cache_path).map_err(|error| ProfileError::Io(error.to_string()))?;
        self.profiles = Some(profiles);
        Ok(())
    }

    pub fn current(&self) -> Option<&TerminalParserProfiles> { self.profiles.as_ref() }

    /// Refreshes the public Cloud profile only after obtaining its advertised
    /// SHA-256. Network or validation failures leave the current cache intact.
    pub async fn refresh_from_cloud(
        &mut self,
        client: &reqwest::Client,
        cloud_http_url: &str,
    ) -> Result<(), ProfileError> {
        #[derive(Deserialize)]
        struct Envelope<T> { data: T }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Version { sha256: String }
        let base = cloud_http_url.trim_end_matches('/');
        let version: Envelope<Version> = client
            .get(format!("{base}/api/v1/app-config/terminal-parser-profiles/version"))
            .send().await.map_err(|error| ProfileError::Io(error.to_string()))?
            .error_for_status().map_err(|error| ProfileError::Io(error.to_string()))?
            .json().await.map_err(|error| ProfileError::Io(error.to_string()))?;
        let bytes = client
            .get(format!("{base}/api/v1/app-config/terminal-parser-profiles"))
            .send().await.map_err(|error| ProfileError::Io(error.to_string()))?
            .error_for_status().map_err(|error| ProfileError::Io(error.to_string()))?
            .bytes().await.map_err(|error| ProfileError::Io(error.to_string()))?;
        let envelope: Envelope<TerminalParserProfiles> =
            serde_json::from_slice(&bytes).map_err(|_| ProfileError::InvalidJson)?;
        let profiles_bytes = serde_json::to_vec(&envelope.data)
            .map_err(|error| ProfileError::Io(error.to_string()))?;
        // Cloud hashes the canonical profile payload, not the transport
        // envelope. Validate before replacing the cache.
        if sha256_hex(&canonical_json(&envelope.data)) != version.data.sha256 {
            return Err(ProfileError::HashMismatch);
        }
        self.accept_remote(&profiles_bytes, &sha256_hex(&profiles_bytes))
    }

    pub fn parse(bytes: &[u8], _expected_sha256: Option<&str>) -> Result<TerminalParserProfiles, ProfileError> {
        if bytes.len() > MAX_PROFILE_BYTES { return Err(ProfileError::TooLarge); }
        let profiles: TerminalParserProfiles = serde_json::from_slice(bytes).map_err(|_| ProfileError::InvalidJson)?;
        validate(&profiles)?;
        Ok(profiles)
    }
}

fn validate(value: &TerminalParserProfiles) -> Result<(), ProfileError> {
    if value.schema_version != SUPPORTED_SCHEMA_VERSION {
        return Err(ProfileError::UnsupportedSchema(value.schema_version));
    }
    if value.profiles_version.trim().is_empty() || value.profiles.is_empty() || value.profiles.len() > MAX_PROFILES {
        return Err(ProfileError::Invalid("invalid profile set metadata".into()));
    }
    let mut ids = std::collections::HashSet::<String>::new();
    for profile in &value.profiles {
        if profile.id.trim().is_empty() || !ids.insert(profile.id.clone()) {
            return Err(ProfileError::Invalid("duplicate or empty profile id".into()));
        }
        if profile.command_matchers.len() > MAX_PATTERNS || profile.summary_patterns.len() > MAX_PATTERNS {
            return Err(ProfileError::Invalid(profile.id.clone()));
        }
        for pattern in profile.command_matchers.iter().chain(profile.summary_patterns.iter()) {
            if pattern.len() > MAX_PATTERN_BYTES || Regex::new(pattern).is_err() {
                return Err(ProfileError::Invalid(profile.id.clone()));
            }
        }
    }
    if !ids.contains("generic-exit") { return Err(ProfileError::MissingGeneric); }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn canonical_json(value: &TerminalParserProfiles) -> Vec<u8> {
    fn sort(value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(map) => {
                let mut entries: Vec<_> = map.into_iter().collect();
                entries.sort_by(|left, right| left.0.cmp(&right.0));
                serde_json::Value::Object(entries.into_iter().map(|(key, value)| (key, sort(value))).collect())
            }
            serde_json::Value::Array(values) => serde_json::Value::Array(values.into_iter().map(sort).collect()),
            other => other,
        }
    }
    serde_json::to_vec(&sort(serde_json::to_value(value).unwrap_or_default())).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json() -> Vec<u8> {
        br#"{"schemaVersion":1,"profilesVersion":"1","profiles":[{"id":"generic-exit","priority":0,"commandMatchers":[],"summaryPatterns":[],"operationRules":{},"diagnosticRules":{},"exclusions":[]},{"id":"maven","priority":1,"commandMatchers":["^mvn"],"summaryPatterns":["BUILD SUCCESS"],"operationRules":{},"diagnosticRules":{},"exclusions":[] }]}"#.to_vec()
    }

    #[test]
    fn rejects_profiles_without_generic_fallback() {
        let value = String::from_utf8(json()).unwrap().replace("generic-exit", "missing");
        assert_eq!(TerminalProfileStore::parse(value.as_bytes(), None), Err(ProfileError::MissingGeneric));
    }

    #[test]
    fn accepts_hashed_profile_and_round_trips_cache() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profiles.json");
        let bytes = json();
        let expected = sha256_hex(&bytes);
        let mut store = TerminalProfileStore::new(&path);
        store.accept_remote(&bytes, &expected).unwrap();
        let mut loaded = TerminalProfileStore::new(&path);
        assert_eq!(loaded.load_cached().unwrap().unwrap().profiles_version, "1");
    }
}
