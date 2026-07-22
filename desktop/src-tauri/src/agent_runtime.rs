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
        let configured_home = std::env::var("KN_HOME")
            .ok()
            .map(PathBuf::from)
            .filter(is_safe_home);
        Self::resolve(cfg!(debug_assertions), home, configured_home)
    }

    fn resolve(is_debug_build: bool, home: PathBuf, configured_home: Option<PathBuf>) -> Self {
        let development_home = home.join(".kn-dev");
        let production_home = home.join(".kn");
        let configured_home = configured_home.filter(|path| {
            if is_debug_build {
                path != &production_home
            } else {
                path != &development_home
            }
        });

        if is_debug_build {
            Self {
                environment: AgentRuntimeEnvironment::Development,
                config_dir: configured_home.unwrap_or(development_home),
                launchd_label: "com.kn.agent.dev",
            }
        } else {
            Self {
                environment: AgentRuntimeEnvironment::Production,
                config_dir: configured_home.unwrap_or(production_home),
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

fn is_safe_home(path: &PathBuf) -> bool {
    path.is_absolute()
        && !path
            .components()
            .any(|component| component == std::path::Component::ParentDir)
}

pub(crate) fn should_restart_agent(agent_updated: bool, plist_updated: bool) -> bool {
    agent_updated || plist_updated
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
        let runtime = AgentRuntime::resolve(true, PathBuf::from("/Users/test"), None);
        assert_eq!(runtime.config_dir, PathBuf::from("/Users/test/.kn-dev"));
        assert_eq!(runtime.launchd_label, "com.kn.agent.dev");
        assert_eq!(runtime.environment_name(), "development");
    }

    #[test]
    fn explicit_home_is_preserved_for_each_runtime() {
        let runtime = AgentRuntime::resolve(
            false,
            PathBuf::from("/Users/test"),
            Some(PathBuf::from("/Volumes/kn-production")),
        );
        assert_eq!(runtime.config_dir, PathBuf::from("/Volumes/kn-production"));
        assert_eq!(
            runtime.agent_dir(),
            PathBuf::from("/Volumes/kn-production/agent")
        );
        assert_eq!(runtime.launchd_label, "com.kn.agent");
    }

    #[test]
    fn invalid_home_does_not_cross_runtime_boundaries() {
        assert!(!is_safe_home(&PathBuf::from("relative/kn")));
        assert!(!is_safe_home(&PathBuf::from("/Users/test/../other")));
        assert!(is_safe_home(&PathBuf::from("/Users/test/.kn-dev")));
    }

    #[test]
    fn debug_runtime_rejects_the_production_default_home() {
        let runtime = AgentRuntime::resolve(
            true,
            PathBuf::from("/Users/test"),
            Some(PathBuf::from("/Users/test/.kn")),
        );

        assert_eq!(runtime.config_dir, PathBuf::from("/Users/test/.kn-dev"));
    }

    #[test]
    fn production_runtime_rejects_the_development_default_home() {
        let runtime = AgentRuntime::resolve(
            false,
            PathBuf::from("/Users/test"),
            Some(PathBuf::from("/Users/test/.kn-dev")),
        );

        assert_eq!(runtime.config_dir, PathBuf::from("/Users/test/.kn"));
    }

    #[test]
    fn plist_update_requires_restart_even_without_binary_update() {
        assert!(!should_restart_agent(false, false));
        assert!(should_restart_agent(true, false));
        assert!(should_restart_agent(false, true));
        assert!(should_restart_agent(true, true));
    }

    #[test]
    fn plist_values_escape_xml_special_characters() {
        assert_eq!(
            escape_plist_value("/Volumes/R&D/<kn>\"agent\""),
            "/Volumes/R&amp;D/&lt;kn&gt;&quot;agent&quot;"
        );
    }
}
