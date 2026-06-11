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
    site: String,
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
        let url = format!("https://api.{}/api/v2/oauth2/register", self.site);

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
        let url = format!("https://api.{}/oauth2/v1/token", self.site);

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
    #[allow(clippy::too_many_arguments)]
    pub fn build_authorization_url(
        &self,
        client_id: &str,
        redirect_uri: &str,
        state: &str,
        challenge: &super::pkce::PkceChallenge,
        scopes: &[&str],
        subdomain: Option<&str>,
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

        // Use custom subdomain for SAML/SSO auth, otherwise use standard app.{site}.
        // The subdomain replaces the `app` prefix on whichever site is in play, so a
        // staging login (--site datad0g.com --subdomain dd) routes to dd.datad0g.com
        // rather than collapsing to dd.datadoghq.com (prod).
        // An empty subdomain string (`Some("")`) is coerced to `None` rather than
        // composing `https://.{site}/...` and producing a malformed URL the browser
        // would surface as an opaque DNS error.
        let auth_host = match subdomain.filter(|s| !s.is_empty()) {
            Some(sub) => format!("{sub}.{}", self.site),
            None => format!("app.{}", self.site),
        };
        format!("https://{auth_host}/oauth2/v1/authorize?{params}")
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
    fn build_authorization_url_defaults_to_app_subdomain() {
        let client = DcrClient::new("datadoghq.com");
        let url = client.build_authorization_url(
            "client123",
            "http://127.0.0.1:8000/oauth/callback",
            "state",
            &challenge(),
            &["dashboards_read"],
            None,
            None,
        );
        assert!(
            url.starts_with("https://app.datadoghq.com/oauth2/v1/authorize?"),
            "expected app.{{site}} prefix, got: {url}"
        );
    }

    #[test]
    fn build_authorization_url_uses_subdomain_on_provided_site() {
        // The bug: --subdomain used to hardcode .datadoghq.com regardless of --site,
        // so a staging login (datad0g.com) silently redirected to prod. The fix
        // composes the subdomain against the actual site.
        let client = DcrClient::new("datad0g.com");
        let url = client.build_authorization_url(
            "client123",
            "http://127.0.0.1:8000/oauth/callback",
            "state",
            &challenge(),
            &["dashboards_read"],
            Some("dd"),
            None,
        );
        assert!(
            url.starts_with("https://dd.datad0g.com/oauth2/v1/authorize?"),
            "expected dd.datad0g.com host, got: {url}"
        );
        assert!(
            !url.contains("datadoghq.com"),
            "staging login must not leak to prod host: {url}"
        );
    }

    #[test]
    fn build_authorization_url_uses_subdomain_on_eu_site() {
        let client = DcrClient::new("datadoghq.eu");
        let url = client.build_authorization_url(
            "client123",
            "http://127.0.0.1:8000/oauth/callback",
            "state",
            &challenge(),
            &["dashboards_read"],
            Some("acme"),
            None,
        );
        assert!(
            url.starts_with("https://acme.datadoghq.eu/oauth2/v1/authorize?"),
            "expected acme.datadoghq.eu host, got: {url}"
        );
    }

    #[test]
    fn build_authorization_url_treats_empty_subdomain_as_unset() {
        // An empty `--subdomain ""` previously composed to `https://.datadoghq.com/...`,
        // a malformed URL whose browser-side failure was an opaque DNS error.
        // The fix coerces empty to None so we fall back to app.{site} — same shape
        // as omitting the flag.
        let client = DcrClient::new("datadoghq.com");
        let url = client.build_authorization_url(
            "client123",
            "http://127.0.0.1:8000/oauth/callback",
            "state",
            &challenge(),
            &["dashboards_read"],
            Some(""),
            None,
        );
        assert!(
            url.starts_with("https://app.datadoghq.com/oauth2/v1/authorize?"),
            "empty subdomain should fall back to app.{{site}}, got: {url}"
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
            None,
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
            None,
        );
        assert!(!url.contains("dd_oid"), "got: {url}");
    }
}
