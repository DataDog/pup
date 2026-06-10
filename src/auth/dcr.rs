#[cfg(not(target_arch = "wasm32"))]
use anyhow::{bail, Context, Result};
#[cfg(not(target_arch = "wasm32"))]
use chrono::Utc;
#[cfg(not(target_arch = "wasm32"))]
use serde::{Deserialize, Serialize};

#[cfg(not(target_arch = "wasm32"))]
use super::types::{ClientCredentials, TokenSet};

#[cfg(not(target_arch = "wasm32"))]
pub const DCR_CLIENT_NAME: &str = "datadog-pup-cli";
// DCR_REDIRECT_PORTS is referenced from main.rs::resolve_callback_port, which
// runs on both native and wasm builds (the wasm login() stub bails before
// touching the port, but the symbol still has to resolve). Plain &[u16] has
// no platform requirements, so it's safe to expose unconditionally.
pub const DCR_REDIRECT_PORTS: &[u16] = &[8000, 8080, 8888, 9000];

#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
pub fn get_redirect_uris() -> Vec<String> {
    DCR_REDIRECT_PORTS
        .iter()
        .map(|port| format!("http://127.0.0.1:{port}/oauth/callback"))
        .collect()
}

#[cfg(not(target_arch = "wasm32"))]
/// DCR + token exchange client.
pub struct DcrClient {
    /// Normalized site used as the token-storage key.
    site: String,
    /// Host for API calls (register, token). Canonical sites → `api.{site}`;
    /// literal hosts (vanity/gateway) → verbatim.
    api_host: String,
    /// Host for the OAuth authorize redirect. Canonical sites → `app.{site}`;
    /// literal hosts → verbatim.
    auth_host: String,
    http: reqwest::Client,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Serialize)]
struct RegistrationRequest {
    client_name: String,
    redirect_uris: Vec<String>,
    grant_types: Vec<String>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Deserialize)]
struct RegistrationResponse {
    client_id: String,
    client_name: String,
    redirect_uris: Vec<String>,
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
impl DcrClient {
    pub fn new(site: &str) -> Self {
        Self {
            api_host: crate::config::api_host_for(site),
            auth_host: crate::config::auth_host_for(site),
            site: site.to_string(),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("failed to build HTTP client"),
        }
    }

