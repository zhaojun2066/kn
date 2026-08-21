use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentRuntimeEnvironment {
    Development,
    Production,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentRuntime {
    pub environment: AgentRuntimeEnvironment,
    pub config_dir: PathBuf,
    pub launchd_label: &'static str,
}

impl AgentRuntime {
    pub fn current() -> Self {
        let home = kn_common::path::home_dir();
        Self::resolve(cfg!(debug_assertions), home)
    }

    fn resolve(is_debug_build: bool, home: PathBuf) -> Self {
        let development_home = home.join(".kn-dev");
        let production_home = home.join(".kn");

        if is_debug_build {
            Self {
                environment: AgentRuntimeEnvironment::Development,
                config_dir: development_home,
                launchd_label: "com.kn.agent.dev",
            }
        } else {
            Self {
                environment: AgentRuntimeEnvironment::Production,
                config_dir: production_home,
                launchd_label: "com.kn.agent",
            }
        }
    }

    pub fn agent_dir(&self) -> PathBuf {
        self.config_dir.join("agent")
    }

    pub fn environment_name(&self) -> &'static str {
        match self.environment {
            AgentRuntimeEnvironment::Development => "development",
            AgentRuntimeEnvironment::Production => "production",
        }
    }
}

pub(crate) fn should_restart_agent(
    agent_updated: bool,
    plist_updated: bool,
    running_version_mismatch: bool,
) -> bool {
    agent_updated || plist_updated || running_version_mismatch
}

pub(crate) fn escape_plist_value(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '\"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_runtime_uses_isolated_home_and_label() {
        let runtime = AgentRuntime::resolve(true, PathBuf::from("/Users/test"));
        assert_eq!(runtime.config_dir, PathBuf::from("/Users/test/.kn-dev"));
        assert_eq!(runtime.launchd_label, "com.kn.agent.dev");
        assert_eq!(runtime.environment_name(), "development");
    }

    #[test]
    fn production_runtime_uses_isolated_home_and_label() {
        let runtime = AgentRuntime::resolve(false, PathBuf::from("/Users/test"));
        assert_eq!(runtime.config_dir, PathBuf::from("/Users/test/.kn"));
        assert_eq!(runtime.agent_dir(), PathBuf::from("/Users/test/.kn/agent"));
        assert_eq!(runtime.launchd_label, "com.kn.agent");
        assert_eq!(runtime.environment_name(), "production");
    }

    #[test]
    fn runtime_home_is_build_environment_isolated() {
        std::env::set_var("KN_HOME", "/tmp/kn-override");
        let runtime = AgentRuntime::resolve(true, PathBuf::from("/Users/test"));
        assert_eq!(runtime.config_dir, PathBuf::from("/Users/test/.kn-dev"));

        let runtime = AgentRuntime::resolve(false, PathBuf::from("/Users/test"));
        assert_eq!(runtime.config_dir, PathBuf::from("/Users/test/.kn"));
        std::env::remove_var("KN_HOME");
    }

    #[test]
    fn plist_update_requires_restart_even_without_binary_update() {
        assert!(!should_restart_agent(false, false, false));
        assert!(should_restart_agent(true, false, false));
        assert!(should_restart_agent(false, true, false));
        assert!(should_restart_agent(true, true, false));
        assert!(should_restart_agent(false, false, true));
    }

    #[test]
    fn plist_values_escape_xml_special_characters() {
        assert_eq!(
            escape_plist_value("/Volumes/R&D/<kn>\"agent\""),
            "/Volumes/R&amp;D/&lt;kn&gt;&quot;agent&quot;"
        );
    }
}
