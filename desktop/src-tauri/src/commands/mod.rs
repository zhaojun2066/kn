//! Tauri command handlers — re-exported from domain-specific sub-modules.
//!
//! Uses glob re-exports (`pub use module::*`) so Tauri's `#[command]` proc-macro
//! generated companion types (`__cmd__`, `__tauri_command_name_`) are accessible
//! in `commands::` alongside the functions themselves.

pub mod agent_control;
pub mod app_config;
pub mod env_check;
pub mod external;
pub mod file_io;
pub mod network;
pub mod platform;
pub mod prompt_library;
pub mod profile;
pub mod release;
pub mod system_scan;

pub use agent_control::*;
pub use app_config::*;
pub use env_check::*;
pub use external::*;
pub use file_io::*;
pub use network::*;
pub use platform::*;
pub use prompt_library::*;
pub use profile::*;
pub use release::*;
pub use system_scan::*;
