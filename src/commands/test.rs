use anyhow::Result;

use crate::config::Config;

fn mask_key(key: &str) -> String {
    if key.len() <= 12 {
        return "*".repeat(key.len());
    }
    format!("{}...{}", &key[..8], &key[key.len() - 4..])
}

pub fn run(cfg: &Config) -> Result<()> {
    println!("Site: {}", cfg.site);
    println!("API host: {}", cfg.api_host());

    if let Some(ref api_key) = cfg.api_key {
        println!("API Key: {}", mask_key(api_key));
    } else {
        println!("API Key: not set");
    }

    if let Some(ref app_key) = cfg.app_key {
        println!("App Key: {}", mask_key(app_key));
    } else {
        println!("App Key: not set");
    }

    if cfg.has_bearer_token() {
        println!("Bearer Token: configured");
    }

    println!("Output: {}", cfg.output_format);

    Ok(())
}
