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
        name: "pi-dev",
        env_vars: &["PI_CODING_AGENT"],
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

fn is_env_present(key: &str) -> bool {
    std::env::var(key).is_ok_and(|val| !val.is_empty())
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
    if is_env_present("DEVIN_SESSION_ID") {
        return AgentInfo {
            name: "devin".to_string(),
            detected: true,
        };
    }
    AgentInfo {
        name: String::new(),
        detected: false,
    }
}

pub fn is_agent_mode() -> bool {
    // `PUP_AGENT_MODE` is injected into extension subprocesses so a child `pup`
    // call inherits the parent's agent mode.
    is_env_truthy("FORCE_AGENT_MODE")
        || is_env_truthy("PUP_AGENT_MODE")
        || detect_agent_info().detected
}

#[allow(dead_code)]
pub fn get() -> String {
    get_with_command(None)
}

/// Returns the underlying Datadog SDK's `name/version` token (e.g.
/// `datadog-api-client-rust/0.30.0`) for inclusion in pup's User-Agent.
/// `None` when the SDK isn't compiled in (any non-default-feature build).
#[cfg(any(feature = "native", feature = "wasi", feature = "browser"))]
fn sdk_token() -> Option<&'static str> {
    datadog_api_client::datadog::DEFAULT_USER_AGENT
        .as_str()
        .split_whitespace()
        .next()
}

#[cfg(not(any(feature = "native", feature = "wasi", feature = "browser")))]
fn sdk_token() -> Option<&'static str> {
    None
}

