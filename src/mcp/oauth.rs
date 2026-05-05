use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::auth::callback::CallbackServer;
use crate::auth::pkce;
use crate::auth::types::TokenSet;
use crate::config::Config;

const MCP_CLIENT_NAME: &str = "pup-mcp-proxy";

fn mcp_base(site: &str) -> String {
    format!("https://mcp.{}", site)
}

#[derive(Serialize)]
struct RegistrationRequest {
    client_name: String,
    redirect_uris: Vec<String>,
    grant_types: Vec<String>,
}

#[derive(Deserialize)]
struct RegistrationResponse {
    client_id: String,
    #[allow(dead_code)]
    client_name: String,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    token_type: String,
    expires_in: i64,
    #[serde(default)]
    refresh_token: String,
    #[serde(default)]
    scope: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct McpCredentials {
    pub client_id: String,
    pub site: String,
}

struct McpOAuthClient {
    site: String,
    http: reqwest::Client,
}

impl McpOAuthClient {
    fn new(site: &str) -> Self {
        Self {
            site: site.to_string(),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("failed to build HTTP client"),
        }
    }

    async fn register(&self, redirect_uri: &str) -> Result<McpCredentials> {
        let url = format!("{}/api/unstable/mcp-server/register", mcp_base(&self.site));

        let body = RegistrationRequest {
            client_name: MCP_CLIENT_NAME.to_string(),
            redirect_uris: vec![redirect_uri.to_string()],
            grant_types: vec![
                "authorization_code".to_string(),
                "refresh_token".to_string(),
            ],
        };

        let resp = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("MCP OAuth registration failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("MCP OAuth registration failed (HTTP {status}): {body}");
        }

        let reg: RegistrationResponse = resp
            .json()
            .await
            .context("failed to parse MCP registration response")?;

        Ok(McpCredentials {
            client_id: reg.client_id,
            site: self.site.clone(),
        })
    }

    fn build_authorization_url(
        &self,
        client_id: &str,
        redirect_uri: &str,
        state: &str,
        challenge: &pkce::PkceChallenge,
    ) -> String {
        let params = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("response_type", "code")
            .append_pair("client_id", client_id)
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("state", state)
            .append_pair("code_challenge", &challenge.challenge)
            .append_pair("code_challenge_method", &challenge.method)
            .finish();

        format!(
            "{}/api/unstable/mcp-server/authorize?{params}",
            mcp_base(&self.site)
        )
    }

    async fn exchange_code(
        &self,
        code: &str,
        redirect_uri: &str,
        code_verifier: &str,
        client_id: &str,
    ) -> Result<TokenSet> {
        let url = format!("{}/api/unstable/mcp-server/token", mcp_base(&self.site));

        let params = [
            ("grant_type", "authorization_code"),
            ("client_id", client_id),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("code_verifier", code_verifier),
        ];

        let resp = self
            .http
            .post(&url)
            .form(&params)
            .send()
            .await
            .context("MCP token exchange failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("MCP token exchange failed (HTTP {status}): {body}");
        }

        let token_resp: TokenResponse = resp
            .json()
            .await
            .context("failed to parse MCP token response")?;

        Ok(TokenSet {
            access_token: token_resp.access_token,
            refresh_token: token_resp.refresh_token,
            token_type: token_resp.token_type,
            expires_in: token_resp.expires_in,
            issued_at: Utc::now().timestamp(),
            scope: token_resp.scope,
            client_id: client_id.to_string(),
        })
    }

