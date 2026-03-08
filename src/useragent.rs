use crate::version;

#[allow(dead_code)]
pub struct AgentInfo {
    pub name: String,
    pub detected: bool,
}

struct AgentDetector {
    name: &'static str,
    env_vars: &'static [&'static str],
}

/// Table-driven AI agent detection, checked in priority order.
static AGENT_DETECTORS: &[AgentDetector] = &[
    AgentDetector {
        name: "claude-code",
        env_vars: &["CLAUDECODE", "CLAUDE_CODE"],
    },
    AgentDetector {
        name: "cursor",
        env_vars: &["CURSOR_AGENT"],
    },
    AgentDetector {
        name: "codex",
        env_vars: &["CODEX", "OPENAI_CODEX"],
    },
    AgentDetector {
        name: "opencode",
        env_vars: &["OPENCODE"],
    },
    AgentDetector {
        name: "aider",
        env_vars: &["AIDER"],
    },
    AgentDetector {
        name: "cline",
        env_vars: &["CLINE"],
    },
    AgentDetector {
        name: "windsurf",
        env_vars: &["WINDSURF_AGENT"],
    },
    AgentDetector {
        name: "github-copilot",
        env_vars: &["GITHUB_COPILOT"],
    },
    AgentDetector {
        name: "amazon-q",
        env_vars: &["AMAZON_Q", "AWS_Q_DEVELOPER"],
    },
    AgentDetector {
        name: "gemini-code",
        env_vars: &["GEMINI_CODE_ASSIST"],
    },
    AgentDetector {
        name: "sourcegraph-cody",
        env_vars: &["SRC_CODY"],
    },
    AgentDetector {
        name: "generic-agent",
        env_vars: &["AGENT"],
    },
];

fn is_env_truthy(key: &str) -> bool {
    match std::env::var(key) {
        Ok(val) => matches!(val.to_lowercase().as_str(), "1" | "true"),
        Err(_) => false,
    }
}

pub fn detect_agent_info() -> AgentInfo {
    for detector in AGENT_DETECTORS {
        for env_var in detector.env_vars {
            if is_env_truthy(env_var) {
                return AgentInfo {
                    name: detector.name.to_string(),
                    detected: true,
                };
            }
        }
    }
    AgentInfo {
        name: String::new(),
        detected: false,
    }
}

fn is_env_falsy(key: &str) -> bool {
    match std::env::var(key) {
        Ok(val) => matches!(val.to_lowercase().as_str(), "0" | "false"),
        Err(_) => false,
    }
}

pub fn is_agent_mode() -> bool {
    if is_env_falsy("FORCE_AGENT_MODE") {
        return false;
    }
    is_env_truthy("FORCE_AGENT_MODE") || detect_agent_info().detected
}

