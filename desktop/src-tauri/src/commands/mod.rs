//! Tauri command handlers — re-exported from domain-specific sub-modules.
//!
//! Uses glob re-exports (`pub use module::*`) so Tauri's `#[command]` proc-macro
//! generated companion types (`__cmd__`, `__tauri_command_name_`) are accessible
//! in `commands::` alongside the functions themselves.

pub mod profile;
pub mod file_io;
pub mod app_config;
pub mod system_scan;
pub mod platform;
pub mod network;
pub mod external;
pub mod env_check;

pub use app_config::*;
pub use env_check::*;
pub use external::*;
pub use file_io::*;
pub use network::*;
pub use platform::*;
pub use profile::*;
pub use system_scan::*;
