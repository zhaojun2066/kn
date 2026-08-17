use thiserror::Error;

#[derive(Error, Debug)]
pub enum CommonError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("YAML parse error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Config error: {0}")]
    Config(String),

    #[error("Crypto error: {0}")]
    Crypto(String),

    #[error("Keychain error: {0}")]
    Keychain(String),

    #[error("Fingerprint error: {0}")]
    Fingerprint(String),
}

pub type Result<T> = std::result::Result<T, CommonError>;

// Convenience conversion for String errors (used by existing code)
impl From<String> for CommonError {
    fn from(s: String) -> Self {
        CommonError::Config(s)
    }
}

impl From<&str> for CommonError {
    fn from(s: &str) -> Self {
        CommonError::Config(s.to_string())
    }
}
