use anyhow::{bail, Result};

use crate::auth::storage;
use crate::config::Config;

/// Helper to run a closure with the storage lock held (non-async to avoid holding lock across await).
fn with_storage<F, R>(f: F) -> Result<R>
where
    F: FnOnce(&mut dyn storage::Storage) -> Result<R>,
{
    let guard = storage::get_storage()?;
    let mut lock = guard.lock().unwrap();
    let store = lock.as_mut().unwrap();
    f(&mut **store)
}

/// Format the trailing " (org: <name>)" segment used in user-facing log lines.
/// Returns an empty string when there's no org so the surrounding messages stay
/// clean for the default-org case.
fn org_suffix(org: Option<&str>) -> String {
    org.map(|o| format!(" (org: {o})")).unwrap_or_default()
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn login(
    cfg: &Config,
    scopes: Vec<String>,
    subdomain: Option<&str>,
    callback_port: Option<u16>,
    org_uuid: Option<&str>,
) -> Result<()> {
    use crate::auth::{dcr, pkce};

    let site = &cfg.site;
    let org = cfg.org.as_deref();

    // Resolve effective org_uuid: CLI flag wins; otherwise recall from any
    // saved session so re-auth keeps emitting `dd_oid` without re-passing the
    // flag.
    let stored_session = storage::find_session(site, org);
    let effective_org_uuid: Option<String> = org_uuid
        .map(String::from)
        .or_else(|| stored_session.as_ref().and_then(|s| s.org_uuid.clone()));

    // 1. Start callback server. `callback_port` (from --callback-port or
    //    PUP_OAUTH_CALLBACK_PORT) pins the port deterministically for SSH
    //    port-forwarded workflows; otherwise we scan the DCR allowlist.
    let mut server = crate::auth::callback::CallbackServer::new(callback_port).await?;
    let redirect_uri = server.redirect_uri();
    let org_label = org_suffix(org);
    eprintln!("\n🔐 Starting OAuth2 login for site: {site}{org_label}\n");
    if let Some(sub) = subdomain {
        // Compose against the actual site, not a hardcoded prod host. Mirrors
        // the URL-construction fix in dcr::build_authorization_url so the log
        // line and the URL the browser opens stay in agreement on staging.
        eprintln!("🏢 Using SAML/SSO subdomain: {sub}.{site}");
    }
    eprintln!("📡 Callback server started on: {redirect_uri}");

    let scope_strs: Vec<&str> = scopes.iter().map(String::as_str).collect();
    if scopes.len() > 10 {
        eprintln!(
            "🔑 Requesting {} scope(s) (use --scopes to customize)",
            scopes.len()
        );
    } else {
        eprintln!(
            "🔑 Requesting {} scope(s): {}",
            scopes.len(),
            scopes.join(", ")
        );
    }

    // 2. Load existing client credentials (lock released before any await)
    // Client credentials are site-scoped (DCR is per-site, shared across orgs)
    let existing_creds = with_storage(|store| store.load_client_credentials(site))?;

    let creds = match existing_creds {
        Some(creds) if creds.client_name == dcr::DCR_CLIENT_NAME => {
            eprintln!("✓ Using existing client registration");
            creds
        }
        other => {
            if other.is_some() {
                eprintln!("📝 Re-registering OAuth2 client (name changed)...");
                with_storage(|store| store.delete_client_credentials(site))?;
            } else {
                eprintln!("📝 Registering new OAuth2 client...");
            }
            let dcr_client = dcr::DcrClient::new(site);
            let creds = dcr_client.register(&redirect_uri, &scope_strs).await?;
            with_storage(|store| store.save_client_credentials(site, &creds))?;
            eprintln!("✓ Registered client: {}", creds.client_id);
            creds
        }
    };

    // 3. Generate PKCE challenge + state
    let challenge = pkce::generate_pkce_challenge()?;
    let state = pkce::generate_state()?;

    // 4. Build authorization URL
    let dcr_client = dcr::DcrClient::new(site);
    let auth_url = dcr_client.build_authorization_url(
        &creds.client_id,
        &redirect_uri,
        &state,
        &challenge,
        &scope_strs,
        subdomain,
        effective_org_uuid.as_deref(),
    );
    if let Some(uuid) = effective_org_uuid.as_deref() {
        eprintln!("🎯 Hinting org UUID (dd_oid): {uuid}");
    }

    // 5. Open browser
    eprintln!("\n🌐 Opening browser for authentication...");
    eprintln!("If the browser doesn't open, visit: {auth_url}");
    let browser_opened = open::that(&auth_url).is_ok();
    if !browser_opened {
        eprintln!(
            "\nNo local browser detected (remote/SSH session?). To complete login:\n  \
             1. Open the URL above on a machine with a browser and authorize.\n  \
             2. Your browser will redirect to {redirect_uri}?... and fail to load\n     \
             (expected). Copy that full URL from the address bar.\n  \
             3. Paste it below, then press Enter.\n     \
             Example: {redirect_uri}?code=...&state=..."
        );
    }

    // 6. Wait for callback. The happy path waits on the HTTP listener only,
    // exactly as before this change. When the browser failed to open, also
    // race a stdin paste path so users on remote machines can manually relay
    // the redirect URL. The stdin path is only enabled in the headless branch
    // so legitimate non-interactive launches (closed stdin, piped /dev/null)
    // can't short-circuit a working browser flow.
    eprintln!("\n⏳ Waiting for authorization...");
    let result = if browser_opened {
        server
            .wait_for_callback(std::time::Duration::from_secs(300))
            .await?
    } else {
        use std::io::Write;
        eprint!("> ");
        let _ = std::io::stderr().flush();
        tokio::select! {
            r = server.wait_for_callback(std::time::Duration::from_secs(300)) => r?,
            r = crate::auth::callback::read_callback_url_from_stdin() => r?,
        }
    };

    if let Some(err) = &result.error {
        let desc = result.error_description.as_deref().unwrap_or("");
        bail!("OAuth error: {err}: {desc}");
    }

    if result.state != state {
        bail!("OAuth state mismatch (possible CSRF attack)");
    }

    // 7. Exchange code for tokens
    eprintln!("🔄 Exchanging authorization code for tokens...");
    let tokens = dcr_client
        .exchange_code(&result.code, &redirect_uri, &challenge.verifier, &creds)
        .await?;

    // 8. Resolve the save target — relabel under the actual returned org on
    // dd_oid mismatch. The OAuth code is single-use, so refusing to save
    // would throw away the user's click.
    //
    // Filter callback `dd_oid` through a shape check so a tampered URL
    // pasted into the stdin fallback path can't seed an arbitrary string
    // into the session registry.
    let validated_dd_oid = result.dd_oid.as_deref().filter(|u| looks_like_uuid(u));
    let saved_org_owned: Option<String> = match resolve_save_target(
        org,
        effective_org_uuid.as_deref(),
        validated_dd_oid,
        result.dd_org_name.as_deref(),
    ) {
        SaveTarget::Requested(label) => label,
        SaveTarget::Reassigned {
            label,
            requested_uuid,
            actual_uuid,
            actual_name,
        } => {
            let actual_name_suffix = actual_name
                .as_deref()
                .map(|n| format!(" ({n})"))
                .unwrap_or_default();
            let cleanup_hint = match org {
                Some(o) => format!(
                    "\n   Your existing '{o}' session is unchanged. \
                     Run `pup auth logout --org {o}` to clear it if it's stale."
                ),
                None => String::new(),
            };
            eprintln!(
                "⚠️  Requested org UUID {requested_uuid} but OAuth returned \
                 {actual_uuid}{actual_name_suffix}. Saving token under \
                 \"{label}\" instead of the requested label.{cleanup_hint}"
            );
            Some(label)
        }
    };
    let saved_org = saved_org_owned.as_deref();
    let saved_org_label = org_suffix(saved_org);

    let location = with_storage(|store| {
        store.save_tokens(site, saved_org, &tokens)?;
        Ok(store.storage_location())
    })?;

    // Prefer the callback's `dd_oid` over the CLI/stored hint — it reflects
    // the org the user actually consented for. Skip values that don't look
    // like a UUID so we don't persist garbage from a tampered callback URL.
    let saved_org_uuid = validated_dd_oid
        .or(effective_org_uuid.as_deref())
        .map(String::from);
    storage::save_session(&storage::SessionEntry {
        site: site.clone(),
        org: saved_org.map(String::from),
        org_uuid: saved_org_uuid,
    })?;

    let expires_at = chrono::DateTime::from_timestamp(tokens.issued_at + tokens.expires_in, 0)
        .map(|dt| dt.with_timezone(&chrono::Local).to_rfc3339())
        .unwrap_or_else(|| format!("in {} hours", tokens.expires_in / 3600));

    eprintln!("\n✅ Login successful{saved_org_label}!");
    eprintln!("   Access token expires: {expires_at}");
    eprintln!("   Token stored in: {location}");

    Ok(())
}

/// Loose UUID shape check (8-4-4-4-12 hex with dashes, ASCII only). The
/// callback's `dd_oid` is unsigned metadata — in the stdin-paste fallback
/// path a tampered URL could carry an arbitrary string. Rejecting anything
/// that doesn't look like a UUID keeps such values from being persisted into
/// `sessions.json` or rendered as a future hint.
#[cfg(not(target_arch = "wasm32"))]
fn looks_like_uuid(s: &str) -> bool {
    s.len() == 36
        && s.chars().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit(),
        })
}