/// Build the User-Agent string, optionally including a command identifier
/// so that audit logs can differentiate which pup command made the request.
///
/// Format: `pup/<ver> (rust <rustver>; os <os>; arch <arch>[; ai-agent <name>][; sdk <sdk_name/ver>][; cmd <cmd>])`
///
/// Each parenthesized comment must be a `key value` 2-tuple so the
/// smart-edge audit logger's `client_telemetry` metric parser accepts it
/// (see `dd-source/.../web_traffic_dashboard/client_sdk_info.go`). A bare
/// token like `rust` (without a version) causes the parser to reject the
/// entire UA, dropping all pup product-version telemetry — that's why we
/// always emit `rust <version>` (falling back to `rust unknown`).
pub fn get_with_command(command: Option<&str>) -> String {
    let agent = detect_agent_info();
    let base = format!(
        "pup/{} (rust {}; os {}; arch {}",
        version::VERSION,
        version::rustc_version(),
        std::env::consts::OS,
        std::env::consts::ARCH,
    );
    let with_agent = if agent.detected {
        format!("{}; ai-agent {}", base, agent.name)
    } else {
        base
    };
    let with_sdk = match sdk_token() {
        Some(sdk) => format!("{}; sdk {}", with_agent, sdk),
        None => with_agent,
    };
    if let Some(cmd) = command {
        format!("{}; cmd {})", with_sdk, cmd)
    } else {
        format!("{})", with_sdk)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::ENV_LOCK;

    fn clear_all_agent_vars() {
        for det in AGENT_DETECTORS {
            for var in det.env_vars {
                std::env::remove_var(var);
            }
        }
        std::env::remove_var("FORCE_AGENT_MODE");
        std::env::remove_var("PUP_AGENT_MODE");
        std::env::remove_var("DEVIN_SESSION_ID");
    }

    #[test]
    fn test_is_env_truthy() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::set_var("__PUP_TEST_TRUE__", "true");
        assert!(is_env_truthy("__PUP_TEST_TRUE__"));
        std::env::set_var("__PUP_TEST_ONE__", "1");
        assert!(is_env_truthy("__PUP_TEST_ONE__"));
        std::env::set_var("__PUP_TEST_FALSE__", "false");
        assert!(!is_env_truthy("__PUP_TEST_FALSE__"));
        assert!(!is_env_truthy("__PUP_TEST_NONEXISTENT__"));
        std::env::remove_var("__PUP_TEST_TRUE__");
        std::env::remove_var("__PUP_TEST_ONE__");
        std::env::remove_var("__PUP_TEST_FALSE__");
    }

    #[test]
    fn test_is_env_present() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::set_var("__PUP_TEST_PRESENT__", "some-session-id");
        assert!(is_env_present("__PUP_TEST_PRESENT__"));
        std::env::set_var("__PUP_TEST_PRESENT_EMPTY__", "");
        assert!(!is_env_present("__PUP_TEST_PRESENT_EMPTY__"));
        assert!(!is_env_present("__PUP_TEST_PRESENT_MISSING__"));
        std::env::remove_var("__PUP_TEST_PRESENT__");
        std::env::remove_var("__PUP_TEST_PRESENT_EMPTY__");
    }

    #[test]
    fn test_user_agent_format() {
        let _guard = ENV_LOCK.blocking_lock();
        clear_all_agent_vars();
        let ua = get();
        assert!(ua.starts_with("pup/"));
        assert!(ua.contains("rust"));
        assert!(ua.contains("os "));
        assert!(ua.contains("arch "));
        assert!(!ua.contains("cmd "));
    }

    #[test]
    fn test_user_agent_with_command() {
        let _guard = ENV_LOCK.blocking_lock();
        clear_all_agent_vars();
        let ua = get_with_command(Some("security-findings-analyze"));
        assert!(ua.starts_with("pup/"));
        assert!(ua.contains("; cmd security-findings-analyze)"));
    }

    /// Mirrors the smart-edge audit logger's `client_telemetry` parser
    /// (`dd-source/.../web_traffic_dashboard/client_sdk_info.go`): each
    /// parenthesized comment must split into exactly 2 tokens on the first
    /// space. If this assertion fails, the metric stops firing for pup
    /// until the UA is fixed.
    #[test]
    fn test_user_agent_parses_for_smart_edge_telemetry() {
        let _guard = ENV_LOCK.blocking_lock();
        clear_all_agent_vars();
        for ua in [
            get_with_command(None),
            get_with_command(Some("monitors-list")),
        ] {
            let open = ua
                .find('(')
                .unwrap_or_else(|| panic!("UA missing '(' : {ua}"));
            let close = ua
                .rfind(')')
                .unwrap_or_else(|| panic!("UA missing ')' : {ua}"));
            let inside = &ua[open + 1..close];
            for raw in inside.split(';') {
                let trimmed = raw.trim();
                let mut parts = trimmed.splitn(2, ' ');
                let key = parts.next().unwrap_or("");
                let value = parts.next();
                assert!(
                    value.is_some(),
                    "smart-edge parser requires `key value` tuples — \
                     bare comment {trimmed:?} in {ua:?} would drop the entire UA",
                );
                assert!(!key.is_empty(), "empty key in comment {trimmed:?}");
                assert!(
                    !value.unwrap().is_empty(),
                    "empty value in comment {trimmed:?}",
                );
            }
        }
    }

    #[test]
    fn test_user_agent_with_no_command() {
        let _guard = ENV_LOCK.blocking_lock();
        clear_all_agent_vars();
        let ua = get_with_command(None);
        assert!(!ua.contains("cmd "));
        assert_eq!(ua, get());
    }

    #[test]
    fn test_agent_detectors_not_empty() {
        assert!(!AGENT_DETECTORS.is_empty());
        assert_eq!(AGENT_DETECTORS[0].name, "claude-code");
    }

    #[test]
    fn test_detect_agent_info_no_agent() {
        let _guard = ENV_LOCK.blocking_lock();
        clear_all_agent_vars();
        let info = detect_agent_info();
        assert!(!info.detected);
        assert!(info.name.is_empty());
    }

    #[test]
    fn test_detect_agent_info_claude_code() {
        let _guard = ENV_LOCK.blocking_lock();
        clear_all_agent_vars();
        std::env::set_var("CLAUDE_CODE", "1");
        let info = detect_agent_info();
        assert!(info.detected);
        assert_eq!(info.name, "claude-code");
        std::env::remove_var("CLAUDE_CODE");
    }

    #[test]
    fn test_detect_agent_info_cursor() {
        let _guard = ENV_LOCK.blocking_lock();
        clear_all_agent_vars();
        std::env::set_var("CURSOR_AGENT", "true");
        let info = detect_agent_info();
        assert!(info.detected);
        assert_eq!(info.name, "cursor");
        std::env::remove_var("CURSOR_AGENT");
    }

    #[test]
    fn test_is_agent_mode_force() {
        let _guard = ENV_LOCK.blocking_lock();
        clear_all_agent_vars();
        std::env::set_var("FORCE_AGENT_MODE", "1");
        assert!(is_agent_mode());
        std::env::remove_var("FORCE_AGENT_MODE");
    }

    #[test]
    fn test_is_agent_mode_via_pup_env() {
        // PUP_AGENT_MODE is injected into extension subprocesses so a child `pup`
        // call inherits the parent's agent mode.
        let _guard = ENV_LOCK.blocking_lock();
        clear_all_agent_vars();
        std::env::set_var("PUP_AGENT_MODE", "true");
        assert!(is_agent_mode());
        std::env::remove_var("PUP_AGENT_MODE");
    }

    #[test]
    fn test_is_agent_mode_via_detector() {
        let _guard = ENV_LOCK.blocking_lock();
        clear_all_agent_vars();
        std::env::set_var("CLAUDE_CODE", "true");
        assert!(is_agent_mode());
        std::env::remove_var("CLAUDE_CODE");
    }

    #[test]
    fn test_is_agent_mode_false() {
        let _guard = ENV_LOCK.blocking_lock();
        clear_all_agent_vars();
        assert!(!is_agent_mode());
    }

    #[test]
    fn test_user_agent_with_detected_agent() {
        let _guard = ENV_LOCK.blocking_lock();
        clear_all_agent_vars();
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
        let _guard = ENV_LOCK.blocking_lock();
        clear_all_agent_vars();
        let ua = get();
        assert!(
            !ua.contains("ai-agent"),
            "ua should not contain agent info: {ua}"
        );
        assert!(ua.ends_with(')'));
    }

    #[test]
    fn test_detect_agent_info_pi_dev() {
        let _guard = ENV_LOCK.blocking_lock();
        clear_all_agent_vars();
        std::env::set_var("PI_CODING_AGENT", "true");
        let info = detect_agent_info();
        assert!(info.detected);
        assert_eq!(info.name, "pi-dev");
        std::env::remove_var("PI_CODING_AGENT");
    }

    #[test]
    fn test_detect_agent_info_pi_dev_falsy() {
        let _guard = ENV_LOCK.blocking_lock();
        clear_all_agent_vars();
        std::env::set_var("PI_CODING_AGENT", "false");
        let info = detect_agent_info();
        assert!(!info.detected);
        std::env::remove_var("PI_CODING_AGENT");
    }

    #[test]
    fn test_detect_agent_info_devin() {
        let _guard = ENV_LOCK.blocking_lock();
        clear_all_agent_vars();
        std::env::set_var("DEVIN_SESSION_ID", "devin-run-abc123");
        let info = detect_agent_info();
        assert!(info.detected);
        assert_eq!(info.name, "devin");
        std::env::remove_var("DEVIN_SESSION_ID");
    }

    #[test]
    fn test_detect_agent_info_devin_empty_not_detected() {
        let _guard = ENV_LOCK.blocking_lock();
        clear_all_agent_vars();
        std::env::set_var("DEVIN_SESSION_ID", "");
        let info = detect_agent_info();
        assert!(!info.detected);
        std::env::remove_var("DEVIN_SESSION_ID");
    }

    #[test]
    fn test_detect_agent_info_generic_agent() {
        let _guard = ENV_LOCK.blocking_lock();
        clear_all_agent_vars();
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
