pub mod discovery;
pub mod exec;
pub mod install;
pub mod manifest;

pub use discovery::extension_path;
pub use exec::exec_extension;

use clap::CommandFactory;

use crate::config;
use crate::useragent;
use crate::Cli;

/// Result of the single-pass argument walk.
pub(crate) struct ParsedArgs {
    /// The first non-flag positional argument (the subcommand candidate).
    pub candidate: Option<String>,
    /// All arguments after the candidate (to be forwarded to the extension).
    pub ext_args: Vec<String>,
    /// Pre-parsed global flag values.
    pub globals: PreParsedGlobals,
}

pub(crate) struct PreParsedGlobals {
    pub output: Option<String>,
    pub yes: bool,
    pub agent: bool,
    pub read_only: bool,
    pub org: Option<String>,
}

/// Parse the CLI arguments in a single left-to-right pass.
/// Extracts the subcommand candidate, extension args, and global flag values.
pub(crate) fn parse_extension_args(args: &[String]) -> ParsedArgs {
    let mut globals = PreParsedGlobals {
        output: None,
        yes: false,
        agent: false,
        read_only: false,
        org: None,
    };
    let mut candidate: Option<String> = None;
    let mut ext_args: Vec<String> = Vec::new();

    let mut iter = args.iter().skip(1); // skip binary name
    while let Some(arg) = iter.next() {
        // Once we have found the candidate, everything else is an extension arg.
        if candidate.is_some() {
            ext_args.push(arg.clone());
            continue;
        }

        match arg.as_str() {
            // Space-separated value flags: --output table, -o table, --org prod
            "--output" | "-o" => {
                if let Some(val) = iter.next() {
                    globals.output = Some(val.clone());
                }
            }
            "--org" => {
                if let Some(val) = iter.next() {
                    globals.org = Some(val.clone());
                }
            }
            // Boolean flags
            "--yes" | "-y" => globals.yes = true,
            "--agent" => globals.agent = true,
            "--read-only" => globals.read_only = true,
            // Equals-syntax value flags: --output=table, --org=prod
            s if s.starts_with("--output=") => {
                globals.output = Some(s["--output=".len()..].to_string());
            }
            s if s.starts_with("--org=") => {
                globals.org = Some(s["--org=".len()..].to_string());
            }
            // Short flag with attached value: -ojson, -otable
            s if s.starts_with("-o") && s.len() > 2 => {
                globals.output = Some(s[2..].to_string());
            }
            // Any other flag - skip.
            s if s.starts_with('-') => {}
            // First non-flag argument is the subcommand candidate.
            _ => {
                candidate = Some(arg.clone());
            }
        }
    }

    ParsedArgs {
        candidate,
        ext_args,
        globals,
    }
}

/// Check if a name matches any built-in pup command (including aliases).
/// Uses .build() to materialize clap's auto-generated subcommands (like "help")
/// so nothing can slip through.
pub(crate) fn is_builtin_command(name: &str) -> bool {
    let mut cmd = Cli::command();
    cmd.build();
    let result = cmd
        .get_subcommands()
        .any(|sub| sub.get_name() == name || sub.get_all_aliases().any(|alias| alias == name));
    result
}

