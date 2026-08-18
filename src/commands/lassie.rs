//! Shared helpers for the Datadog Bits AI (Lassie NG) API.
//!
//! Used by both `bits ask` and `acp serve` to avoid duplicating agent
//! resolution, agent creation, and auth logic.

use anyhow::Result;

pub const LASSIE_BASE: &str = "/api/unstable/lassie-ng/v1";

/// Add auth headers to a request builder.
pub fn add_auth(
    req: reqwest::RequestBuilder,
    access_token: Option<&str>,
    api_key: Option<&str>,
    app_key: Option<&str>,
) -> Result<reqwest::RequestBuilder> {
    let req = req.header("User-Agent", crate::useragent::get());
    if let Some(token) = access_token {
        return Ok(req.header("Authorization", format!("Bearer {token}")));
    }
    if let (Some(ak), Some(apk)) = (api_key, app_key) {
        return Ok(req
            .header("DD-API-KEY", ak)
            .header("DD-APPLICATION-KEY", apk));
    }
    anyhow::bail!("no authentication configured")
}

/// Resolve the first available Bits AI agent ID from the API.
///
/// If `auto_create` is true and no agents are found, creates a new agent.
#[cfg(not(target_arch = "wasm32"))]
pub async fn resolve_agent_id(
    app_base: &str,
    access_token: Option<&str>,
    api_key: Option<&str>,
    app_key: Option<&str>,
    auto_create: bool,
) -> Result<String> {
    let url = format!("{app_base}{LASSIE_BASE}/agents?limit=1");
    let client = reqwest::Client::new();
    let req = client
        .get(&url)
        .header("Accept", "application/json")
        .header("User-Agent", crate::useragent::get());
    let req = add_auth(req, access_token, api_key, app_key)?;

    let resp = req
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to list Bits AI agents: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("GET /agents failed (HTTP {status}): {body}");
    }

    let val: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to parse agents response: {e}"))?;

    let agents = val.as_array().ok_or_else(|| {
        anyhow::anyhow!(
            "Unexpected response format from Bits AI agents API.\n\
             Expected a JSON array but got: {}\n\
             This may indicate an API version mismatch — please report this at\n\
             https://github.com/DataDog/pup/issues if the issue persists.",
            serde_json::to_string(&val).unwrap_or_else(|_| "<unparseable>".to_string())
        )
    })?;

    if agents.is_empty() {
        if auto_create {
            eprintln!("No Bits AI agents found — creating one automatically...");
            return create_agent(app_base, access_token, api_key, app_key).await;
        }
        anyhow::bail!(
            "No Bits AI agents found in your Datadog organization. Pass --auto-create to create one."
        );
    }

    let id = agents[0]
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Agent missing 'id' field"))?;

    Ok(id.to_string())
}

/// Create a new Bits AI agent and return its ID.
#[cfg(not(target_arch = "wasm32"))]
pub async fn create_agent(
    app_base: &str,
    access_token: Option<&str>,
    api_key: Option<&str>,
    app_key: Option<&str>,
) -> Result<String> {
    let url = format!("{app_base}{LASSIE_BASE}/agents");
    let body = serde_json::json!({
        "name": "Pup CLI Agent",
        "description": "Auto-created by pup CLI for bits ask",
    });

    let client = reqwest::Client::new();
    let req = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .header("User-Agent", crate::useragent::get());
    let req = add_auth(req, access_token, api_key, app_key)?;

    let resp = req
        .json(&body)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create Bits AI agent: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let err_body = resp.text().await.unwrap_or_default();
        anyhow::bail!("Failed to create Bits AI agent (HTTP {status}): {err_body}");
    }

    let val: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to parse create-agent response: {e}"))?;

    let id = val
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Create-agent response missing 'id' field"))?;

    eprintln!("Created Bits AI agent: {id}");
    Ok(id.to_string())
}