    /// Dynamic Client Registration (RFC 7591).
    pub async fn register(
        &self,
        redirect_uri: &str,
        _scopes: &[&str],
    ) -> Result<ClientCredentials> {
        let url = format!("https://{}/api/v2/oauth2/register", self.api_host);

        let body = RegistrationRequest {
            client_name: DCR_CLIENT_NAME.to_string(),
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
            .context("DCR registration request failed")?;

        if resp.status() != reqwest::StatusCode::CREATED {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("DCR registration failed (HTTP {status}): {body}");
        }

        let reg: RegistrationResponse =
            resp.json().await.context("failed to parse DCR response")?;

        Ok(ClientCredentials {
            client_id: reg.client_id,
            client_name: reg.client_name,
            redirect_uris: reg.redirect_uris,
            registered_at: Utc::now().timestamp(),
            site: self.site.clone(),
        })
    }

    /// Exchange authorization code for tokens.
    pub async fn exchange_code(
        &self,
        code: &str,
        redirect_uri: &str,
        code_verifier: &str,
        creds: &ClientCredentials,
    ) -> Result<TokenSet> {
        let params = [
            ("grant_type", "authorization_code"),
            ("client_id", &creds.client_id),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("code_verifier", code_verifier),
        ];
        self.request_tokens(&params, &creds.client_id).await
    }

    /// Refresh an access token.
    pub async fn refresh_token(
        &self,
        refresh_token: &str,
        creds: &ClientCredentials,
    ) -> Result<TokenSet> {
        let params = [
            ("grant_type", "refresh_token"),
            ("client_id", &creds.client_id),
            ("refresh_token", refresh_token),
            ("redirect_uri", ""),  // not needed for refresh
            ("code_verifier", ""), // not needed for refresh
        ];
        self.request_tokens(&params, &creds.client_id).await
    }

    async fn request_tokens(&self, params: &[(&str, &str)], client_id: &str) -> Result<TokenSet> {
        let url = format!("https://{}/oauth2/v1/token", self.api_host);

        // Filter out empty params
        let form_params: Vec<(&str, &str)> = params
            .iter()
            .filter(|(_, v)| !v.is_empty())
            .copied()
            .collect();

        let resp = self
            .http
            .post(&url)
            .form(&form_params)
            .send()
            .await
            .context("token request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("token exchange failed (HTTP {status}): {body}");
        }

        let token_resp: TokenResponse = resp
            .json()
            .await
            .context("failed to parse token response")?;

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

    /// Build the authorization URL for the browser. `org_uuid` is appended as
    /// `dd_oid` when set; callers should coerce empty strings to `None`
    /// upstream so this function doesn't have to second-guess them.
    ///
    /// The OAuth host is derived from `auth_host`: canonical sites use `app.{site}`;
    /// literal hosts (vanity domain or gateway) are used verbatim. Pass the full
    /// desired host via `--site` / `DD_SITE` — `--subdomain` has been removed.
    pub fn build_authorization_url(
        &self,
        client_id: &str,
        redirect_uri: &str,
        state: &str,
        challenge: &super::pkce::PkceChallenge,
        scopes: &[&str],
        org_uuid: Option<&str>,
    ) -> String {
        // Sort scopes so the printed authorize URL has a deterministic
        // `scope=` parameter order — easier to diff and grep across runs.
        // OAuth treats `scope` as an unordered set, so this is a no-op for
        // the issuer.
        let mut sorted_scopes: Vec<&str> = scopes.to_vec();
        sorted_scopes.sort();
        let scope = sorted_scopes.join(" ");
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        serializer
            .append_pair("response_type", "code")
            .append_pair("client_id", client_id)
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("state", state)
            .append_pair("scope", &scope)
            .append_pair("code_challenge", &challenge.challenge)
            .append_pair("code_challenge_method", &challenge.method);
        if let Some(uuid) = org_uuid {
            serializer.append_pair("dd_oid", uuid);
        }
        let params = serializer.finish();

        format!("https://{}/oauth2/v1/authorize?{params}", self.auth_host)
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::auth::pkce::PkceChallenge;

    fn challenge() -> PkceChallenge {
        PkceChallenge {
            verifier: "v".into(),
            challenge: "c".into(),
            method: "S256".into(),
        }
    }

    #[test]
    fn build_authorization_url_uses_app_for_canonical_site() {
        // Canonical sites → app.{site} as OAuth host.
        let client = DcrClient::new("datadoghq.com");
        let url = client.build_authorization_url(
            "client123",
            "http://127.0.0.1:8000/oauth/callback",
            "state",
            &challenge(),
            &["dashboards_read"],
            None,
        );
        assert!(
            url.starts_with("https://app.datadoghq.com/oauth2/v1/authorize?"),
            "expected app.datadoghq.com host, got: {url}"
        );
    }

    #[test]
    fn build_authorization_url_uses_app_for_eu_canonical_site() {
        let client = DcrClient::new("datadoghq.eu");
        let url = client.build_authorization_url(
            "client123",
            "http://127.0.0.1:8000/oauth/callback",
            "state",
            &challenge(),
            &["dashboards_read"],
            None,
        );
        assert!(
            url.starts_with("https://app.datadoghq.eu/oauth2/v1/authorize?"),
            "expected app.datadoghq.eu host, got: {url}"
        );
    }

    #[test]
    fn build_authorization_url_uses_app_for_staging_site() {
        // datad0g.com is canonical (staging); pass --site dd.datad0g.com for
        // a vanity-style staging login instead of using the removed --subdomain.
        let client = DcrClient::new("datad0g.com");
        let url = client.build_authorization_url(
            "client123",
            "http://127.0.0.1:8000/oauth/callback",
            "state",
            &challenge(),
            &["dashboards_read"],
            None,
        );
        assert!(
            url.starts_with("https://app.datad0g.com/oauth2/v1/authorize?"),
            "expected app.datad0g.com host, got: {url}"
        );
        assert!(
            !url.contains("datadoghq.com"),
            "staging login must not leak to prod host: {url}"
        );
    }

    #[test]
    fn build_authorization_url_uses_literal_host_verbatim() {
        // Vanity/SAML host passed directly as --site: used verbatim (replaces --subdomain).
        let client = DcrClient::new("mycompany.datadoghq.com");
        let url = client.build_authorization_url(
            "client123",
            "http://127.0.0.1:8000/oauth/callback",
            "state",
            &challenge(),
            &["dashboards_read"],
            None,
        );
        assert!(
            url.starts_with("https://mycompany.datadoghq.com/oauth2/v1/authorize?"),
            "expected literal host, got: {url}"
        );
    }

    #[test]
    fn build_authorization_url_uses_gateway_host_verbatim() {
        let client = DcrClient::new("mygateway.example.com");
        let url = client.build_authorization_url(
            "client123",
            "http://127.0.0.1:8000/oauth/callback",
            "state",
            &challenge(),
            &["dashboards_read"],
            None,
        );
        assert!(
            url.starts_with("https://mygateway.example.com/oauth2/v1/authorize?"),
            "expected gateway host, got: {url}"
        );
    }

    #[test]
    fn build_authorization_url_includes_required_oauth_params() {
        let client = DcrClient::new("datadoghq.com");
        let url = client.build_authorization_url(
            "client123",
            "http://127.0.0.1:8000/oauth/callback",
            "the-state",
            &challenge(),
            &["dashboards_read", "metrics_read"],
            None,
        );
        assert!(url.contains("response_type=code"));
        assert!(url.contains("client_id=client123"));
        assert!(url.contains("state=the-state"));
        assert!(url.contains("code_challenge=c"));
        assert!(url.contains("code_challenge_method=S256"));
        // Scopes are joined with a space, then URL-encoded as `+` or `%20`.
        assert!(url.contains("scope=dashboards_read") && url.contains("metrics_read"));
    }

    #[test]
    fn build_authorization_url_appends_dd_oid_when_org_uuid_set() {
        let client = DcrClient::new("datadoghq.com");
        let url = client.build_authorization_url(
            "client123",
            "http://127.0.0.1:8000/oauth/callback",
            "state",
            &challenge(),
            &["dashboards_read"],
            Some("00000000-1111-2222-3333-444444444444"),
        );
        assert!(
            url.contains("dd_oid=00000000-1111-2222-3333-444444444444"),
            "expected dd_oid query param, got: {url}"
        );
    }

    #[test]
    fn build_authorization_url_omits_dd_oid_when_unset() {
        let client = DcrClient::new("datadoghq.com");
        let url = client.build_authorization_url(
            "client123",
            "http://127.0.0.1:8000/oauth/callback",
            "state",
            &challenge(),
            &["dashboards_read"],
            None,
        );
        assert!(!url.contains("dd_oid"), "got: {url}");
    }

    /// Verify that DCR registration and token exchange target the literal host
    /// verbatim for non-canonical sites (vanity domains, custom gateways).
    /// These URLs are constructed at call time so there is no HTTP call here;
    /// the test inspects `api_host` indirectly by verifying the fields set at
    /// construction, which `register`/`request_tokens` use directly.
    #[test]
    fn dcr_client_api_host_for_literal_site() {
        // Canonical site: register/token should hit api.datadoghq.com.
        let canonical = DcrClient::new("datadoghq.com");
        assert_eq!(canonical.api_host, "api.datadoghq.com");
        assert_eq!(canonical.auth_host, "app.datadoghq.com");

        // Vanity/SAML site (literal): must use the host verbatim without prepending api.
        let vanity = DcrClient::new("mycompany.datadoghq.com");
        assert_eq!(vanity.api_host, "mycompany.datadoghq.com");
        assert_eq!(vanity.auth_host, "mycompany.datadoghq.com");

        // Custom gateway (literal): same — no api. prefix.
        let gateway = DcrClient::new("mygateway.example.com");
        assert_eq!(gateway.api_host, "mygateway.example.com");
        assert_eq!(gateway.auth_host, "mygateway.example.com");

        // Staging canonical: api./app. prefixes apply.
        let staging = DcrClient::new("datad0g.com");
        assert_eq!(staging.api_host, "api.datad0g.com");
        assert_eq!(staging.auth_host, "app.datad0g.com");
    }
}
