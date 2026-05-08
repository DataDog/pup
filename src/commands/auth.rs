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
    let org_label = org.map(|o| format!(" (org: {o})")).unwrap_or_default();
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

    // 8. Resolve the save target. By default we save under the user-supplied
    // (site, org) label. But if the user hinted a UUID and the callback
    // returned a different `dd_oid`, it would be misleading to label the
    // resulting token with the requested org name — so we switch the label
    // to the one the OAuth server reports (`dd_org_name`, falling back to a
    // shortened UUID) and warn on stderr. The OAuth code is single-use, so
    // refusing to save would throw away the user's click; mislabeling is the
    // bigger risk and this side-steps it.
    let save_target = resolve_save_target(
        org,
        effective_org_uuid.as_deref(),
        result.dd_oid.as_deref(),
        result.dd_org_name.as_deref(),
    );
    let saved_org = save_target.org.as_deref();
    let saved_org_label = saved_org
        .map(|o| format!(" (org: {o})"))
        .unwrap_or_default();

    let location = with_storage(|store| {
        store.save_tokens(site, saved_org, &tokens)?;
        Ok(store.storage_location())
    })?;

    // Register this session in the session registry, tagged with the
    // authoritative `dd_oid` from the callback so re-auth without the flag
    // keeps hinting the right org. The callback's UUID is preferred over the
    // CLI/stored value because it reflects the org the user actually
    // consented for.
    let saved_org_uuid = result.dd_oid.as_deref().or(effective_org_uuid.as_deref());
    storage::save_session(site, saved_org, saved_org_uuid)?;

    let expires_at = chrono::DateTime::from_timestamp(tokens.issued_at + tokens.expires_in, 0)
        .map(|dt| dt.with_timezone(&chrono::Local).to_rfc3339())
        .unwrap_or_else(|| format!("in {} hours", tokens.expires_in / 3600));

    eprintln!("\n✅ Login successful{saved_org_label}!");
    eprintln!("   Access token expires: {expires_at}");
    eprintln!("   Token stored in: {location}");

    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
struct SaveTarget {
    org: Option<String>,
}

/// Pick the `(site, org)` save target for a finished login.
///
/// The default is the user-supplied `--org` label. If the user hinted a UUID
/// but the OAuth server returned a different one, we switch the label to
/// `dd_org_name` (or a shortened UUID) and warn on stderr — saving under the
/// requested label would mislabel the token. Any other case (no hint, hint
/// matches, server didn't return `dd_oid`) keeps the requested label.
#[cfg(not(target_arch = "wasm32"))]
fn resolve_save_target(
    requested_org: Option<&str>,
    requested_uuid: Option<&str>,
    actual_uuid: Option<&str>,
    actual_org_name: Option<&str>,
) -> SaveTarget {
    let mismatch = match (requested_uuid, actual_uuid) {
        (Some(req), Some(actual)) => !req.eq_ignore_ascii_case(actual),
        _ => false,
    };
    if !mismatch {
        return SaveTarget {
            org: requested_org.map(String::from),
        };
    }
    // Mismatch: swap to the actual org. Prefer the human-readable
    // `dd_org_name`; if that's absent, fall back to the UUID's first 8 chars
    // so the saved label is still distinguishable on disk.
    let actual_label = actual_org_name
        .map(String::from)
        .or_else(|| actual_uuid.map(|u| u.chars().take(8).collect::<String>()));
    eprintln!(
        "⚠️  Requested org UUID {} but OAuth returned {}{}. \
         Saving token under {} instead of the requested label.",
        requested_uuid.unwrap_or("?"),
        actual_uuid.unwrap_or("?"),
        actual_org_name
            .map(|n| format!(" ({n})"))
            .unwrap_or_default(),
        actual_label
            .as_deref()
            .map(|l| format!("\"{l}\""))
            .unwrap_or_else(|| "the default session".to_string()),
    );
    SaveTarget { org: actual_label }
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
    let org_label = org.map(|o| format!(" (org: {o})")).unwrap_or_default();
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

                let org_label = org.map(|o| format!(" (org: {o})")).unwrap_or_default();
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
                let org_label = org.map(|o| format!(" (org: {o})")).unwrap_or_default();
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

    let org_label = org.map(|o| format!(" (org: {o})")).unwrap_or_default();
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

        storage::save_session("datadoghq.com", None, None).unwrap();
        storage::save_session("datadoghq.com", Some("prod-child"), None).unwrap();

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
        assert_eq!(t.org.as_deref(), Some("prod-child"));
    }

    #[test]
    fn resolve_save_target_hint_matches_keeps_requested_org() {
        let uuid = "00000000-1111-2222-3333-444444444444";
        let t = resolve_save_target(
            Some("prod-child"),
            Some(uuid),
            Some(uuid),
            Some("Datadog HQ"),
        );
        assert_eq!(t.org.as_deref(), Some("prod-child"));
    }

    #[test]
    fn resolve_save_target_hint_matches_case_insensitive() {
        // UUIDs are hex; normalise case so a mismatch in the issuer's
        // canonicalisation doesn't trip the warn-and-relabel path.
        let upper = "AABBCCDD-1111-2222-3333-EEFFAABBCCDD";
        let lower = "aabbccdd-1111-2222-3333-eeffaabbccdd";
        let t = resolve_save_target(Some("prod-child"), Some(upper), Some(lower), None);
        assert_eq!(t.org.as_deref(), Some("prod-child"));
    }

    #[test]
    fn resolve_save_target_mismatch_uses_dd_org_name() {
        let req = "00000000-1111-2222-3333-444444444444";
        let act = "11111111-2222-3333-4444-555555555555";
        let t = resolve_save_target(Some("prod-child"), Some(req), Some(act), Some("Other Org"));
        assert_eq!(t.org.as_deref(), Some("Other Org"));
    }

    #[test]
    fn resolve_save_target_mismatch_falls_back_to_uuid_prefix() {
        // No dd_org_name in the callback (older issuer or unusual flow): use
        // the first 8 chars of the actual UUID as a stable, distinguishable
        // label rather than reusing the wrong --org name.
        let req = "00000000-1111-2222-3333-444444444444";
        let act = "11111111-2222-3333-4444-555555555555";
        let t = resolve_save_target(Some("prod-child"), Some(req), Some(act), None);
        assert_eq!(t.org.as_deref(), Some("11111111"));
    }

    #[test]
    fn resolve_save_target_actual_uuid_missing_keeps_requested() {
        // If the issuer didn't echo dd_oid, we have no comparison to make;
        // trust the user's --org label and the in-flight UUID we sent.
        let req = "00000000-1111-2222-3333-444444444444";
        let t = resolve_save_target(Some("prod-child"), Some(req), None, None);
        assert_eq!(t.org.as_deref(), Some("prod-child"));
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
        storage::save_session(site, None, None).unwrap();
        storage::save_session(site, Some("keep-me"), None).unwrap();

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