impl PreParsedGlobals {
    pub fn apply_to(&self, cfg: &mut config::Config) {
        if let Some(ref fmt) = self.output {
            if let Ok(f) = fmt.parse() {
                cfg.output_format = f;
            }
        }
        if self.yes {
            cfg.auto_approve = true;
        }
        cfg.agent_mode = self.agent || useragent::is_agent_mode();
        if cfg.agent_mode {
            cfg.auto_approve = true;
        }
        if self.read_only {
            cfg.read_only = true;
        }
        if let Some(ref org) = self.org {
            #[cfg(all(not(feature = "browser"), not(target_arch = "wasm32")))]
            config::apply_org_override(cfg, org.clone());
            #[cfg(any(feature = "browser", target_arch = "wasm32"))]
            {
                cfg.org = Some(org.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn test_parse_simple_extension() {
        let parsed = parse_extension_args(&args("pup terraform plan --workspace prod"));
        assert_eq!(parsed.candidate.as_deref(), Some("terraform"));
        assert_eq!(parsed.ext_args, vec!["plan", "--workspace", "prod"]);
    }

    #[test]
    fn test_parse_output_space() {
        let parsed = parse_extension_args(&args("pup --output table terraform plan"));
        assert_eq!(parsed.candidate.as_deref(), Some("terraform"));
        assert_eq!(parsed.ext_args, vec!["plan"]);
        assert_eq!(parsed.globals.output.as_deref(), Some("table"));
    }

    #[test]
    fn test_parse_output_equals() {
        let parsed = parse_extension_args(&args("pup --output=table terraform"));
        assert_eq!(parsed.candidate.as_deref(), Some("terraform"));
        assert_eq!(parsed.globals.output.as_deref(), Some("table"));
    }

    #[test]
    fn test_parse_short_output_space() {
        let parsed = parse_extension_args(&args("pup -o json terraform"));
        assert_eq!(parsed.candidate.as_deref(), Some("terraform"));
        assert_eq!(parsed.globals.output.as_deref(), Some("json"));
    }

    #[test]
    fn test_parse_short_output_attached() {
        let parsed = parse_extension_args(&args("pup -ojson terraform"));
        assert_eq!(parsed.candidate.as_deref(), Some("terraform"));
        assert_eq!(parsed.globals.output.as_deref(), Some("json"));
    }

    #[test]
    fn test_parse_org_space() {
        let parsed = parse_extension_args(&args("pup --org staging terraform"));
        assert_eq!(parsed.candidate.as_deref(), Some("terraform"));
        assert_eq!(parsed.globals.org.as_deref(), Some("staging"));
    }

    #[test]
    fn test_parse_org_equals() {
        let parsed = parse_extension_args(&args("pup --org=staging terraform"));
        assert_eq!(parsed.candidate.as_deref(), Some("terraform"));
        assert_eq!(parsed.globals.org.as_deref(), Some("staging"));
    }

    #[test]
    fn test_parse_boolean_flags() {
        let parsed = parse_extension_args(&args("pup --yes --agent --read-only terraform"));
        assert_eq!(parsed.candidate.as_deref(), Some("terraform"));
        assert!(parsed.globals.yes);
        assert!(parsed.globals.agent);
        assert!(parsed.globals.read_only);
    }

    #[test]
    fn test_parse_short_yes() {
        let parsed = parse_extension_args(&args("pup -y terraform"));
        assert_eq!(parsed.candidate.as_deref(), Some("terraform"));
        assert!(parsed.globals.yes);
    }

    #[test]
    fn test_parse_all_globals() {
        let parsed = parse_extension_args(&args("pup --org=staging -ojson --yes terraform plan"));
        assert_eq!(parsed.candidate.as_deref(), Some("terraform"));
        assert_eq!(parsed.ext_args, vec!["plan"]);
        assert_eq!(parsed.globals.org.as_deref(), Some("staging"));
        assert_eq!(parsed.globals.output.as_deref(), Some("json"));
        assert!(parsed.globals.yes);
    }

    #[test]
    fn test_parse_no_args() {
        let parsed = parse_extension_args(&args("pup"));
        assert!(parsed.candidate.is_none());
        assert!(parsed.ext_args.is_empty());
    }

    #[test]
    fn test_parse_only_flags() {
        let parsed = parse_extension_args(&args("pup --help"));
        assert!(parsed.candidate.is_none());
    }

    #[test]
    fn test_parse_output_missing_value() {
        let parsed = parse_extension_args(&args("pup --output"));
        assert!(parsed.candidate.is_none());
        assert!(parsed.globals.output.is_none());
    }

    #[test]
    fn test_parse_extension_args_forwarded() {
        let parsed = parse_extension_args(&args(
            "pup terraform plan --workspace prod --var-file vars.tfvars",
        ));
        assert_eq!(parsed.candidate.as_deref(), Some("terraform"));
        assert_eq!(
            parsed.ext_args,
            vec!["plan", "--workspace", "prod", "--var-file", "vars.tfvars"]
        );
    }

    #[test]
    fn test_is_builtin_monitors() {
        assert!(is_builtin_command("monitors"));
    }

    #[test]
    fn test_is_builtin_unknown() {
        assert!(!is_builtin_command("terraform"));
    }

    #[test]
    fn test_is_builtin_extension() {
        assert!(is_builtin_command("extension"));
    }
}
