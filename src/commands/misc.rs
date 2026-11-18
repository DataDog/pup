use anyhow::Result;
use datadog_api_client::datadogV1::api_authentication::AuthenticationAPI;
use datadog_api_client::datadogV1::api_ip_ranges::IPRangesAPI;

use crate::config::Config;
use crate::formatter;

pub async fn ip_ranges(cfg: &Config) -> Result<()> {
    let api = crate::make_api!(IPRangesAPI, cfg);
    let resp = api
        .get_ip_ranges()
        .await
        .map_err(|e| anyhow::anyhow!("failed to get IP ranges: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn status(cfg: &Config) -> Result<()> {
    if !cfg.has_bearer_token() && !cfg.has_api_keys() && !cfg.has_browser_session() {
        let transformed = serde_json::json!({
            "message": "no credentials configured",
            "status": "unauthenticated"
        });
        return formatter::output(cfg, &transformed);
    }
    let api = crate::make_api!(AuthenticationAPI, cfg);
    let _resp = api
        .validate()
        .await
        .map_err(|e| anyhow::anyhow!("failed to validate API keys: {e:?}"))?;
    let transformed = serde_json::json!({
        "message": "API is operational",
        "status": "ok"
    });
    formatter::output(cfg, &transformed)
}

#[cfg(test)]
mod tests {

    use crate::test_support::*;

    #[tokio::test]
    async fn test_misc_ip_ranges() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{}"#).await;
        let _ = super::ip_ranges(&cfg).await;
        cleanup_env();
    }
}
