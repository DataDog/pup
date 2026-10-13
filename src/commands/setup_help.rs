use anyhow::Result;
use clap::{Arg, ArgAction};
use serde::Deserialize;

const PUP_MANAGED_FLAGS: &[&str] = &["--site", "--headless"];
const USAGE: &str = "pup setup --product <PRODUCT> [OPTIONS] [PRODUCT FLAGS]";

#[derive(Debug, Deserialize)]
pub struct AiSetupHelp {
    global_flags: Vec<HelpFlag>,
    products: Vec<HelpProduct>,
}

#[derive(Debug, Deserialize)]
struct HelpFlag {
    name: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    description: String,
}

#[derive(Debug, Deserialize)]
struct HelpProduct {
    name: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    flags: Vec<HelpFlag>,
}

pub fn parse(json: &[u8]) -> Option<AiSetupHelp> {
    serde_json::from_slice(json).ok()
}

pub fn print(help: &AiSetupHelp) -> Result<()> {
    let mut root = crate::cli_command();
    root.build();
    let base = root
        .find_subcommand("setup")
        .ok_or_else(|| anyhow::anyhow!("setup command is not registered"))?;
    help_command(base, help).print_long_help()?;
    Ok(())
}

fn help_command(base: &clap::Command, help: &AiSetupHelp) -> clap::Command {
    let mut cmd = clap::Command::new("setup")
        .override_usage(USAGE)
        .after_long_help(products_section(&help.products));
    if let Some(about) = base.get_about() {
        cmd = cmd.about(about.clone());
    }
    if let Some(long_about) = base.get_long_about() {
        cmd = cmd.long_about(long_about.clone());
    }
    cmd.args(
        help.global_flags
            .iter()
            .filter(|flag| !PUP_MANAGED_FLAGS.contains(&flag.name.as_str()))
            .filter_map(flag_arg),
    )
    .args(
        base.get_arguments()
            .filter(|arg| arg.is_global_set())
            .cloned(),
    )
}

fn products_section(products: &[HelpProduct]) -> String {
    let name_width = products.iter().map(|p| p.name.len()).max().unwrap_or(0);
    let mut out = String::from("Products:\n");
    for product in products {
        let summary = match (
            product.display_name.is_empty(),
            product.description.is_empty(),
        ) {
            (false, false) => format!("{}: {}", product.display_name, product.description),
            (false, true) => product.display_name.clone(),
            _ => product.description.clone(),
        };
        out.push_str(&format!("  {:<name_width$}  {summary}\n", product.name));
        let labels: Vec<String> = product.flags.iter().map(flag_label).collect();
        let flag_width = labels.iter().map(String::len).max().unwrap_or(0);
        for (label, flag) in labels.iter().zip(&product.flags) {
            out.push_str(&format!(
                "      {label:<flag_width$}  {}\n",
                flag.description
            ));
        }
    }
    out
}

fn flag_arg(flag: &HelpFlag) -> Option<Arg> {
    let long = flag.name.strip_prefix("--")?.to_string();
    let arg = Arg::new(long.clone())
        .long(long)
        .help(flag.description.clone());
    Some(match value_name(flag) {
        Some(value) => arg.value_name(value).action(ArgAction::Set),
        None => arg.action(ArgAction::SetTrue),
    })
}

fn flag_label(flag: &HelpFlag) -> String {
    match value_name(flag) {
        Some(value) => format!("{} <{value}>", flag.name),
        None => flag.name.clone(),
    }
}

fn value_name(flag: &HelpFlag) -> Option<String> {
    (flag.kind != "boolean").then(|| {
        flag.name
            .trim_start_matches('-')
            .to_uppercase()
            .replace('-', "_")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "name": "ai-setup-cli",
        "version": "1.0.0",
        "usage": "ai-setup-cli --site <site> --product <product> [product flags]",
        "global_flags": [
            {"name": "--site", "type": "string", "required": true, "description": "Datadog site"},
            {"name": "--product", "type": "string", "required": false, "required_in_headless": true, "description": "Product to set up (see products)"},
            {"name": "--headless", "type": "boolean", "required": false, "description": "Run setup non-interactively"},
            {"name": "--auto-approve", "type": "boolean", "required": false, "description": "Auto-approve all bash commands"}
        ],
        "products": [
            {"name": "linux", "display_name": "Linux Observability", "description": "Metrics, logs and traces from Linux",
             "flags": [{"name": "--env", "type": "string", "required": false, "description": "Environment name"}]},
            {"name": "otel", "display_name": "Datadog OpenTelemetry", "description": "Direct OTLP export", "flags": []}
        ]
    }"#;

    fn sample() -> AiSetupHelp {
        parse(SAMPLE.as_bytes()).expect("sample help parses")
    }

    fn built_setup() -> clap::Command {
        let mut root = crate::cli_command();
        root.build();
        help_command(root.find_subcommand("setup").unwrap(), &sample())
    }

    #[test]
    fn parse_rejects_text_help() {
        assert!(parse(b"Interactive tool for Datadog setup").is_none());
    }

    #[test]
    fn text_help_uses_pup_usage_and_lists_products() {
        let mut cmd = built_setup();
        let text = cmd.render_long_help().to_string();
        assert!(text.contains(&format!("Usage: {USAGE}")), "{text}");
        assert!(text.contains("--product <PRODUCT>"), "{text}");
        assert!(text.contains("--read-only"), "{text}");
        assert!(text.contains("Products:"), "{text}");
        assert!(text.contains("linux  Linux Observability: Metrics, logs and traces from Linux"));
        assert!(text.contains("--env <ENV>  Environment name"), "{text}");
        assert!(!text.contains("--headless"), "{text}");
        assert!(!text.contains("ai-setup-cli"), "{text}");
    }
}