/// What `resolve_save_target` decided about where to save the new token.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug)]
enum SaveTarget {
    /// dd_oid hint matched, no hint was sent, or there's no `--org` to
    /// relabel away from — save under the requested label (or the default
    /// session when `--org` was unset).
    Requested(Option<String>),
    /// dd_oid mismatch on a named-org login — save under `label` and warn
    /// the user. Carries the requested vs actual UUIDs so the caller can
    /// render a useful warning.
    Reassigned {
        label: String,
        requested_uuid: String,
        actual_uuid: String,
        actual_name: Option<String>,
    },
}

/// Pure: pick the `(site, org)` save target for a finished login. Mismatch
/// detection compares hinted vs callback UUIDs case-insensitively (UUIDs are
/// hex). Relabel only triggers for named-org logins — when `--org` is unset
/// the default session is the source of truth and gets overwritten with the
/// new UUID directly, so plain `pup auth login` stays idempotent.
///
/// On relabel, the new label always embeds an 8-char UUID prefix so the
/// stored entry is unambiguous even if the issuer's display name collides
/// with another existing session on the same site.
#[cfg(not(target_arch = "wasm32"))]
fn resolve_save_target(
    requested_org: Option<&str>,
    requested_uuid: Option<&str>,
    actual_uuid: Option<&str>,
    actual_org_name: Option<&str>,
) -> SaveTarget {
    match (requested_org, requested_uuid, actual_uuid) {
        (Some(_), Some(req), Some(actual)) if !req.eq_ignore_ascii_case(actual) => {
            let prefix: String = actual.chars().take(8).collect();
            let label = match actual_org_name {
                Some(name) if !name.is_empty() => format!("{name} [{prefix}]"),
                _ => prefix,
            };
            SaveTarget::Reassigned {
                label,
                requested_uuid: req.to_string(),
                actual_uuid: actual.to_string(),
                actual_name: actual_org_name.filter(|n| !n.is_empty()).map(String::from),
            }
        }
        _ => SaveTarget::Requested(requested_org.map(String::from)),
    }
}

