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

pub fn print(help: &AiSetupHelp, json: bool) -> Result<()> {
    let mut root = crate::cli_command();
    root.build();
    let base = root
        .find_subcommand("setup")
        .ok_or_else(|| anyhow::anyhow!("setup command is not registered"))?;
    let mut cmd = help_command(base, help);
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&agent_schema(&root, &cmd, help))?
        );
    } else {
        cmd.print_long_help()?;
    }
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

fn agent_schema(
    root: &clap::Command,
    cmd: &clap::Command,
    help: &AiSetupHelp,
) -> serde_json::Value {
    let mut schema = crate::build_agent_schema_scoped(root, cmd, &["setup"]);
    let products: Vec<serde_json::Value> = help.products.iter().map(product_schema).collect();
    if let Some(command) = schema
        .pointer_mut("/commands/0")
        .and_then(serde_json::Value::as_object_mut)
    {
        command.insert("products".into(), serde_json::Value::Array(products));
    }
    schema
}

fn product_schema(product: &HelpProduct) -> serde_json::Value {
    let cmd =
        clap::Command::new(product.name.clone()).args(product.flags.iter().filter_map(flag_arg));
    let flags = crate::build_command_schema(&cmd, "setup")
        .get("flags")
        .cloned()
        .unwrap_or_else(|| serde_json::json!([]));
    serde_json::json!({
        "name": product.name,
        "display_name": product.display_name,
        "description": product.description,
        "flags": flags,
    })
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
            {"name": "--product", "type": "string", "required": false, "description": "Product to set up (see products)"},
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

    fn built_setup() -> (clap::Command, clap::Command) {
        let mut root = crate::cli_command();
        root.build();
        let cmd = help_command(root.find_subcommand("setup").unwrap(), &sample());
        (root, cmd)
    }

    #[test]
    fn parse_rejects_text_help() {
        assert!(parse(b"Interactive tool for Datadog setup").is_none());
    }

    #[test]
    fn agent_schema_uses_pup_shape_and_hides_pup_managed_flags() {
        let (root, cmd) = built_setup();
        let schema = agent_schema(&root, &cmd, &sample());
        assert_eq!(schema["command"], "setup");
        assert_eq!(schema["version"], crate::version::VERSION);
        let command = &schema["commands"][0];
        assert_eq!(command["full_path"], "setup");
        assert_eq!(command["read_only"], false);
        let flags: Vec<&str> = command["flags"]
            .as_array()
            .unwrap()
            .iter()
            .map(|flag| flag["name"].as_str().unwrap())
            .collect();
        assert!(flags.contains(&"--product"), "{flags:?}");
        assert!(flags.contains(&"--auto-approve"), "{flags:?}");
        assert!(!flags.contains(&"--site"), "{flags:?}");
        assert!(!flags.contains(&"--headless"), "{flags:?}");
        let product = &command["products"][0];
        assert_eq!(product["name"], "linux");
        assert_eq!(product["flags"][0]["name"], "--env");
        assert_eq!(product["flags"][0]["type"], "string");
        assert!(product["flags"][0]["arity"].is_object());
        assert_eq!(command["products"][1]["flags"], serde_json::json!([]));
    }

    #[test]
    fn text_help_uses_pup_usage_and_lists_products() {
        let (_, mut cmd) = built_setup();
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