#[allow(dead_code)]
pub fn get() -> String {
    let agent = detect_agent_info();
    let base = format!(
        "pup/{} (rust; os {}; arch {}",
        version::VERSION,
        std::env::consts::OS,
        std::env::consts::ARCH,
    );
    if agent.detected {
        format!("{}; ai-agent {})", base, agent.name)
    } else {
        format!("{})", base)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_env_truthy() {
        // These tests use env vars that shouldn't be set in normal environments
        std::env::set_var("__PUP_TEST_TRUE__", "true");
        assert!(is_env_truthy("__PUP_TEST_TRUE__"));

        std::env::set_var("__PUP_TEST_ONE__", "1");
        assert!(is_env_truthy("__PUP_TEST_ONE__"));

        std::env::set_var("__PUP_TEST_FALSE__", "false");
        assert!(!is_env_truthy("__PUP_TEST_FALSE__"));

        assert!(!is_env_truthy("__PUP_TEST_NONEXISTENT__"));

        // Clean up
        std::env::remove_var("__PUP_TEST_TRUE__");
        std::env::remove_var("__PUP_TEST_ONE__");
        std::env::remove_var("__PUP_TEST_FALSE__");
    }

    #[test]
    fn test_user_agent_format() {
        let ua = get();
        assert!(ua.starts_with("pup/"));
        assert!(ua.contains("rust"));
        assert!(ua.contains("os "));
        assert!(ua.contains("arch "));
    }

    #[test]
    fn test_agent_detectors_not_empty() {
        assert!(!AGENT_DETECTORS.is_empty());
        assert_eq!(AGENT_DETECTORS[0].name, "claude-code");
    }

    #[test]
    fn test_detect_agent_info_no_agent() {
        // Clear all agent env vars
        for det in AGENT_DETECTORS {
            for var in det.env_vars {
                std::env::remove_var(var);
            }
        }
        std::env::remove_var("FORCE_AGENT_MODE");

        let info = detect_agent_info();
        assert!(!info.detected);
        assert!(info.name.is_empty());
    }

    #[test]
    fn test_detect_agent_info_claude_code() {
        std::env::set_var("CLAUDE_CODE", "1");
        let info = detect_agent_info();
        assert!(info.detected);
        assert_eq!(info.name, "claude-code");
        std::env::remove_var("CLAUDE_CODE");
    }

    #[test]
    fn test_detect_agent_info_cursor() {
        // Clear higher-priority detectors
        std::env::remove_var("CLAUDECODE");
        std::env::remove_var("CLAUDE_CODE");
        std::env::set_var("CURSOR_AGENT", "true");
        let info = detect_agent_info();
        assert!(info.detected);
        assert_eq!(info.name, "cursor");
        std::env::remove_var("CURSOR_AGENT");
    }

    #[test]
    fn test_is_agent_mode_force() {
        std::env::set_var("FORCE_AGENT_MODE", "1");
        assert!(is_agent_mode());
        std::env::remove_var("FORCE_AGENT_MODE");
    }

    #[test]
    fn test_is_agent_mode_force_disable() {
        std::env::set_var("CLAUDE_CODE", "1");
        std::env::set_var("FORCE_AGENT_MODE", "0");
        assert!(!is_agent_mode());
        std::env::remove_var("FORCE_AGENT_MODE");
        std::env::remove_var("CLAUDE_CODE");
    }

    #[test]
    fn test_is_agent_mode_via_detector() {
        std::env::remove_var("FORCE_AGENT_MODE");
        std::env::set_var("CLAUDE_CODE", "true");
        assert!(is_agent_mode());
        std::env::remove_var("CLAUDE_CODE");
    }

    #[test]
    fn test_is_agent_mode_false() {
        std::env::remove_var("FORCE_AGENT_MODE");
        for det in AGENT_DETECTORS {
            for var in det.env_vars {
                std::env::remove_var(var);
            }
        }
        assert!(!is_agent_mode());
    }

    #[test]
    fn test_user_agent_with_detected_agent() {
        std::env::set_var("CLAUDE_CODE", "1");
        let ua = get();
        assert!(
            ua.contains("ai-agent claude-code"),
            "ua should contain agent info: {ua}"
        );
        std::env::remove_var("CLAUDE_CODE");
    }

    #[test]
    fn test_user_agent_without_agent() {
        for det in AGENT_DETECTORS {
            for var in det.env_vars {
                std::env::remove_var(var);
            }
        }
        let ua = get();
        assert!(
            !ua.contains("ai-agent"),
            "ua should not contain agent info: {ua}"
        );
        assert!(ua.ends_with(')'));
    }

    #[test]
    fn test_detect_agent_info_generic_agent() {
        for det in AGENT_DETECTORS {
            for var in det.env_vars {
                std::env::remove_var(var);
            }
        }
        std::env::set_var("AGENT", "1");
        let info = detect_agent_info();
        assert!(info.detected);
        assert_eq!(info.name, "generic-agent");
        std::env::remove_var("AGENT");
    }

    #[test]
    fn test_all_detectors_have_names() {
        for det in AGENT_DETECTORS {
            assert!(!det.name.is_empty());
            assert!(!det.env_vars.is_empty());
        }
    }
}