#[cfg(target_arch = "wasm32")]
pub async fn login(
    _cfg: &Config,
    _scopes: Vec<String>,
    _subdomain: Option<&str>,
    _callback_port: Option<u16>,
    _org_uuid: Option<&str>,
) -> Result<()> {
    bail!(
        "OAuth login is not available in WASM builds.\n\
         Use DD_ACCESS_TOKEN env var for bearer token auth,\n\
         or DD_API_KEY + DD_APP_KEY for API key auth."
    )
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn logout(cfg: &Config) -> Result<()> {
    let site = &cfg.site;
    let org = cfg.org.as_deref();
    with_storage(|store| {
        store.delete_tokens(site, org)?;
        // Only delete client credentials when logging out the default (no-org) session;
        // client credentials are site-scoped and shared across orgs
        if org.is_none() {
            store.delete_client_credentials(site)?;
        }
        Ok(())
    })?;
    storage::remove_session(site, org)?;
    let org_label = org_suffix(org);
    eprintln!("Logged out from {site}{org_label}. Tokens removed.");
    Ok(())
}

#[cfg(target_arch = "wasm32")]
pub async fn logout(_cfg: &Config) -> Result<()> {
    bail!(
        "OAuth logout is not available in WASM builds.\n\
         Token storage is not available — credentials are read from environment variables."
    )
}

pub fn status(cfg: &Config) -> Result<()> {
    let site = &cfg.site;
    let org = cfg.org.as_deref();

    // In WASM, just report env var status
    with_storage(|store| {
        match store.load_tokens(site, org)? {
            Some(tokens) => {
                let expires_at_ts = tokens.issued_at + tokens.expires_in;
                let now = chrono::Utc::now().timestamp();
                let remaining_secs = expires_at_ts - now;

                let (status, remaining_str) = if tokens.is_expired() {
                    ("expired".to_string(), "expired".to_string())
                } else {
                    let mins = remaining_secs / 60;
                    let secs = remaining_secs % 60;
                    ("valid".to_string(), format!("{mins}m{secs}s"))
                };

                let org_label = org_suffix(org);
                if tokens.is_expired() {
                    eprintln!("⚠️  Token expired for site: {site}{org_label}");
                } else {
                    eprintln!("✅ Authenticated for site: {site}{org_label}");
                    eprintln!("   Token expires in: {remaining_str}");
                }

                let expires_at = chrono::DateTime::from_timestamp(expires_at_ts, 0)
                    .map(|dt| dt.with_timezone(&chrono::Local).to_rfc3339())
                    .unwrap_or_default();

                let scopes: Vec<&str> = tokens
                    .scope
                    .split_whitespace()
                    .filter(|s| !s.is_empty())
                    .collect();

                let json = serde_json::json!({
                    "authenticated": true,
                    "expires_at": expires_at,
                    "has_refresh": !tokens.refresh_token.is_empty(),
                    "org": org,
                    "scopes": scopes,
                    "site": site,
                    "status": status,
                    "token_type": tokens.token_type,
                });
                println!("{}", serde_json::to_string_pretty(&json).unwrap());
            }
            None => {
                let org_label = org_suffix(org);
                eprintln!("❌ Not authenticated for site: {site}{org_label}");
                let json = serde_json::json!({
                    "authenticated": false,
                    "org": org,
                    "site": site,
                    "status": "no token",
                });
                println!("{}", serde_json::to_string_pretty(&json).unwrap());
            }
        }
        Ok(())
    })
}

#[cfg(debug_assertions)]
pub fn token(cfg: &Config) -> Result<()> {
    if let Some(token) = &cfg.access_token {
        println!("{token}");
        return Ok(());
    }

    let site = &cfg.site;
    let org = cfg.org.as_deref();
    with_storage(|store| match store.load_tokens(site, org)? {
        Some(tokens) => {
            if tokens.is_expired() {
                bail!("token is expired — run 'pup auth login' to refresh");
            }
            println!("{}", tokens.access_token);
            Ok(())
        }
        None => bail!("no token available — run 'pup auth login' or set DD_ACCESS_TOKEN"),
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn refresh(cfg: &Config) -> Result<()> {
    use crate::auth::dcr;

    let site = &cfg.site;
    let org = cfg.org.as_deref();

    let tokens = with_storage(|store| store.load_tokens(site, org))?.ok_or_else(|| {
        anyhow::anyhow!("no tokens found for site {site} — run 'pup auth login' first")
    })?;

    if tokens.refresh_token.is_empty() {
        bail!("no refresh token available — run 'pup auth login' to re-authenticate");
    }

    let creds = with_storage(|store| store.load_client_credentials(site))?.ok_or_else(|| {
        anyhow::anyhow!("no client credentials found for site {site} — run 'pup auth login' first")
    })?;

    let org_label = org_suffix(org);
    eprintln!("🔄 Refreshing access token for site: {site}{org_label}...");

    let dcr_client = dcr::DcrClient::new(site);
    let new_tokens = dcr_client
        .refresh_token(&tokens.refresh_token, &creds)
        .await?;

    let location = with_storage(|store| {
        store.save_tokens(site, org, &new_tokens)?;
        Ok(store.storage_location())
    })?;

    let expires_at =
        chrono::DateTime::from_timestamp(new_tokens.issued_at + new_tokens.expires_in, 0)
            .map(|dt| dt.with_timezone(&chrono::Local).to_rfc3339())
            .unwrap_or_else(|| format!("in {} hours", new_tokens.expires_in / 3600));

    eprintln!("✅ Token refreshed successfully!");
    eprintln!("   Access token expires: {expires_at}");
    eprintln!("   Token stored in: {location}");

    Ok(())
}

#[cfg(target_arch = "wasm32")]
pub async fn refresh(_cfg: &Config) -> Result<()> {
    bail!("OAuth token refresh is not available in WASM builds.")
}

/// List all stored org sessions from the session registry, enriched with token status.
#[cfg(not(target_arch = "wasm32"))]
pub fn list(cfg: &Config) -> Result<()> {
    let sessions = storage::list_sessions()?;

    let enriched: Vec<serde_json::Value> = sessions
        .into_iter()
        .map(|s| {
            let tokens = with_storage(|store| store.load_tokens(&s.site, s.org.as_deref()))
                .ok()
                .flatten();

            match tokens {
                Some(t) => {
                    let expires_at_ts = t.issued_at + t.expires_in;
                    let is_expired = t.is_expired();
                    let status = if is_expired { "expired" } else { "valid" };
                    let expires_at = chrono::DateTime::from_timestamp(expires_at_ts, 0)
                        .map(|dt| dt.with_timezone(&chrono::Local).to_rfc3339())
                        .unwrap_or_default();
                    let scopes: Vec<&str> = t
                        .scope
                        .split_whitespace()
                        .filter(|s| !s.is_empty())
                        .collect();
                    serde_json::json!({
                        "expires_at": expires_at,
                        "has_refresh": !t.refresh_token.is_empty(),
                        "org": s.org,
                        "scopes": scopes,
                        "site": s.site,
                        "status": status,
                    })
                }
                None => serde_json::json!({
                    "expires_at": null,
                    "has_refresh": false,
                    "org": s.org,
                    "scopes": [],
                    "site": s.site,
                    "status": "no token",
                }),
            }
        })
        .collect();

    crate::formatter::output(cfg, &enriched)
}

#[cfg(target_arch = "wasm32")]
pub fn list(_cfg: &Config) -> Result<()> {
    bail!("Session listing is not available in WASM builds.")
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::auth::storage::SessionEntry;
    use crate::config::{Config, OutputFormat};

    /// Temporary directory that removes itself on drop. Mirrors the helper
    /// used in src/auth/storage.rs tests so we don't depend on an external
    /// tempdir crate.
    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0);
            let dir = std::env::temp_dir().join(format!("pup_auth_cmd_test_{label}_{nanos}"));
            std::fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }

        fn path(&self) -> &std::path::PathBuf {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn base_config() -> Config {
        Config {
            api_key: None,
            app_key: None,
            access_token: None,
            site: "datadoghq.com".into(),
            site_explicit: false,
            org: None,
            output_format: OutputFormat::Json,
            auto_approve: false,
            agent_mode: false,
            read_only: false,
        }
    }

    // ------------------------------------------------------------------
    // token() — the one function with an access_token bypass that never
    // touches the global STORAGE singleton. Hermetic.
    // ------------------------------------------------------------------

    #[test]
    fn test_token_prints_access_token_from_config() {
        let mut cfg = base_config();
        cfg.access_token = Some("oauth-access-token-from-cfg".into());
        // Positive path: cfg.access_token is Some → returns Ok without
        // touching storage. We only assert the Result; capturing stdout
        // would require redirecting std::io::stdout and is not necessary
        // to verify the bypass branch runs.
        assert!(token(&cfg).is_ok());
    }

    #[test]
    fn test_token_empty_string_still_bypasses_storage() {
        // An empty-string access_token is still Some(_) — the guard uses
        // `if let Some(token)` and does not check for empty. Pin this
        // behaviour so future refactors are intentional.
        let mut cfg = base_config();
        cfg.access_token = Some(String::new());
        assert!(token(&cfg).is_ok());
    }

    // ------------------------------------------------------------------
    // list() — empty session registry path is hermetic: when there are
    // no sessions on disk, the .map() closure that calls with_storage
    // never runs, so the global STORAGE singleton is not touched.
    //
    // All tests below use the tokio-based lock from test_support so that
    // both sync and async tests in this module serialize against a single
    // mutex and don't race each other for PUP_CONFIG_DIR / DD_TOKEN_STORAGE.
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_list_empty_session_registry_returns_ok() {
        let _lock = crate::test_support::lock_env().await;
        let tmp = TempDir::new("list_empty");
        std::env::set_var("PUP_CONFIG_DIR", tmp.path());

        let cfg = base_config();
        let result = list(&cfg);

        std::env::remove_var("PUP_CONFIG_DIR");
        assert!(
            result.is_ok(),
            "list with empty session registry should be Ok"
        );
    }

    #[tokio::test]
    async fn test_list_with_saved_sessions_returns_ok() {
        // After save_session, the sessions.json file is present but we
        // haven't stored any tokens. The storage backend is exercised but
        // returns None → list() enriches each session with "no token"
        // status and returns Ok. This covers the populated branch of
        // list() without requiring real credentials.
        let _lock = crate::test_support::lock_env().await;
        let tmp = TempDir::new("list_populated");
        std::env::set_var("PUP_CONFIG_DIR", tmp.path());
        std::env::set_var("DD_TOKEN_STORAGE", "file");

        storage::save_session(&SessionEntry {
            site: "datadoghq.com".into(),
            org: None,
            org_uuid: None,
        })
        .unwrap();
        storage::save_session(&SessionEntry {
            site: "datadoghq.com".into(),
            org: Some("prod-child".into()),
            org_uuid: None,
        })
        .unwrap();

        let cfg = base_config();
        let result = list(&cfg);

        std::env::remove_var("DD_TOKEN_STORAGE");
        std::env::remove_var("PUP_CONFIG_DIR");
        assert!(
            result.is_ok(),
            "list with saved sessions should be Ok even when no tokens stored"
        );
    }

    // ------------------------------------------------------------------
    // status() — reads tokens via the global STORAGE singleton. We can
    // assert the return is Ok on the unauthenticated branch (no tokens
    // in whatever dir STORAGE was bound to), and cannot cleanly test
    // the authenticated branch from here without writing to STORAGE's
    // captured base_dir (which the singleton freezes at first init and
    // we cannot observe from outside auth/storage.rs).
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_status_returns_ok_for_unauthenticated() {
        let _lock = crate::test_support::lock_env().await;
        let tmp = TempDir::new("status_unauth");
        std::env::set_var("PUP_CONFIG_DIR", tmp.path());
        std::env::set_var("DD_TOKEN_STORAGE", "file");

        let mut cfg = base_config();
        cfg.site = "unauth-site.example.invalid".into();
        let result = status(&cfg);

        std::env::remove_var("DD_TOKEN_STORAGE");
        std::env::remove_var("PUP_CONFIG_DIR");
        // status() always returns Ok; it reports authentication state
        // via printed JSON, not via the Result.
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_status_returns_ok_with_org_label() {
        // Same contract as above but with an org set. Covers the
        // org_label = " (org: ...)" branch in the unauthenticated arm.
        let _lock = crate::test_support::lock_env().await;
        let tmp = TempDir::new("status_org");
        std::env::set_var("PUP_CONFIG_DIR", tmp.path());
        std::env::set_var("DD_TOKEN_STORAGE", "file");

        let mut cfg = base_config();
        cfg.site = "unauth-org-site.example.invalid".into();
        cfg.org = Some("test-org-label".into());
        let result = status(&cfg);

        std::env::remove_var("DD_TOKEN_STORAGE");
        std::env::remove_var("PUP_CONFIG_DIR");
        assert!(result.is_ok());
    }

    // ------------------------------------------------------------------
    // logout() — removes tokens / credentials / session entry. All three
    // are idempotent: deleting non-existent items returns Ok. That gives
    // us a clean negative-space test.
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_logout_is_idempotent_when_not_logged_in() {
        let _lock = crate::test_support::lock_env().await;
        let tmp = TempDir::new("logout_idempotent");
        std::env::set_var("PUP_CONFIG_DIR", tmp.path());
        std::env::set_var("DD_TOKEN_STORAGE", "file");

        let mut cfg = base_config();
        cfg.site = "logout-clean.example.invalid".into();
        let result = logout(&cfg).await;

        std::env::remove_var("DD_TOKEN_STORAGE");
        std::env::remove_var("PUP_CONFIG_DIR");
        assert!(
            result.is_ok(),
            "logout on an un-logged-in site should be a no-op"
        );
    }

    // ------------------------------------------------------------------
    // resolve_save_target — pure function, no env / storage state.
    // ------------------------------------------------------------------

    #[test]
    fn resolve_save_target_no_hint_keeps_requested_org() {
        let t = resolve_save_target(Some("prod-child"), None, None, None);
        assert!(matches!(t, SaveTarget::Requested(Some(s)) if s == "prod-child"));
    }

    #[test]
    fn resolve_save_target_hint_matches_keeps_requested_org() {
        let uuid = "00000000-1111-2222-3333-444444444444";
        let t = resolve_save_target(
            Some("prod-child"),
            Some(uuid),
            Some(uuid),
            Some("Mos Eisley Cantina"),
        );
        assert!(matches!(t, SaveTarget::Requested(Some(s)) if s == "prod-child"));
    }

    #[test]
    fn resolve_save_target_hint_matches_case_insensitive() {
        // UUIDs are hex; normalise case so a mismatch in the issuer's
        // canonicalisation doesn't trip the warn-and-relabel path.
        let upper = "AABBCCDD-1111-2222-3333-EEFFAABBCCDD";
        let lower = "aabbccdd-1111-2222-3333-eeffaabbccdd";
        let t = resolve_save_target(Some("prod-child"), Some(upper), Some(lower), None);
        assert!(matches!(t, SaveTarget::Requested(Some(s)) if s == "prod-child"));
    }

    #[test]
    fn resolve_save_target_mismatch_combines_name_and_uuid_prefix() {
        // The relabel embeds the UUID prefix so a save under (site, "Other Org")
        // can never silently overwrite an unrelated existing "Other Org" session
        // on the same site that has a different underlying UUID.
        let req = "00000000-1111-2222-3333-444444444444";
        let act = "11111111-2222-3333-4444-555555555555";
        let t = resolve_save_target(Some("prod-child"), Some(req), Some(act), Some("Other Org"));
        match t {
            SaveTarget::Reassigned {
                label,
                requested_uuid,
                actual_uuid,
                actual_name,
            } => {
                assert_eq!(label, "Other Org [11111111]");
                assert_eq!(requested_uuid, req);
                assert_eq!(actual_uuid, act);
                assert_eq!(actual_name.as_deref(), Some("Other Org"));
            }
            other => panic!("expected Reassigned, got {other:?}"),
        }
    }

    #[test]
    fn resolve_save_target_mismatch_falls_back_to_uuid_prefix_only() {
        // No dd_org_name in the callback (older issuer or unusual flow): the
        // 8-char UUID prefix becomes the entire label so it's still stable
        // and distinguishable.
        let req = "00000000-1111-2222-3333-444444444444";
        let act = "11111111-2222-3333-4444-555555555555";
        let t = resolve_save_target(Some("prod-child"), Some(req), Some(act), None);
        match t {
            SaveTarget::Reassigned {
                label, actual_name, ..
            } => {
                assert_eq!(label, "11111111");
                assert!(actual_name.is_none());
            }
            other => panic!("expected Reassigned, got {other:?}"),
        }
    }

    #[test]
    fn resolve_save_target_mismatch_treats_empty_org_name_as_missing() {
        // An empty `dd_org_name=""` shouldn't produce labels like " [11111111]"
        // with a leading space — fall back to the UUID-only label instead.
        let req = "00000000-1111-2222-3333-444444444444";
        let act = "11111111-2222-3333-4444-555555555555";
        let t = resolve_save_target(Some("prod-child"), Some(req), Some(act), Some(""));
        match t {
            SaveTarget::Reassigned { label, .. } => {
                assert_eq!(label, "11111111");
            }
            other => panic!("expected Reassigned, got {other:?}"),
        }
    }

    #[test]
    fn resolve_save_target_actual_uuid_missing_keeps_requested() {
        // If the issuer didn't echo dd_oid, we have no comparison to make;
        // trust the user's --org label and the in-flight UUID we sent.
        let req = "00000000-1111-2222-3333-444444444444";
        let t = resolve_save_target(Some("prod-child"), Some(req), None, None);
        assert!(matches!(t, SaveTarget::Requested(Some(s)) if s == "prod-child"));
    }

    #[test]
    fn resolve_save_target_default_org_mismatch_stays_default() {
        // Default-org login with a stale stored UUID hint that no longer
        // matches the issuer's response: `pup auth login` is supposed to be
        // idempotent for the default session, so we don't relabel onto a
        // phantom slot. The default session gets refreshed in place; the
        // stored UUID hint is updated by the caller from the callback.
        let req = "00000000-1111-2222-3333-444444444444";
        let act = "11111111-2222-3333-4444-555555555555";
        let t = resolve_save_target(None, Some(req), Some(act), Some("Other Org"));
        assert!(matches!(t, SaveTarget::Requested(None)));
    }

    // looks_like_uuid -----------------------------------------------------------

    #[test]
    fn looks_like_uuid_accepts_canonical_form() {
        assert!(looks_like_uuid("00000000-1111-2222-3333-444444444444"));
        assert!(looks_like_uuid("aabbccdd-1111-2222-3333-eeffaabbccdd"));
        assert!(looks_like_uuid("AABBCCDD-1111-2222-3333-EEFFAABBCCDD"));
    }

    #[test]
    fn looks_like_uuid_rejects_garbage() {
        assert!(!looks_like_uuid(""));
        assert!(!looks_like_uuid("not-a-uuid"));
        // Wrong length
        assert!(!looks_like_uuid("00000000-1111-2222-3333-44444444444"));
        // Non-hex character
        assert!(!looks_like_uuid("0000000g-1111-2222-3333-444444444444"));
        // Missing dash positions
        assert!(!looks_like_uuid("000000001111-2222-3333-44444444444444"));
        // Hostile injection attempt
        assert!(!looks_like_uuid("<script>alert(1)</script>            "));
    }

    #[tokio::test]
    async fn test_logout_with_org_removes_session_entry() {
        // save a session, then logout should remove just that org's entry
        // from sessions.json. PUP_CONFIG_DIR is read fresh by the session
        // registry helpers (unlike the frozen STORAGE singleton), so this
        // assertion is hermetic.
        let _lock = crate::test_support::lock_env().await;
        let tmp = TempDir::new("logout_session");
        std::env::set_var("PUP_CONFIG_DIR", tmp.path());
        std::env::set_var("DD_TOKEN_STORAGE", "file");

        let site = "logout-session.example.invalid";
        storage::save_session(&SessionEntry {
            site: site.into(),
            org: None,
            org_uuid: None,
        })
        .unwrap();
        storage::save_session(&SessionEntry {
            site: site.into(),
            org: Some("keep-me".into()),
            org_uuid: None,
        })
        .unwrap();

        let mut cfg = base_config();
        cfg.site = site.into();
        cfg.org = Some("keep-me".into());
        let result = logout(&cfg).await;

        let remaining = storage::list_sessions().unwrap();
        std::env::remove_var("DD_TOKEN_STORAGE");
        std::env::remove_var("PUP_CONFIG_DIR");

        assert!(result.is_ok());
        // The "keep-me" org entry was removed; the default-org entry for
        // the same site survives.
        assert!(
            remaining.iter().any(|s| s.site == site && s.org.is_none()),
            "default-org session for site should remain after logging out of a different org"
        );
        assert!(
            !remaining
                .iter()
                .any(|s| s.site == site && s.org.as_deref() == Some("keep-me")),
            "logged-out org session should be removed"
        );
    }
}