    async fn refresh_token(&self, refresh_token: &str, client_id: &str) -> Result<TokenSet> {
        let url = format!("{}/api/unstable/mcp-server/token", mcp_base(&self.site));

        let params = [
            ("grant_type", "refresh_token"),
            ("client_id", client_id),
            ("refresh_token", refresh_token),
        ];

        let resp = self
            .http
            .post(&url)
            .form(&params)
            .send()
            .await
            .context("MCP token refresh failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("MCP token refresh failed (HTTP {status}): {body}");
        }

        let token_resp: TokenResponse = resp
            .json()
            .await
            .context("failed to parse MCP token response")?;

        Ok(TokenSet {
            access_token: token_resp.access_token,
            refresh_token: token_resp.refresh_token,
            token_type: token_resp.token_type,
            expires_in: token_resp.expires_in,
            issued_at: Utc::now().timestamp(),
            scope: token_resp.scope,
            client_id: client_id.to_string(),
        })
    }
}

// --- Token storage using config dir files ---

fn mcp_token_path(site: &str) -> Option<std::path::PathBuf> {
    crate::config::config_dir().map(|d| d.join(format!("mcp_token_{site}.json")))
}

fn mcp_creds_path(site: &str) -> Option<std::path::PathBuf> {
    crate::config::config_dir().map(|d| d.join(format!("mcp_client_{site}.json")))
}

fn save_mcp_token(site: &str, tokens: &TokenSet) -> Result<()> {
    let path = mcp_token_path(site).ok_or_else(|| anyhow::anyhow!("no config dir"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(tokens)?;
    std::fs::write(&path, &json)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn load_mcp_token(site: &str) -> Option<TokenSet> {
    let path = mcp_token_path(site)?;
    let data = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&data).ok()
}

fn save_mcp_creds(site: &str, creds: &McpCredentials) -> Result<()> {
    let path = mcp_creds_path(site).ok_or_else(|| anyhow::anyhow!("no config dir"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(creds)?;
    std::fs::write(&path, json)?;
    Ok(())
}

fn load_mcp_creds(site: &str) -> Option<McpCredentials> {
    let path = mcp_creds_path(site)?;
    let data = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&data).ok()
}

/// Get a valid MCP access token, refreshing or re-authenticating if needed.
pub async fn get_mcp_token(cfg: &Config) -> Result<String> {
    let site = &cfg.site;

    // Try loading existing token
    if let Some(tokens) = load_mcp_token(site) {
        if !tokens.is_expired() {
            return Ok(tokens.access_token);
        }

        // Try refresh
        if !tokens.refresh_token.is_empty() {
            if let Some(creds) = load_mcp_creds(site) {
                let client = McpOAuthClient::new(site);
                match client
                    .refresh_token(&tokens.refresh_token, &creds.client_id)
                    .await
                {
                    Ok(new_tokens) => {
                        save_mcp_token(site, &new_tokens)?;
                        return Ok(new_tokens.access_token);
                    }
                    Err(e) => {
                        eprintln!("MCP token refresh failed, re-authenticating: {e}");
                    }
                }
            }
        }
    }

    // No valid token — run the full OAuth flow
    mcp_login(cfg).await
}

/// Run the MCP OAuth login flow (DCR + PKCE + browser).
async fn mcp_login(cfg: &Config) -> Result<String> {
    let site = &cfg.site;
    let client = McpOAuthClient::new(site);

    // 1. Start callback server
    let mut server = CallbackServer::new().await?;
    let redirect_uri = server.redirect_uri();

    eprintln!("\n🔐 MCP authentication required for site: {site}");
    eprintln!("📡 Callback server started on: {redirect_uri}");

    // 2. Register client (or reuse existing)
    let creds = match load_mcp_creds(site) {
        Some(creds) => {
            eprintln!("✓ Using existing MCP client registration");
            creds
        }
        None => {
            eprintln!("📝 Registering MCP OAuth2 client...");
            let creds = client.register(&redirect_uri).await?;
            save_mcp_creds(site, &creds)?;
            eprintln!("✓ Registered MCP client: {}", creds.client_id);
            creds
        }
    };

    // 3. PKCE challenge + state
    let challenge = pkce::generate_pkce_challenge()?;
    let state = pkce::generate_state()?;

    // 4. Build auth URL and open browser
    let auth_url =
        client.build_authorization_url(&creds.client_id, &redirect_uri, &state, &challenge);

    eprintln!("\n🌐 Opening browser for MCP authentication...");
    eprintln!("If the browser doesn't open, visit: {auth_url}");
    let _ = open::that(&auth_url);

    // 5. Wait for callback
    eprintln!("\n⏳ Waiting for MCP authorization...");
    let result = server
        .wait_for_callback(std::time::Duration::from_secs(300))
        .await?;

    if let Some(err) = &result.error {
        let desc = result.error_description.as_deref().unwrap_or("");
        bail!("MCP OAuth error: {err}: {desc}");
    }

    if result.state != state {
        bail!("MCP OAuth state mismatch");
    }

    // 6. Exchange code for tokens
    eprintln!("🔄 Exchanging code for MCP tokens...");
    let tokens = client
        .exchange_code(
            &result.code,
            &redirect_uri,
            &challenge.verifier,
            &creds.client_id,
        )
        .await?;

    save_mcp_token(site, &tokens)?;
    eprintln!("✅ MCP authentication successful!\n");

    Ok(tokens.access_token)
}
