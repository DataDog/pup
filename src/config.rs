use anyhow::{bail, Result};
#[cfg(not(feature = "browser"))]
use serde::Deserialize;
#[cfg(not(feature = "browser"))]
use std::collections::HashMap;
#[cfg(all(not(feature = "browser"), not(target_arch = "wasm32")))]
use std::io::Write;
use std::path::PathBuf;

/// Runtime configuration with precedence: flag > env > file > default.
pub struct Config {
    pub api_key: Option<String>,
    pub app_key: Option<String>,
    pub access_token: Option<String>,
    pub site: String,
    /// True if `site` was explicitly set via DD_SITE env var, --site flag, or
    /// config file. False if it was derived from a stored session for the
    /// current org or fell through to the `datadoghq.com` default. Used to
    /// decide whether `--org` should pull the site from the session registry.
    pub site_explicit: bool,
    pub org: Option<String>,
    pub output_format: OutputFormat,
    pub auto_approve: bool,
    pub agent_mode: bool,
    pub read_only: bool,
    /// jq expression applied to command output before formatting (`--jq` flag).
    pub jq: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum OutputFormat {
    Json,
    Table,
    Yaml,
    Csv,
    Tsv,
}

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OutputFormat::Json => write!(f, "json"),
            OutputFormat::Table => write!(f, "table"),
            OutputFormat::Yaml => write!(f, "yaml"),
            OutputFormat::Csv => write!(f, "csv"),
            OutputFormat::Tsv => write!(f, "tsv"),
        }
    }
}

impl std::str::FromStr for OutputFormat {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "json" => Ok(OutputFormat::Json),
            "table" => Ok(OutputFormat::Table),
            "yaml" => Ok(OutputFormat::Yaml),
            "csv" => Ok(OutputFormat::Csv),
            "tsv" => Ok(OutputFormat::Tsv),
            _ => bail!("invalid output format: {s:?} (expected json, table, yaml, csv, or tsv)"),
        }
    }
}

/// Per-profile settings in the config file.
#[cfg(not(feature = "browser"))]
#[derive(Deserialize, Default)]
struct ProfileConfig {
    /// Comma-separated OAuth scopes to request when logging in with this profile.
    scopes: Option<String>,
}

/// Config file structure (~/.config/pup/config.yaml)
#[cfg(not(feature = "browser"))]
#[derive(Deserialize, Default)]
struct FileConfig {
    api_key: Option<String>,
    app_key: Option<String>,
    access_token: Option<String>,
    site: Option<String>,
    org: Option<String>,
    output: Option<String>,
    auto_approve: Option<bool>,
    read_only: Option<bool>,
    /// Default OAuth scopes to request on login (comma-separated).
    scopes: Option<String>,
    /// Per-org profile settings. Profile key matches the --org value used at login.
    profiles: Option<HashMap<String, ProfileConfig>>,
    /// Non-Datadog hosts trusted to receive credentials without a per-invocation
    /// prompt. Mirrors `--trust-site` and `PUP_TRUST_SITE` but persists across
    /// invocations. Each entry is normalized via `normalize_site` at comparison
    /// time, so `app.foo.com` and `foo.com` both match `foo.com`.
    trusted_sites: Option<Vec<String>>,
}

impl Config {
    /// Load configuration with precedence: flag overrides > env > file > keychain > defaults.
    /// Flag overrides are applied by the caller after this returns.
    #[cfg(not(feature = "browser"))]
    pub fn from_env() -> Result<Self> {
        let file_cfg = load_config_file().unwrap_or_default();

        let access_token = env_or("DD_ACCESS_TOKEN", file_cfg.access_token);
        let explicit_site = env_or("DD_SITE", file_cfg.site);
        let site_explicit = explicit_site.is_some();
        let org = env_or("DD_ORG", file_cfg.org); // flag override applied in main_inner

        // Reject a whitespace-only explicit site before normalize_site can silently
        // convert it to "datadoghq.com". `env_or` filters empty strings but not
        // whitespace-only ones; without this check, DD_SITE="  " would result in
        // site="datadoghq.com" with site_explicit=true, blocking --org from correcting
        // the site and silently routing to the wrong endpoint.
        if let Some(ref raw) = explicit_site {
            if raw.trim().is_empty() {
                bail!("--site / DD_SITE must not be empty or whitespace-only");
            }
        }

        // Resolve site: explicit env/file > saved session for this org > default.
        // Custom-site logins record their site in the session registry, so a
        // bare `--org foo` (or DD_ORG=foo) should pick up that site automatically.
        let raw_site = explicit_site
            .or_else(|| {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    org.as_deref()
                        .and_then(crate::auth::storage::find_session_site)
                }
                #[cfg(target_arch = "wasm32")]
                {
                    None
                }
            })
            .or_else(|| {
                // When no org is set and no explicit site was given, fall back to
                // the single no-org ("default") session's site. This fixes bare
                // commands after a datacenter-switched login (e.g. datadoghq.eu):
                // the token is stored under the switched site but the resolver
                // previously hard-coded datadoghq.com. This fires whenever the
                // DD_ORG/file org is unset, which includes `--org <flag>`
                // invocations (the flag is applied later, in main_inner); for
                // those, apply_org_override re-resolves the site from the named
                // org's session (or resets to the default when it has none), so a
                // named org never keeps the no-org session's site.
                #[cfg(not(target_arch = "wasm32"))]
                {
                    if org.is_none() {
                        crate::auth::storage::find_default_session_site()
                    } else {
                        None
                    }
                }
                #[cfg(target_arch = "wasm32")]
                {
                    None
                }
            })
            .unwrap_or_else(|| "datadoghq.com".into());
        let site = normalize_site(&raw_site);
        // Validate all sources — explicit DD_SITE/config-file and session-derived —
        // to catch URL-smuggling values before they reach URL construction.
        // Session data is pup-written and should always be valid, but we validate
        // unconditionally so a tampered sessions file is rejected loudly rather than
        // silently routing to an attacker-controlled host.
        validate_site(&site)?;

        // If no token from env/file, try loading from keychain/storage (where `pup auth login` saves)
        #[cfg(not(target_arch = "wasm32"))]
        let access_token = access_token.or_else(|| load_token_from_storage(&site, org.as_deref()));

        let cfg = Config {
            api_key: env_or("DD_API_KEY", file_cfg.api_key),
            app_key: env_or("DD_APP_KEY", file_cfg.app_key),
            access_token,
            site,
            site_explicit,
            org,
            // `PUP_OUTPUT` is the variable pup injects into extension subprocesses,
            // so a child `pup` call inherits the parent's format. `DD_OUTPUT` (the
            // user-facing variable) still wins when both are set.
            //
            // Ambient config (env vars, config file) degrades to JSON on an
            // unparseable value rather than erroring — this is deliberate: a bad
            // value here should not make every command fail. The explicit
            // `--output` flag, by contrast, is validated and errors loudly
            // (see `resolve_output_format` in main.rs).
            output_format: env_or("DD_OUTPUT", None)
                .or_else(|| env_or("PUP_OUTPUT", file_cfg.output))
                .and_then(|s| s.parse().ok())
                .unwrap_or(OutputFormat::Json),
            // `PUP_AUTO_APPROVE` / `PUP_READ_ONLY` are injected into extension
            // subprocesses so a child `pup` call inherits the parent's mode flags.
            auto_approve: env_bool("DD_AUTO_APPROVE")
                || env_bool("DD_CLI_AUTO_APPROVE")
                || env_bool("PUP_AUTO_APPROVE")
                || file_cfg.auto_approve.unwrap_or(false),
            agent_mode: false, // set by caller from --agent flag or useragent detection
            read_only: env_bool("DD_READ_ONLY")
                || env_bool("DD_CLI_READ_ONLY")
                || env_bool("PUP_READ_ONLY")
                || file_cfg.read_only.unwrap_or(false),
            // `PUP_FILTER` is injected into extension subprocesses (like
            // `PUP_OUTPUT` above) so a child `pup` call inherits the parent's
            // --jq expression; an explicit --jq flag still overrides it.
            jq: env_or("PUP_FILTER", None),
        };

        Ok(cfg)
    }

    /// Create configuration from explicit parameters (no env vars or filesystem).
    /// Used by the browser WASM build where `std::env` is unavailable.
    ///
    /// Callers are the pup browser extension (trusted internal code). The `site`
    /// value originates from pup's own stored session data or from an in-extension
    /// UI — it is never passed directly from untrusted browser content. Validation
    /// at this layer is therefore omitted; the value is normalized and used as-is.
    #[cfg(feature = "browser")]
    pub fn from_params(
        site: String,
        access_token: Option<String>,
        api_key: Option<String>,
        app_key: Option<String>,
    ) -> Self {
        Config {
            api_key,
            app_key,
            access_token,
            site: normalize_site(&site),
            site_explicit: true,
            org: None,
            output_format: OutputFormat::Json,
            auto_approve: false,
            agent_mode: false,
            read_only: false,
            jq: None,
        }
    }

    /// Override the site as if the user passed it via DD_SITE / `--site` /
    /// config file. Validates the raw input *before* normalization so that an
    /// empty `--site ""` is rejected rather than silently converted to
    /// `"datadoghq.com"` by `normalize_site`'s empty-string fallback.
    /// Keeps `site` and `site_explicit` in lockstep so a later
    /// `apply_org_override` does not silently swap a user-pinned site for a
    /// session-derived one.
    pub fn set_site_explicit(&mut self, site: String) -> Result<()> {
        let raw = site.trim();
        if raw.is_empty() {
            bail!("--site / DD_SITE must not be empty");
        }
        let normalized = normalize_site(raw);
        validate_site(&normalized)?;
        self.site = normalized;
        self.site_explicit = true;
        Ok(())
    }

    /// Ensure credentials may be sent to `self.site`. Datadog-owned hosts are
    /// always trusted. A non-Datadog host (vanity typo or enterprise proxy) must
    /// be explicitly opted in, else — when a terminal is available — the user is
    /// prompted once for this invocation; with no terminal and no opt-in we fail
    /// closed rather than silently leak credentials.
    ///
    /// Opt-in precedence mirrors pup's flag > env > config ladder:
    ///   --trust-site flag  >  PUP_TRUST_SITE=1 env  >  trusted_sites config.
    ///
    /// `trusted_sites` is sourced from the config file at the call site (see
    /// [`configured_trusted_sites`]) rather than carried on `Config`, so this
    /// security check stays self-contained.
    #[cfg(not(feature = "browser"))]
    pub fn ensure_site_trusted(
        &self,
        trust_site_flag: bool,
        interactive: bool,
        trusted_sites: &[String],
    ) -> Result<()> {
        // Zero friction for the common case: all Datadog-owned hosts are always trusted.
        if is_datadog_owned_host(&self.site) {
            return Ok(());
        }

        // --trust-site on this invocation.
        if trust_site_flag {
            return Ok(());
        }

        // PUP_TRUST_SITE=1 in the environment: emit one informational line (so a
        // typo'd env value still surfaces) and proceed.
        if env_bool("PUP_TRUST_SITE") {
            eprintln!(
                "Using non-Datadog host '{}' (trusted via PUP_TRUST_SITE)",
                self.site
            );
            return Ok(());
        }

        // trusted_sites in config: any entry that normalizes to the current site is trusted.
        if trusted_sites
            .iter()
            .any(|entry| normalize_site(entry) == self.site)
        {
            return Ok(());
        }

        // Interactive fallback: prompt the user once for this invocation.
        #[cfg(not(target_arch = "wasm32"))]
        if interactive {
            eprintln!("⚠️  WARNING: '{}' is not a Datadog-owned host.", self.site);
            eprintln!("    pup will send your credentials there.");
            eprint!("    Trust this host for this command? [y/N]: ");
            std::io::stderr().flush().ok();

            let mut s = String::new();
            std::io::stdin().read_line(&mut s)?;

            if matches!(s.trim().to_lowercase().as_str(), "y" | "yes") {
                eprintln!(
                    "    To skip this prompt next time, add '{}' to trusted_sites in your \
                     pup config, pass --trust-site, or set PUP_TRUST_SITE=1.",
                    self.site
                );
                return Ok(());
            }

            bail!(
                "aborted: credentials will not be sent to non-Datadog host '{}'",
                self.site
            );
        }

        // No terminal and no opt-in: fail closed and name every remediation.
        bail!(
            "'{}' is not a Datadog-owned host and no terminal was available to prompt. \
             To send credentials there, use one of: \
             --trust-site (this invocation), \
             PUP_TRUST_SITE=1 (env, this invocation), \
             or add it to trusted_sites in your pup config (durable).",
            self.site
        )
    }

    /// Validate that sufficient auth credentials are configured.
    pub fn validate_auth(&self) -> Result<()> {
        if self.access_token.is_none() && (self.api_key.is_none() || self.app_key.is_none()) {
            #[cfg(all(not(feature = "browser"), not(target_arch = "wasm32")))]
            if has_stored_refresh_token(&self.site, self.org.as_deref()) {
                bail!(
                    "authentication expired. Run 'pup auth refresh' to renew your session, \
                     or 'pup auth login' to start a new one"
                );
            }
            bail!(
                "authentication required: set DD_ACCESS_TOKEN for bearer auth, \
                 run 'pup auth login' for OAuth2, \
                 or set DD_API_KEY and DD_APP_KEY for API+APP key auth"
            );
        }
        Ok(())
    }

    /// Validate that both DD_API_KEY and DD_APP_KEY are configured.
    /// Used for endpoints that require API key auth and do not accept OAuth2 tokens.
    pub fn validate_api_and_app_keys(&self) -> Result<()> {
        if self.api_key.is_none() || self.app_key.is_none() {
            bail!(
                "this command requires both DD_API_KEY and DD_APP_KEY — \
                 OAuth2 bearer tokens are not supported here"
            );
        }
        Ok(())
    }

    /// Validate that DD_API_KEY is configured for API-key-only endpoints.
    pub fn validate_api_key_only(&self) -> Result<()> {
        if self.api_key.is_none() {
            bail!(
                "this command requires DD_API_KEY — \
                 OAuth2 bearer tokens are not supported here"
            );
        }
        Ok(())
    }

    pub fn has_api_keys(&self) -> bool {
        self.api_key.is_some() && self.app_key.is_some()
    }

    pub fn has_bearer_token(&self) -> bool {
        self.access_token.is_some()
    }

    /// Returns the API host (e.g., `api.datadoghq.com`).
    ///
    /// Delegates to [`api_host_for`] after handling the `PUP_MOCK_SERVER` override.
    pub fn api_host(&self) -> String {
        #[cfg(not(feature = "browser"))]
        {
            if let Ok(mock) = std::env::var("PUP_MOCK_SERVER") {
                let host = mock
                    .trim_start_matches("http://")
                    .trim_start_matches("https://");
                return host.to_string();
            }
        }
        api_host_for(&self.site)
    }

    /// Returns the OAuth authorization host (e.g., `app.datadoghq.com`).
    ///
    /// Canonical sites get an `app.` prefix; literal hosts are used verbatim.
    pub fn auth_host(&self) -> String {
        auth_host_for(&self.site)
    }

    /// Returns the full API base URL (e.g., `https://api.datadoghq.com`).
    /// Respects `PUP_MOCK_SERVER` for testing (native/WASI only).
    ///
    /// Note: `api_base_url` and `api_host` handle `PUP_MOCK_SERVER` with
    /// intentionally different semantics. `api_host` strips the scheme and
    /// returns a bare host (e.g. `127.0.0.1:9999`), while `api_base_url`
    /// returns the raw value including `http://` so the mock server can use
    /// plain HTTP. Callers that need a full URL should use `api_base_url`;
    /// callers that need only the host (e.g. for SDK `server_variables["name"]`)
    /// should use `api_host`.
    pub fn api_base_url(&self) -> String {
        #[cfg(not(feature = "browser"))]
        {
            if let Ok(mock) = std::env::var("PUP_MOCK_SERVER") {
                return mock;
            }
        }
        format!("https://{}", self.api_host())
    }
}

/// Config file path: ~/.config/pup/config.yaml
/// Respects PUP_CONFIG_DIR env var for testing and custom installs.
#[cfg(not(target_arch = "wasm32"))]
pub fn config_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("PUP_CONFIG_DIR") {
        if !dir.is_empty() {
            return Some(PathBuf::from(dir));
        }
    }
    dirs::config_dir().map(|d| d.join("pup"))
}

/// WASI: use PUP_CONFIG_DIR env var or return None
#[cfg(all(target_arch = "wasm32", not(feature = "browser")))]
pub fn config_dir() -> Option<PathBuf> {
    std::env::var("PUP_CONFIG_DIR").ok().map(PathBuf::from)
}

/// Browser WASM: no filesystem access
#[cfg(feature = "browser")]
pub fn config_dir() -> Option<PathBuf> {
    None
}

/// Returns candidate config.yaml paths in priority order.
///
/// On macOS, `dirs::config_dir()` returns `~/Library/Application Support` rather
/// than `~/.config`. To keep the documented `~/.config/pup/config.yaml` path
/// working cross-platform, the XDG-style path is checked as a fallback when
/// `PUP_CONFIG_DIR` has not been set explicitly.
#[cfg(not(feature = "browser"))]
fn config_file_candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(dir) = config_dir() {
        paths.push(dir.join("config.yaml"));
    }
    // On macOS, also try the XDG-style path as a fallback. Skip when
    // PUP_CONFIG_DIR is set so explicit overrides are fully respected.
    #[cfg(all(not(target_arch = "wasm32"), target_os = "macos"))]
    if std::env::var("PUP_CONFIG_DIR")
        .ok()
        .filter(|s| !s.is_empty())
        .is_none()
    {
        if let Some(home) = dirs::home_dir() {
            let xdg = home.join(".config/pup/config.yaml");
            if !paths.contains(&xdg) {
                paths.push(xdg);
            }
        }
    }
    paths
}

#[cfg(not(feature = "browser"))]
fn load_config_file() -> Option<FileConfig> {
    let contents = config_file_candidates()
        .into_iter()
        .find_map(|p| std::fs::read_to_string(p).ok())?;
    serde_norway::from_str(&contents).ok()
}

/// Load configured login scopes for a given org profile from the config file.
/// Profile key matches the --org value; falls back to top-level scopes field.
/// Returns None if no scopes are configured (caller uses defaults).
#[cfg(not(feature = "browser"))]
pub fn load_configured_scopes(org: Option<&str>) -> Option<Vec<String>> {
    let file_cfg = load_config_file()?;

    // Check per-org profile first
    if let (Some(org_name), Some(profiles)) = (org, &file_cfg.profiles) {
        if let Some(profile) = profiles.get(org_name) {
            if let Some(scopes_str) = &profile.scopes {
                let scopes = parse_scopes(scopes_str);
                if !scopes.is_empty() {
                    return Some(scopes);
                }
            }
        }
    }

    // Fall back to top-level scopes
    let scopes = parse_scopes(file_cfg.scopes.as_deref()?);
    if scopes.is_empty() {
        None
    } else {
        Some(scopes)
    }
}

/// Load the `trusted_sites` allowlist from the config file (empty if unset).
/// Read at gate time rather than carried on [`Config`] so the trust feature
/// stays self-contained; see [`Config::ensure_site_trusted`].
#[cfg(not(feature = "browser"))]
pub fn configured_trusted_sites() -> Vec<String> {
    load_config_file()
        .and_then(|c| c.trusted_sites)
        .unwrap_or_default()
}

/// Returns `true` when `site` may receive credentials without an interactive
/// prompt: it is Datadog-owned, or opted in via `PUP_TRUST_SITE` or the
/// `trusted_sites` config list. This is the non-interactive subset of the
/// [`Config::ensure_site_trusted`] ladder (it omits `--trust-site` and the
/// prompt) — keep the two in sync. Used to gate background network calls that
/// happen with no terminal context, such as the implicit token refresh in
/// [`load_token_from_storage`].
#[cfg(not(feature = "browser"))]
pub fn site_trusted_without_prompt(site: &str, trusted_sites: &[String]) -> bool {
    is_datadog_owned_host(site)
        || env_bool("PUP_TRUST_SITE")
        || trusted_sites
            .iter()
            .any(|entry| normalize_site(entry) == site)
}

/// Parse a comma-separated scope string into a Vec of trimmed, non-empty strings.
#[cfg(not(feature = "browser"))]
pub fn parse_scopes(s: &str) -> Vec<String> {
    s.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

/// Apply an org override (from `--org` or extension global flag) to a config.
///
/// Sets `cfg.org` to the new value, and — when no site was explicitly provided
/// via DD_SITE/`--site`/config — adopts the site recorded in the session
/// registry for that org. Then re-keys the access token to the resulting
/// (site, org) pair from storage, unless DD_ACCESS_TOKEN is already set in
/// the environment. May leave `cfg.access_token` at `None` if no token is
/// stored for the new pair.
///
/// Returns an error if the stored session site fails validation (tampered
/// session file). Consistent with `from_env`, which bails on an invalid site
/// rather than routing silently to the wrong endpoint.
///
/// Centralized so the binary entry point and the extension dispatcher share
/// one resolution path.
#[cfg(all(not(feature = "browser"), not(target_arch = "wasm32")))]
pub fn apply_org_override(cfg: &mut Config, org: String) -> Result<()> {
    cfg.org = Some(org);
    if !cfg.site_explicit {
        if let Some(saved_site) = cfg
            .org
            .as_deref()
            .and_then(crate::auth::storage::find_session_site)
        {
            let normalized = normalize_site(&saved_site);
            // Session data is pup-written and should always be valid, but bail
            // on an invalid value so a tampered sessions file is rejected loudly
            // rather than silently routing to the wrong (or attacker-controlled) host.
            // Wrap with context so the user knows how to recover.
            validate_site(&normalized).map_err(|e| {
                anyhow::anyhow!(
                    "session for org {:?} contains an invalid site {:?}: {}. \
                     Run 'pup auth login --org {}' to replace the corrupted session.",
                    cfg.org.as_deref().unwrap_or("(unknown)"),
                    saved_site,
                    e,
                    cfg.org.as_deref().unwrap_or("")
                )
            })?;
            cfg.site = normalized;
        } else {
            // The named org has no saved session. Reset to the default site so a
            // `--org` flag behaves like `DD_ORG` and does not inherit the no-org
            // default-session site that `from_env` may have adopted (it runs
            // before this override and sees org.is_none()). Without this, e.g.
            // `pup auth login --org new-org` with an existing datadoghq.eu default
            // session would misroute the login to EU. Gated on !site_explicit, so
            // an explicit DD_SITE/--site still wins.
            cfg.site = normalize_site("datadoghq.com");
        }
    }
    if std::env::var("DD_ACCESS_TOKEN")
        .ok()
        .filter(|s| !s.is_empty())
        .is_none()
    {
        cfg.access_token = load_token_from_storage(&cfg.site, cfg.org.as_deref());
    }
    Ok(())
}

/// Try to load a valid (non-expired) access token from keychain/file storage.
/// If the token is expired, attempts an automatic refresh using the stored refresh token.
/// Returns None silently on any error — callers fall through to other auth methods.
#[cfg(all(not(feature = "browser"), not(target_arch = "wasm32")))]
pub fn load_token_from_storage(site: &str, org: Option<&str>) -> Option<String> {
    let guard = crate::auth::storage::get_storage().ok()?;
    let lock = guard.lock().ok()?;
    let store = lock.as_ref()?;
    let tokens = store.load_tokens(site, org).ok()??;
    let creds = store.load_client_credentials(site).ok().flatten();

    drop(lock);

    let result = resolve_token(tokens, creds.as_ref(), |refresh_token, creds| {
        // The implicit refresh runs during config load, before the command-level
        // trust gate, and would POST the refresh token to `site`. Don't contact a
        // non-Datadog host that hasn't been trusted non-interactively; treat the
        // token as expired instead and let `ensure_site_trusted` prompt/fail closed
        // at the command boundary.
        if !site_trusted_without_prompt(site, &configured_trusted_sites()) {
            return None;
        }
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let dcr_client = crate::auth::dcr::DcrClient::new(site);
                dcr_client.refresh_token(refresh_token, creds).await.ok()
            })
        })
    });

    match result {
        ResolvedToken::Valid(access_token) => Some(access_token),
        ResolvedToken::Refreshed(new_tokens) => {
            let guard = crate::auth::storage::get_storage().ok()?;
            let lock = guard.lock().ok()?;
            let store = lock.as_ref()?;
            store.save_tokens(site, org, &new_tokens).ok()?;
            eprintln!("🔄 Access token refreshed automatically.");
            Some(new_tokens.access_token)
        }
        ResolvedToken::Expired => None,
    }
}

/// Returns true if a non-empty refresh token exists in storage for the given site/org.
/// Used to tailor the auth-required error message.
#[cfg(all(not(feature = "browser"), not(target_arch = "wasm32")))]
fn has_stored_refresh_token(site: &str, org: Option<&str>) -> bool {
    let Ok(guard) = crate::auth::storage::get_storage() else {
        return false;
    };
    let Ok(lock) = guard.lock() else { return false };
    let Some(store) = lock.as_ref() else {
        return false;
    };
    matches!(
        store.load_tokens(site, org),
        Ok(Some(ref t)) if !t.refresh_token.is_empty()
    )
}

#[cfg(all(not(feature = "browser"), not(target_arch = "wasm32")))]
enum ResolvedToken {
    Valid(String),
    Refreshed(crate::auth::types::TokenSet),
    Expired,
}

#[cfg(all(not(feature = "browser"), not(target_arch = "wasm32")))]
fn resolve_token<F>(
    tokens: crate::auth::types::TokenSet,
    creds: Option<&crate::auth::types::ClientCredentials>,
    refresh_fn: F,
) -> ResolvedToken
where
    F: FnOnce(&str, &crate::auth::types::ClientCredentials) -> Option<crate::auth::types::TokenSet>,
{
    if !tokens.is_expired() {
        return ResolvedToken::Valid(tokens.access_token);
    }

    if tokens.refresh_token.is_empty() {
        return ResolvedToken::Expired;
    }

    let creds = match creds {
        Some(c) => c,
        None => return ResolvedToken::Expired,
    };

    match refresh_fn(&tokens.refresh_token, creds) {
        Some(new_tokens) => ResolvedToken::Refreshed(new_tokens),
        None => ResolvedToken::Expired,
    }
}

/// Canonical Datadog sites that use `api.{site}` for API calls and `app.{site}` for
/// OAuth. Any other site value is treated as a **literal host** and used verbatim for
/// both API requests and OAuth flows (enables vanity-domain and gateway use cases).
pub const KNOWN_SITES: &[&str] = &[
    "datadoghq.com",
    "us3.datadoghq.com",
    "us5.datadoghq.com",
    "ap1.datadoghq.com",
    "ap2.datadoghq.com",
    "datadoghq.eu",
    "ddog-gov.com",
    "datad0g.com", // staging
];

/// Returns `true` when `site` is a known canonical Datadog site (applies `api.`/`app.`
/// prefixes at request time) or an oncall passthrough. Everything else is a literal host.
///
/// Note: oncall hosts match by substring (`contains("oncall")`), which is a pre-existing
/// convention. Both `api_host_for` and `auth_host_for` treat them as verbatim regardless
/// (the `!site.contains("oncall")` guard cancels the canonical prefix), so any
/// hostname that happens to contain "oncall" routes verbatim — the same outcome as a
/// literal host. The practical difference is only in `normalize_site`, which skips
/// prefix stripping for oncall hosts so the full subdomain is preserved.
pub fn is_canonical_site(site: &str) -> bool {
    site.contains("oncall") || KNOWN_SITES.contains(&site)
}

/// Returns `true` for the canonical Datadog sites that take `api.`/`app.`
/// subdomain derivation — and whose backend region the OAuth callback may
/// refine (e.g. `datadoghq.com` → `us3.datadoghq.com`).
///
/// This is the single predicate that separates "Datadog-managed site, derive
/// subdomains and trust the callback region" from "use this host verbatim".
/// Oncall hosts are canonical for token-storage purposes (see
/// [`is_canonical_site`]) but are addressed verbatim, so they are excluded
/// here — same outcome as a literal vanity/gateway host. Keeping `api_host_for`,
/// `auth_host_for`, and the login flow's site resolution on one predicate stops
/// the three from drifting (an earlier version open-coded the guard three times
/// and one copy dropped the oncall exclusion).
pub fn uses_datadog_subdomains(site: &str) -> bool {
    is_canonical_site(site) && !site.contains("oncall")
}

/// Datadog-owned parent domains. Any host equal to one of these or a subdomain
/// of one is Datadog-controlled DNS. Distinct from [`KNOWN_SITES`] (which also
/// enumerates regional subdomains): this is the registrable-domain allowlist.
const DATADOG_BASE_DOMAINS: &[&str] = &[
    "datadoghq.com",
    "datadoghq.eu",
    "ddog-gov.com",
    "datad0g.com", // staging
];

/// Returns `true` when `host` is a Datadog-owned host — one of the canonical
/// parent domains or a subdomain of one (e.g. `us3.datadoghq.com`).
///
/// Used to bound which OAuth-callback `domain` values may be adopted as the
/// token-exchange host: a tampered callback (the stdin-paste login fallback can
/// carry an attacker-chosen `domain`) must not redirect the code exchange — and
/// the PKCE verifier / DCR client credentials sent with it — to a foreign host.
///
/// Two gates, both required:
/// 1. [`validate_site`] — rejects anything that isn't a bare DNS hostname. This
///    is essential, not redundant: without it a payload like
///    `evil.com/x.datadoghq.com` would pass the suffix check below
///    (`strip_suffix` leaves `"evil.com/x."`, which ends with `.`) yet parse as
///    the host `evil.com` with the rest as a URL path.
/// 2. The registrable-domain suffix allowlist — the host must equal a Datadog
///    base domain or be a subdomain of one. A well-formed but foreign hostname
///    (`evil.com`) passes gate 1 but fails here.
///
/// Suffix-based rather than a [`KNOWN_SITES`] exact match so a future Datadog
/// region (e.g. `us6.datadoghq.com`) keeps working without a code change.
pub fn is_datadog_owned_host(host: &str) -> bool {
    if validate_site(host).is_err() {
        return false;
    }
    DATADOG_BASE_DOMAINS
        .iter()
        .any(|base| host == *base || host.strip_suffix(base).is_some_and(|p| p.ends_with('.')))
}

/// Derive the API request host from a normalized site value.
///
/// - Canonical sites → `api.{site}` (e.g. `api.datadoghq.com`).
/// - Oncall passthroughs and literal hosts → verbatim (e.g. `mygateway.example.com`).
pub fn api_host_for(site: &str) -> String {
    if uses_datadog_subdomains(site) {
        format!("api.{site}")
    } else {
        site.to_string()
    }
}

/// Derive the OAuth authorization host from a normalized site value.
///
/// - Canonical sites → `app.{site}` (e.g. `app.datadoghq.com`).
/// - Oncall passthroughs and literal hosts → verbatim (e.g. `mycompany.datadoghq.com`).
pub fn auth_host_for(site: &str) -> String {
    if uses_datadog_subdomains(site) {
        format!("app.{site}")
    } else {
        site.to_string()
    }
}

/// Validate a site or host string to prevent URL smuggling.
///
/// Allows ASCII alphanumeric, `.`, `-`, and an optional single `:port` suffix (1–65535).
/// Rejects `/`, `#`, `?`, `@`, whitespace, `_`, embedded schemes, consecutive dots,
/// and leading/trailing dots. An empty string is also rejected.
pub fn validate_site(s: &str) -> Result<()> {
    if s.is_empty() {
        bail!("--site / DD_SITE must not be empty");
    }
    // Split off an optional trailing :port.
    let (host_part, port_part) = match s.rfind(':') {
        Some(idx) => {
            let port = &s[idx + 1..];
            if !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) {
                (&s[..idx], Some(port))
            } else {
                (s, None)
            }
        }
        None => (s, None),
    };
    if let Some(p) = port_part {
        let n: u32 = p.parse().unwrap_or(0);
        if n == 0 || n > 65535 {
            bail!("--site / DD_SITE port {p} is out of range (1–65535)");
        }
    }
    // Host part: DNS label chars only (alphanumeric, hyphen, dot).
    if !host_part
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
    {
        bail!(
            "--site / DD_SITE {s:?} contains invalid characters; \
             expected a DNS hostname (letters, digits, `.`, `-`) with an optional `:port`"
        );
    }
    if host_part.starts_with('.') || host_part.ends_with('.') || host_part.contains("..") {
        bail!(
            "--site / DD_SITE {s:?} has invalid dot structure \
             (leading, trailing, or consecutive dots)"
        );
    }
    Ok(())
}

/// Normalize a raw site value from user input into a canonical storage form.
///
/// Strips any `https://` / `http://` scheme and trailing `/`, then removes a single
/// leading `www.`, `app.`, or `api.` label. Oncall sites are passed through unchanged.
///
/// The stored value is then interpreted at request time via [`is_canonical_site`]:
/// known sites get `api.`/`app.` prefixed; everything else is used verbatim as a
/// literal host, which enables both vanity-domain SSO and custom gateway routing.
///
/// Examples:
///   `app.datadoghq.com`           → `datadoghq.com`   (canonical)
///   `www.datadoghq.com`           → `datadoghq.com`   (canonical)
///   `api.datadoghq.com`           → `datadoghq.com`   (canonical)
///   `app.us3.datadoghq.com`       → `us3.datadoghq.com` (canonical)
///   `us3.datadoghq.com`           → `us3.datadoghq.com` (canonical)
///   `mycompany.datadoghq.com`     → `mycompany.datadoghq.com` (literal)
///   `mygateway.example.com:8443`  → `mygateway.example.com:8443` (literal)
pub fn normalize_site(site: &str) -> String {
    let trimmed = site
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/');
    if trimmed.is_empty() {
        return "datadoghq.com".into();
    }
    if trimmed.contains("oncall") {
        return trimmed.to_string();
    }
    // Strip a single leading www./app./api. label.
    const STRIP_PREFIXES: &[&str] = &["www", "api", "app"];
    match trimmed.split_once('.') {
        Some((first, rest)) if STRIP_PREFIXES.contains(&first) => rest.to_string(),
        _ => trimmed.to_string(),
    }
}

#[cfg(not(feature = "browser"))]
fn env_or(key: &str, fallback: Option<String>) -> Option<String> {
    std::env::var(key)
        .ok()
        .filter(|s| !s.is_empty())
        .or(fallback)
}

#[cfg(not(feature = "browser"))]
fn env_bool(key: &str) -> bool {
    matches!(
        std::env::var(key)
            .unwrap_or_default()
            .to_lowercase()
            .as_str(),
        "true" | "1"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::storage::SessionEntry;
    use crate::test_utils::ENV_LOCK;

    fn make_cfg(api_key: Option<&str>, app_key: Option<&str>, token: Option<&str>) -> Config {
        Config {
            api_key: api_key.map(String::from),
            app_key: app_key.map(String::from),
            access_token: token.map(String::from),
            site: "datadoghq.com".into(),
            site_explicit: false,
            org: None,
            output_format: OutputFormat::Json,
            auto_approve: false,
            agent_mode: false,
            read_only: false,
            jq: None,
        }
    }

    #[test]
    fn test_output_format_parse() {
        assert_eq!("json".parse::<OutputFormat>().unwrap(), OutputFormat::Json);
        assert_eq!("JSON".parse::<OutputFormat>().unwrap(), OutputFormat::Json);
        assert_eq!(
            "table".parse::<OutputFormat>().unwrap(),
            OutputFormat::Table
        );
        assert_eq!("yaml".parse::<OutputFormat>().unwrap(), OutputFormat::Yaml);
        assert_eq!("csv".parse::<OutputFormat>().unwrap(), OutputFormat::Csv);
        assert_eq!("CSV".parse::<OutputFormat>().unwrap(), OutputFormat::Csv);
        assert_eq!("tsv".parse::<OutputFormat>().unwrap(), OutputFormat::Tsv);
        assert_eq!("TSV".parse::<OutputFormat>().unwrap(), OutputFormat::Tsv);
        assert!("xml".parse::<OutputFormat>().is_err());
    }

    #[test]
    fn test_output_format_display() {
        assert_eq!(OutputFormat::Json.to_string(), "json");
        assert_eq!(OutputFormat::Table.to_string(), "table");
        assert_eq!(OutputFormat::Yaml.to_string(), "yaml");
        assert_eq!(OutputFormat::Csv.to_string(), "csv");
        assert_eq!(OutputFormat::Tsv.to_string(), "tsv");
    }

    #[test]
    fn test_validate_api_and_app_keys_ok() {
        let cfg = make_cfg(Some("key"), Some("app"), None);
        assert!(cfg.validate_api_and_app_keys().is_ok());
    }

    #[test]
    fn test_validate_api_and_app_keys_bearer_only_fails() {
        let cfg = make_cfg(None, None, Some("token"));
        assert!(cfg.validate_api_and_app_keys().is_err());
    }

    #[test]
    fn test_validate_api_and_app_keys_missing_app_key_fails() {
        let cfg = make_cfg(Some("key"), None, None);
        assert!(cfg.validate_api_and_app_keys().is_err());
    }

    #[test]
    fn test_validate_api_key_only_accepts_api_key_without_app_key() {
        let cfg = make_cfg(Some("key"), None, None);
        assert!(cfg.validate_api_key_only().is_ok());
    }

    #[test]
    fn test_validate_api_key_only_rejects_bearer_only() {
        let cfg = make_cfg(None, None, Some("token"));
        assert!(cfg.validate_api_key_only().is_err());
    }

    #[test]
    fn test_validate_api_key_only_rejects_missing_api_key() {
        let cfg = make_cfg(None, Some("app"), None);
        assert!(cfg.validate_api_key_only().is_err());
    }

    #[test]
    fn test_validate_auth_api_keys() {
        let cfg = make_cfg(Some("key"), Some("app"), None);
        assert!(cfg.validate_auth().is_ok());
    }

    #[test]
    fn test_validate_auth_bearer() {
        let cfg = make_cfg(None, None, Some("token"));
        assert!(cfg.validate_auth().is_ok());
    }

    #[test]
    fn test_validate_auth_none() {
        let cfg = make_cfg(None, None, None);
        let err = cfg.validate_auth().unwrap_err();
        assert!(err.to_string().contains("pup auth login"));
    }

    #[test]
    fn test_validate_auth_partial_keys() {
        let cfg = make_cfg(Some("key"), None, None);
        assert!(cfg.validate_auth().is_err());
    }

    #[test]
    fn test_validate_auth_error_message_suggests_login_by_default() {
        // Use a site name that will never have stored tokens.
        let mut cfg = make_cfg(None, None, None);
        cfg.site = "no-tokens.test.invalid".into();
        let err = cfg.validate_auth().unwrap_err().to_string();
        assert!(
            err.contains("pup auth login"),
            "should suggest 'pup auth login' when no stored session: {err}"
        );
        assert!(
            !err.contains("pup auth refresh"),
            "should not suggest 'pup auth refresh' when no stored session: {err}"
        );
    }

    #[test]
    fn test_has_api_keys() {
        assert!(make_cfg(Some("k"), Some("a"), None).has_api_keys());
        assert!(!make_cfg(Some("k"), None, None).has_api_keys());
        assert!(!make_cfg(None, None, None).has_api_keys());
    }

    #[test]
    fn test_has_bearer_token() {
        assert!(make_cfg(None, None, Some("t")).has_bearer_token());
        assert!(!make_cfg(None, None, None).has_bearer_token());
    }

    #[test]
    fn test_api_host_standard() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::remove_var("PUP_MOCK_SERVER");
        let cfg = make_cfg(None, None, Some("t"));
        assert_eq!(cfg.api_host(), "api.datadoghq.com");
    }

    #[test]
    fn test_api_host_eu() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::remove_var("PUP_MOCK_SERVER");
        let mut cfg = make_cfg(None, None, Some("t"));
        cfg.site = "datadoghq.eu".into();
        assert_eq!(cfg.api_host(), "api.datadoghq.eu");
    }

    #[test]
    fn test_api_host_oncall() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::remove_var("PUP_MOCK_SERVER");
        let mut cfg = make_cfg(None, None, Some("t"));
        cfg.site = "navy.oncall.datadoghq.com".into();
        assert_eq!(cfg.api_host(), "navy.oncall.datadoghq.com");
    }

    // --- normalize_site unit tests ---

    #[test]
    fn test_normalize_site_plain() {
        assert_eq!(normalize_site("datadoghq.com"), "datadoghq.com");
    }

    #[test]
    fn test_normalize_site_app_prefix() {
        assert_eq!(normalize_site("app.datadoghq.com"), "datadoghq.com");
    }

    #[test]
    fn test_normalize_site_app_prefix_eu() {
        assert_eq!(normalize_site("app.datadoghq.eu"), "datadoghq.eu");
    }

    #[test]
    fn test_normalize_site_app_prefix_gov() {
        assert_eq!(normalize_site("app.ddog-gov.com"), "ddog-gov.com");
    }

    #[test]
    fn test_normalize_site_app_prefix_staging() {
        assert_eq!(normalize_site("app.datad0g.com"), "datad0g.com");
    }

    #[test]
    fn test_normalize_site_region_prefix() {
        assert_eq!(normalize_site("us3.datadoghq.com"), "us3.datadoghq.com");
    }

    #[test]
    fn test_normalize_site_app_and_region_prefix() {
        assert_eq!(normalize_site("app.us3.datadoghq.com"), "us3.datadoghq.com");
    }

    #[test]
    fn test_normalize_site_eu1_region() {
        assert_eq!(normalize_site("eu1.datadoghq.com"), "eu1.datadoghq.com");
    }

    #[test]
    fn test_normalize_site_app_eu1_region() {
        assert_eq!(normalize_site("app.eu1.datadoghq.com"), "eu1.datadoghq.com");
    }

    #[test]
    fn test_normalize_site_ap1_region() {
        assert_eq!(normalize_site("ap1.datadoghq.com"), "ap1.datadoghq.com");
    }

    #[test]
    fn test_normalize_site_oncall_passthrough() {
        assert_eq!(
            normalize_site("navy.oncall.datadoghq.com"),
            "navy.oncall.datadoghq.com"
        );
    }

    // --- normalize_site literal-host passthrough tests ---

    #[test]
    fn test_normalize_site_custom_subdomain_preserved() {
        // Non-canonical host: stored verbatim as a literal.
        assert_eq!(
            normalize_site("customname.datadoghq.com"),
            "customname.datadoghq.com"
        );
    }

    #[test]
    fn test_normalize_site_app_then_custom_subdomain() {
        // Strip leading app. to get the literal host.
        assert_eq!(
            normalize_site("app.customname.datadoghq.com"),
            "customname.datadoghq.com"
        );
    }

    #[test]
    fn test_normalize_site_gateway_host_preserved() {
        assert_eq!(
            normalize_site("mygateway.example.com"),
            "mygateway.example.com"
        );
    }

    #[test]
    fn test_normalize_site_gateway_host_with_port() {
        assert_eq!(
            normalize_site("mygateway.example.com:8443"),
            "mygateway.example.com:8443"
        );
    }

    #[test]
    fn test_normalize_site_strips_https_scheme() {
        assert_eq!(
            normalize_site("https://mygateway.example.com/"),
            "mygateway.example.com"
        );
    }

    // --- is_canonical_site tests ---

    #[test]
    fn test_is_canonical_site_known_sites() {
        for site in crate::config::KNOWN_SITES {
            assert!(is_canonical_site(site), "{site} should be canonical");
        }
    }

    #[test]
    fn test_is_canonical_site_oncall() {
        assert!(is_canonical_site("navy.oncall.datadoghq.com"));
    }

    #[test]
    fn test_is_canonical_site_vanity_is_not_canonical() {
        assert!(!is_canonical_site("mycompany.datadoghq.com"));
    }

    #[test]
    fn test_is_canonical_site_gateway_is_not_canonical() {
        assert!(!is_canonical_site("mygateway.example.com"));
    }

    #[test]
    fn test_uses_datadog_subdomains() {
        // Known canonical sites get api./app. derivation and callback-region trust.
        for site in crate::config::KNOWN_SITES {
            assert!(
                uses_datadog_subdomains(site),
                "{site} should use subdomains"
            );
        }
        // Oncall is canonical for storage but addressed verbatim — excluded here.
        assert!(!uses_datadog_subdomains("navy.oncall.datadoghq.com"));
        // Vanity and gateway hosts are used verbatim.
        assert!(!uses_datadog_subdomains("mycompany.datadoghq.com"));
        assert!(!uses_datadog_subdomains("mygateway.example.com"));
    }

    #[test]
    fn test_is_datadog_owned_host() {
        // Each parent domain and a subdomain of it (incl. vanity SSO subdomains
        // and regional hosts) is owned.
        for host in [
            "datadoghq.com",
            "us3.datadoghq.com",
            "mycompany.datadoghq.com",
            "datadoghq.eu",
            "app.datadoghq.eu",
            "ddog-gov.com",
            "us1.ddog-gov.com",
            "datad0g.com",
            "staging.datad0g.com",
        ] {
            assert!(
                is_datadog_owned_host(host),
                "{host} should be Datadog-owned"
            );
        }
        // Foreign hosts, look-alikes, and smuggling values are not owned.
        // The */#/@ cases ending in a real base are the critical ones: a bare
        // suffix check would accept them (strip_suffix leaves "...."), but the
        // host actually resolves to the attacker domain — validate_site rejects
        // them before the suffix check can.
        for host in [
            "evil.com",
            "datadoghq.com.evil.com",
            "notdatadoghq.com",
            "evil-datadoghq.com",
            "evil.com/path",
            "attacker@evil.com",
            "evil.com/x.datadoghq.com",
            "foo@evil.com/.datadoghq.com",
            "evil.com#x.datadoghq.com",
            "mygateway.example.com",
        ] {
            assert!(
                !is_datadog_owned_host(host),
                "{host} should NOT be Datadog-owned"
            );
        }
    }

    // --- auth_host tests ---

    #[test]
    fn test_auth_host_canonical() {
        let cfg = make_cfg(None, None, Some("t"));
        assert_eq!(cfg.auth_host(), "app.datadoghq.com");
    }

    #[test]
    fn test_auth_host_eu() {
        let mut cfg = make_cfg(None, None, Some("t"));
        cfg.site = "datadoghq.eu".into();
        assert_eq!(cfg.auth_host(), "app.datadoghq.eu");
    }

    #[test]
    fn test_auth_host_literal() {
        let mut cfg = make_cfg(None, None, Some("t"));
        cfg.site = "mycompany.datadoghq.com".into();
        assert_eq!(cfg.auth_host(), "mycompany.datadoghq.com");
    }

    #[test]
    fn test_auth_host_gateway() {
        let mut cfg = make_cfg(None, None, Some("t"));
        cfg.site = "mygateway.example.com".into();
        assert_eq!(cfg.auth_host(), "mygateway.example.com");
    }

    #[test]
    fn test_auth_host_oncall_verbatim() {
        let mut cfg = make_cfg(None, None, Some("t"));
        cfg.site = "navy.oncall.datadoghq.com".into();
        assert_eq!(cfg.auth_host(), "navy.oncall.datadoghq.com");
    }

    // --- api_host literal-host tests ---

    #[test]
    fn test_api_host_literal_vanity() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::remove_var("PUP_MOCK_SERVER");
        let mut cfg = make_cfg(None, None, Some("t"));
        cfg.site = "mycompany.datadoghq.com".into();
        assert_eq!(cfg.api_host(), "mycompany.datadoghq.com");
    }

    #[test]
    fn test_api_host_literal_gateway() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::remove_var("PUP_MOCK_SERVER");
        let mut cfg = make_cfg(None, None, Some("t"));
        cfg.site = "mygateway.example.com:8443".into();
        assert_eq!(cfg.api_host(), "mygateway.example.com:8443");
    }

    // --- validate_site tests ---

    #[test]
    fn test_validate_site_accepts_known_sites() {
        for site in crate::config::KNOWN_SITES {
            validate_site(site).unwrap_or_else(|e| panic!("{site} should be valid: {e}"));
        }
    }

    #[test]
    fn test_validate_site_accepts_regional_and_custom() {
        for s in [
            "mygateway.example.com",
            "mycompany.datadoghq.com",
            "gw.example.com:8443",
            "host-1.example.com",
        ] {
            validate_site(s).unwrap_or_else(|e| panic!("{s:?} should be valid: {e}"));
        }
    }

    #[test]
    fn test_validate_site_rejects_smuggling() {
        for bad in [
            "evil.com/path",
            "a#b",
            "user@host",
            "a b",
            "../../etc",
            "dd_staging",
            "",
        ] {
            let err = validate_site(bad).unwrap_err();
            let msg = format!("{err}");
            assert!(
                msg.contains("invalid characters") || msg.contains("empty") || msg.contains("dot"),
                "{bad:?} should be rejected, got: {msg}"
            );
        }
    }

    #[test]
    fn test_validate_site_rejects_bad_dot_structure() {
        // Character-valid hosts that fail only the dot-structure check (leading,
        // trailing, or consecutive dots) — exercises that branch specifically.
        for bad in ["evil..com", ".datadoghq.com", "datadoghq.com."] {
            let err = validate_site(bad).unwrap_err();
            let msg = format!("{err}");
            assert!(
                msg.contains("dot"),
                "{bad:?} should be rejected for dot structure, got: {msg}"
            );
        }
    }

    #[test]
    fn test_validate_site_rejects_bad_port() {
        assert!(validate_site("host.example.com:0").is_err());
        assert!(validate_site("host.example.com:99999").is_err());
    }

    #[test]
    fn test_normalize_site_www_prefix() {
        assert_eq!(normalize_site("www.datadoghq.com"), "datadoghq.com");
    }

    // --- api_host tests (site already normalized at construction) ---

    #[test]
    fn test_api_host_app_prefix_us1() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::remove_var("PUP_MOCK_SERVER");
        let mut cfg = make_cfg(None, None, Some("t"));
        cfg.site = normalize_site("app.datadoghq.com");
        assert_eq!(cfg.api_host(), "api.datadoghq.com");
    }

    #[test]
    fn test_api_host_app_prefix_eu() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::remove_var("PUP_MOCK_SERVER");
        let mut cfg = make_cfg(None, None, Some("t"));
        cfg.site = normalize_site("app.datadoghq.eu");
        assert_eq!(cfg.api_host(), "api.datadoghq.eu");
    }

    #[test]
    fn test_api_host_app_prefix_gov() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::remove_var("PUP_MOCK_SERVER");
        let mut cfg = make_cfg(None, None, Some("t"));
        cfg.site = normalize_site("app.ddog-gov.com");
        assert_eq!(cfg.api_host(), "api.ddog-gov.com");
    }

    #[test]
    fn test_api_host_app_prefix_staging() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::remove_var("PUP_MOCK_SERVER");
        let mut cfg = make_cfg(None, None, Some("t"));
        cfg.site = normalize_site("app.datad0g.com");
        assert_eq!(cfg.api_host(), "api.datad0g.com");
    }

    #[test]
    fn test_api_host_region_us3() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::remove_var("PUP_MOCK_SERVER");
        let mut cfg = make_cfg(None, None, Some("t"));
        cfg.site = normalize_site("us3.datadoghq.com");
        assert_eq!(cfg.api_host(), "api.us3.datadoghq.com");
    }

    #[test]
    fn test_api_host_app_region_us3() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::remove_var("PUP_MOCK_SERVER");
        let mut cfg = make_cfg(None, None, Some("t"));
        cfg.site = normalize_site("app.us3.datadoghq.com");
        assert_eq!(cfg.api_host(), "api.us3.datadoghq.com");
    }

    #[test]
    fn test_env_or_with_fallback() {
        assert_eq!(
            env_or("__PUP_TEST_NONEXISTENT__", Some("fallback".into())),
            Some("fallback".into())
        );
    }

    #[test]
    fn test_env_or_no_fallback() {
        assert_eq!(env_or("__PUP_TEST_NONEXISTENT__", None), None);
    }

    #[test]
    fn test_api_base_url_standard() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::remove_var("PUP_MOCK_SERVER");
        let cfg = make_cfg(None, None, Some("t"));
        assert_eq!(cfg.api_base_url(), "https://api.datadoghq.com");
    }

    #[test]
    fn test_api_base_url_eu() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::remove_var("PUP_MOCK_SERVER");
        let mut cfg = make_cfg(None, None, Some("t"));
        cfg.site = "datadoghq.eu".into();
        assert_eq!(cfg.api_base_url(), "https://api.datadoghq.eu");
    }

    #[test]
    fn test_api_base_url_oncall() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::remove_var("PUP_MOCK_SERVER");
        let mut cfg = make_cfg(None, None, Some("t"));
        cfg.site = "navy.oncall.datadoghq.com".into();
        assert_eq!(cfg.api_base_url(), "https://navy.oncall.datadoghq.com");
    }

    #[test]
    fn test_api_base_url_mock_server() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::set_var("PUP_MOCK_SERVER", "http://127.0.0.1:1234");
        let cfg = make_cfg(None, None, Some("t"));
        assert_eq!(cfg.api_base_url(), "http://127.0.0.1:1234");
        std::env::remove_var("PUP_MOCK_SERVER");
    }

    #[test]
    fn test_api_host_mock_server() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::set_var("PUP_MOCK_SERVER", "http://127.0.0.1:5678");
        let cfg = make_cfg(None, None, Some("t"));
        assert_eq!(cfg.api_host(), "127.0.0.1:5678");
        std::env::remove_var("PUP_MOCK_SERVER");
    }

    #[test]
    fn test_env_bool_true() {
        std::env::set_var("__PUP_TEST_BOOL_TRUE__", "true");
        assert!(env_bool("__PUP_TEST_BOOL_TRUE__"));
        std::env::remove_var("__PUP_TEST_BOOL_TRUE__");
    }

    #[test]
    fn test_env_bool_one() {
        std::env::set_var("__PUP_TEST_BOOL_ONE__", "1");
        assert!(env_bool("__PUP_TEST_BOOL_ONE__"));
        std::env::remove_var("__PUP_TEST_BOOL_ONE__");
    }

    #[test]
    fn test_env_bool_false() {
        std::env::set_var("__PUP_TEST_BOOL_FALSE__", "false");
        assert!(!env_bool("__PUP_TEST_BOOL_FALSE__"));
        std::env::remove_var("__PUP_TEST_BOOL_FALSE__");
    }

    #[test]
    fn test_env_bool_missing() {
        assert!(!env_bool("__PUP_TEST_BOOL_MISSING__"));
    }

    #[test]
    fn test_config_dir_returns_path() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::remove_var("PUP_CONFIG_DIR");
        let dir = config_dir();
        // On native builds, dirs::config_dir() should return Some
        assert!(dir.is_some());
        assert!(dir.unwrap().ends_with("pup"));
    }

    #[test]
    fn test_config_dir_respects_override() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::set_var("PUP_CONFIG_DIR", "/tmp/pup_test_override");
        let dir = config_dir();
        std::env::remove_var("PUP_CONFIG_DIR");
        assert_eq!(
            dir,
            Some(std::path::PathBuf::from("/tmp/pup_test_override"))
        );
    }

    // --- config_file_candidates tests ---

    #[test]
    fn test_config_file_candidates_includes_primary() {
        let _guard = ENV_LOCK.blocking_lock();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let tmp = std::env::temp_dir().join(format!("pup_candidates_primary_{nanos}"));
        std::env::set_var("PUP_CONFIG_DIR", &tmp);
        let candidates = config_file_candidates();
        std::env::remove_var("PUP_CONFIG_DIR");
        assert!(!candidates.is_empty());
        assert_eq!(candidates[0], tmp.join("config.yaml"));
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_config_file_candidates_macos_includes_xdg_fallback() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::remove_var("PUP_CONFIG_DIR");
        let candidates = config_file_candidates();
        let xdg = dirs::home_dir().unwrap().join(".config/pup/config.yaml");
        assert!(
            candidates.contains(&xdg),
            "XDG fallback should be in candidates on macOS: {candidates:?}"
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_config_file_candidates_macos_pup_config_dir_suppresses_xdg() {
        let _guard = ENV_LOCK.blocking_lock();
        let tmp = std::env::temp_dir().join("pup_candidates_suppress_xdg");
        std::env::set_var("PUP_CONFIG_DIR", &tmp);
        let candidates = config_file_candidates();
        std::env::remove_var("PUP_CONFIG_DIR");
        let xdg = dirs::home_dir().unwrap().join(".config/pup/config.yaml");
        assert_eq!(
            candidates.len(),
            1,
            "only primary when PUP_CONFIG_DIR is set"
        );
        assert!(!candidates.contains(&xdg));
    }

    #[test]
    fn test_load_config_from_primary_path() {
        let _guard = ENV_LOCK.blocking_lock();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let tmp = std::env::temp_dir().join(format!("pup_cfg_primary_{nanos}"));
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("config.yaml"), "site: us5.datadoghq.com\n").unwrap();
        std::env::set_var("PUP_CONFIG_DIR", &tmp);
        std::env::remove_var("DD_SITE");
        std::env::remove_var("DD_ACCESS_TOKEN");
        std::env::remove_var("DD_API_KEY");
        std::env::remove_var("DD_APP_KEY");
        std::env::remove_var("DD_ORG");
        let cfg = Config::from_env().unwrap();
        std::env::remove_var("PUP_CONFIG_DIR");
        let _ = std::fs::remove_dir_all(&tmp);
        assert_eq!(cfg.site, "us5.datadoghq.com");
        assert!(cfg.site_explicit);
    }

    #[test]
    fn test_env_or_with_env_value() {
        std::env::set_var("__PUP_TEST_ENV_OR__", "env-value");
        assert_eq!(
            env_or("__PUP_TEST_ENV_OR__", Some("fallback".into())),
            Some("env-value".into())
        );
        std::env::remove_var("__PUP_TEST_ENV_OR__");
    }

    #[test]
    fn test_env_or_empty_env_uses_fallback() {
        std::env::set_var("__PUP_TEST_ENV_EMPTY__", "");
        assert_eq!(
            env_or("__PUP_TEST_ENV_EMPTY__", Some("fallback".into())),
            Some("fallback".into())
        );
        std::env::remove_var("__PUP_TEST_ENV_EMPTY__");
    }

    #[test]
    fn test_file_config_read_only() {
        let yaml = "read_only: true\n";
        let fc: FileConfig = serde_norway::from_str(yaml).unwrap();
        assert_eq!(fc.read_only, Some(true));
    }

    #[test]
    fn test_read_only_from_env() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::remove_var("DD_READ_ONLY");
        std::env::remove_var("DD_CLI_READ_ONLY");
        std::env::set_var("PUP_CONFIG_DIR", "/tmp/pup_test_nonexistent");
        std::env::set_var("DD_ACCESS_TOKEN", "test");

        let cfg = Config::from_env().unwrap();
        assert!(!cfg.read_only);

        std::env::set_var("DD_READ_ONLY", "true");
        let cfg = Config::from_env().unwrap();
        assert!(cfg.read_only);
        std::env::remove_var("DD_READ_ONLY");

        std::env::set_var("DD_CLI_READ_ONLY", "1");
        let cfg = Config::from_env().unwrap();
        assert!(cfg.read_only);
        std::env::remove_var("DD_CLI_READ_ONLY");

        std::env::remove_var("DD_ACCESS_TOKEN");
        std::env::remove_var("PUP_CONFIG_DIR");
    }

    /// PUP_* variables are what pup injects into extension subprocesses, so a
    /// child `pup` invocation must inherit the parent's output format and mode.
    #[test]
    fn test_pup_env_inherited_by_child() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::set_var("PUP_CONFIG_DIR", "/tmp/pup_test_nonexistent");
        std::env::set_var("DD_ACCESS_TOKEN", "test");
        for var in [
            "DD_OUTPUT",
            "PUP_OUTPUT",
            "DD_READ_ONLY",
            "DD_CLI_READ_ONLY",
            "PUP_READ_ONLY",
            "DD_AUTO_APPROVE",
            "DD_CLI_AUTO_APPROVE",
            "PUP_AUTO_APPROVE",
            "PUP_FILTER",
        ] {
            std::env::remove_var(var);
        }

        // PUP_OUTPUT drives the format when DD_OUTPUT is unset.
        std::env::set_var("PUP_OUTPUT", "table");
        let cfg = Config::from_env().unwrap();
        assert_eq!(cfg.output_format, OutputFormat::Table);

        // DD_OUTPUT wins over PUP_OUTPUT when both are set.
        std::env::set_var("DD_OUTPUT", "yaml");
        let cfg = Config::from_env().unwrap();
        assert_eq!(cfg.output_format, OutputFormat::Yaml);
        std::env::remove_var("DD_OUTPUT");
        std::env::remove_var("PUP_OUTPUT");

        // PUP_FILTER drives cfg.jq so a nested `pup` call spawned by an extension
        // (e.g. `pup api` invoked from inside a `pup-<ext>` process) inherits the
        // outer --jq expression, the same way it inherits --output.
        assert_eq!(Config::from_env().unwrap().jq, None);
        std::env::set_var("PUP_FILTER", ".[] | .id");
        let cfg = Config::from_env().unwrap();
        assert_eq!(cfg.jq.as_deref(), Some(".[] | .id"));
        std::env::remove_var("PUP_FILTER");

        // PUP_READ_ONLY / PUP_AUTO_APPROVE flip the mode flags.
        std::env::set_var("PUP_READ_ONLY", "true");
        std::env::set_var("PUP_AUTO_APPROVE", "true");
        let cfg = Config::from_env().unwrap();
        assert!(cfg.read_only);
        assert!(cfg.auto_approve);

        std::env::remove_var("PUP_READ_ONLY");
        std::env::remove_var("PUP_AUTO_APPROVE");
        std::env::remove_var("DD_ACCESS_TOKEN");
        std::env::remove_var("PUP_CONFIG_DIR");
    }

    /// Per-org session sites: when DD_ORG is set and DD_SITE is not, the saved
    /// session's site should win over the default. site_explicit must remain
    /// false so subsequent --org overrides can still adjust it.
    #[test]
    fn test_from_env_picks_up_session_site_when_org_set() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::remove_var("DD_SITE");
        std::env::remove_var("DD_ACCESS_TOKEN");
        std::env::remove_var("DD_API_KEY");
        std::env::remove_var("DD_APP_KEY");

        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let tmp = std::env::temp_dir().join(format!("pup_cfg_org_site_{nanos}"));
        std::fs::create_dir_all(&tmp).unwrap();
        std::env::set_var("PUP_CONFIG_DIR", &tmp);

        crate::auth::storage::save_session(&SessionEntry {
            site: "custom.datadoghq.com".into(),
            org: Some("prod-child".into()),
            org_uuid: None,
        })
        .unwrap();
        std::env::set_var("DD_ORG", "prod-child");

        let cfg = Config::from_env().unwrap();

        std::env::remove_var("DD_ORG");
        std::env::remove_var("PUP_CONFIG_DIR");
        let _ = std::fs::remove_dir_all(&tmp);

        assert_eq!(cfg.site, "custom.datadoghq.com");
        assert!(!cfg.site_explicit);
    }

    /// An explicit DD_SITE always wins over a saved session site, even when
    /// DD_ORG points at a registered session on a different site.
    #[test]
    fn test_from_env_explicit_site_overrides_session_site() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::remove_var("DD_ACCESS_TOKEN");
        std::env::remove_var("DD_API_KEY");
        std::env::remove_var("DD_APP_KEY");

        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let tmp = std::env::temp_dir().join(format!("pup_cfg_explicit_site_{nanos}"));
        std::fs::create_dir_all(&tmp).unwrap();
        std::env::set_var("PUP_CONFIG_DIR", &tmp);

        crate::auth::storage::save_session(&SessionEntry {
            site: "custom.datadoghq.com".into(),
            org: Some("prod-child".into()),
            org_uuid: None,
        })
        .unwrap();
        std::env::set_var("DD_ORG", "prod-child");
        std::env::set_var("DD_SITE", "datadoghq.eu");

        let cfg = Config::from_env().unwrap();

        std::env::remove_var("DD_ORG");
        std::env::remove_var("DD_SITE");
        std::env::remove_var("PUP_CONFIG_DIR");
        let _ = std::fs::remove_dir_all(&tmp);

        assert_eq!(cfg.site, "datadoghq.eu");
        assert!(cfg.site_explicit);
    }

    /// `--org` re-pins the site when the previous value was session-derived
    /// (the from_env-loaded org's site sticks otherwise, leading to
    /// orgB-token-on-orgA-site requests).
    #[test]
    fn test_apply_org_override_switches_site_for_new_org() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::remove_var("DD_ACCESS_TOKEN");

        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let tmp = std::env::temp_dir().join(format!("pup_cfg_apply_override_{nanos}"));
        std::fs::create_dir_all(&tmp).unwrap();
        std::env::set_var("PUP_CONFIG_DIR", &tmp);

        crate::auth::storage::save_session(&SessionEntry {
            site: "a.datadoghq.com".into(),
            org: Some("org-a".into()),
            org_uuid: None,
        })
        .unwrap();
        crate::auth::storage::save_session(&SessionEntry {
            site: "b.datadoghq.com".into(),
            org: Some("org-b".into()),
            org_uuid: None,
        })
        .unwrap();

        // Simulate the post-from_env state where we resolved org-a's site via
        // the registry (site_explicit=false because the user did not set DD_SITE).
        let mut cfg = Config {
            api_key: None,
            app_key: None,
            access_token: None,
            site: "a.datadoghq.com".into(),
            site_explicit: false,
            org: Some("org-a".into()),
            output_format: OutputFormat::Json,
            auto_approve: false,
            agent_mode: false,
            read_only: false,
            jq: None,
        };

        super::apply_org_override(&mut cfg, "org-b".into()).unwrap();

        std::env::remove_var("PUP_CONFIG_DIR");
        let _ = std::fs::remove_dir_all(&tmp);

        assert_eq!(cfg.org.as_deref(), Some("org-b"));
        assert_eq!(cfg.site, "b.datadoghq.com");
    }

    /// `--org` must not move the site off whatever the user explicitly pinned
    /// (DD_SITE / --site / config file).
    #[test]
    fn test_apply_org_override_respects_explicit_site() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::remove_var("DD_ACCESS_TOKEN");

        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let tmp = std::env::temp_dir().join(format!("pup_cfg_apply_explicit_{nanos}"));
        std::fs::create_dir_all(&tmp).unwrap();
        std::env::set_var("PUP_CONFIG_DIR", &tmp);

        crate::auth::storage::save_session(&SessionEntry {
            site: "session.datadoghq.com".into(),
            org: Some("org-a".into()),
            org_uuid: None,
        })
        .unwrap();

        let mut cfg = Config {
            api_key: None,
            app_key: None,
            access_token: None,
            site: "explicit.datadoghq.com".into(),
            site_explicit: true,
            org: None,
            output_format: OutputFormat::Json,
            auto_approve: false,
            agent_mode: false,
            read_only: false,
            jq: None,
        };

        super::apply_org_override(&mut cfg, "org-a".into()).unwrap();

        std::env::remove_var("PUP_CONFIG_DIR");
        let _ = std::fs::remove_dir_all(&tmp);

        assert_eq!(cfg.org.as_deref(), Some("org-a"));
        assert_eq!(cfg.site, "explicit.datadoghq.com");
    }

    /// `--org` for an org that has no saved session must not invent one — site
    /// stays where it was on entry.
    #[test]
    fn test_apply_org_override_leaves_site_when_no_session() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::remove_var("DD_ACCESS_TOKEN");

        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let tmp = std::env::temp_dir().join(format!("pup_cfg_apply_no_session_{nanos}"));
        std::fs::create_dir_all(&tmp).unwrap();
        std::env::set_var("PUP_CONFIG_DIR", &tmp);

        let mut cfg = Config {
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
            jq: None,
        };

        super::apply_org_override(&mut cfg, "unknown-org".into()).unwrap();

        std::env::remove_var("PUP_CONFIG_DIR");
        let _ = std::fs::remove_dir_all(&tmp);

        assert_eq!(cfg.org.as_deref(), Some("unknown-org"));
        assert_eq!(cfg.site, "datadoghq.com");
    }

    /// A `--org` flag for an org with no session must reset the site to the
    /// default rather than inherit the no-org default-session site that
    /// `from_env` adopted. Without this, `pup --org new-org ...` (or a
    /// first-time `auth login --org new-org`) with an existing datadoghq.eu
    /// default session would misroute to EU. Regression guard for the Codex
    /// review finding on the #592 fix.
    #[test]
    fn test_apply_org_override_resets_default_session_site_for_unknown_org() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::remove_var("DD_ACCESS_TOKEN");

        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let tmp = std::env::temp_dir().join(format!("pup_cfg_apply_reset_{nanos}"));
        std::fs::create_dir_all(&tmp).unwrap();
        std::env::set_var("PUP_CONFIG_DIR", &tmp);

        // Simulate from_env having adopted the no-org default session's site.
        let mut cfg = Config {
            api_key: None,
            app_key: None,
            access_token: None,
            site: "datadoghq.eu".into(),
            site_explicit: false,
            org: None,
            output_format: OutputFormat::Json,
            auto_approve: false,
            agent_mode: false,
            read_only: false,
            jq: None,
        };

        super::apply_org_override(&mut cfg, "unknown-org".into()).unwrap();

        std::env::remove_var("PUP_CONFIG_DIR");
        let _ = std::fs::remove_dir_all(&tmp);

        assert_eq!(cfg.org.as_deref(), Some("unknown-org"));
        // Reset to default, NOT left at the inherited datadoghq.eu.
        assert_eq!(cfg.site, "datadoghq.com");
    }

    /// When DD_ACCESS_TOKEN is set in the env, `--org` must not overwrite the
    /// caller-supplied bearer with whatever happens to be in keychain storage.
    #[test]
    fn test_apply_org_override_respects_env_access_token() {
        let _guard = ENV_LOCK.blocking_lock();

        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let tmp = std::env::temp_dir().join(format!("pup_cfg_apply_envtoken_{nanos}"));
        std::fs::create_dir_all(&tmp).unwrap();
        std::env::set_var("PUP_CONFIG_DIR", &tmp);
        std::env::set_var("DD_ACCESS_TOKEN", "env-supplied-token");

        let mut cfg = Config {
            api_key: None,
            app_key: None,
            access_token: Some("env-supplied-token".into()),
            site: "datadoghq.com".into(),
            site_explicit: false,
            org: None,
            output_format: OutputFormat::Json,
            auto_approve: false,
            agent_mode: false,
            read_only: false,
            jq: None,
        };

        super::apply_org_override(&mut cfg, "any-org".into()).unwrap();

        std::env::remove_var("DD_ACCESS_TOKEN");
        std::env::remove_var("PUP_CONFIG_DIR");
        let _ = std::fs::remove_dir_all(&tmp);

        assert_eq!(cfg.access_token.as_deref(), Some("env-supplied-token"));
    }

    /// An invalid session site must cause `apply_org_override` to bail rather
    /// than silently leaving `cfg.site` at its pre-override value and routing
    /// to the wrong endpoint. Consistent with `from_env` which also bails.
    #[test]
    fn test_apply_org_override_rejects_invalid_session_site() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::remove_var("DD_ACCESS_TOKEN");

        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let tmp = std::env::temp_dir().join(format!("pup_cfg_invalid_site_{nanos}"));
        std::fs::create_dir_all(&tmp).unwrap();
        std::env::set_var("PUP_CONFIG_DIR", &tmp);

        crate::auth::storage::save_session(&SessionEntry {
            // Deliberately malformed — simulate a tampered sessions file.
            site: "evil.com/path".into(),
            org: Some("bad-org".into()),
            org_uuid: None,
        })
        .unwrap();

        let mut cfg = Config {
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
            jq: None,
        };

        let result = super::apply_org_override(&mut cfg, "bad-org".into());

        std::env::remove_var("PUP_CONFIG_DIR");
        let _ = std::fs::remove_dir_all(&tmp);

        assert!(
            result.is_err(),
            "apply_org_override must bail on an invalid session site"
        );
        // cfg.site must not have been updated to the invalid value.
        assert_eq!(cfg.site, "datadoghq.com");
    }

    /// `set_site_explicit` keeps `site` and `site_explicit` in lockstep so a
    /// later `--org` lookup cannot silently overwrite a user-pinned site.
    #[test]
    fn test_set_site_explicit_marks_site_as_user_provided() {
        let mut cfg = Config {
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
            jq: None,
        };

        cfg.set_site_explicit("app.datadoghq.eu".into()).unwrap();

        assert_eq!(cfg.site, "datadoghq.eu");
        assert!(cfg.site_explicit);
    }

    #[test]
    fn test_set_site_explicit_rejects_smuggling_value() {
        let mut cfg = Config {
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
            jq: None,
        };
        // site and site_explicit must remain unchanged on failure.
        assert!(cfg.set_site_explicit("evil.com/path".into()).is_err());
        assert_eq!(cfg.site, "datadoghq.com");
        assert!(!cfg.site_explicit);
    }

    #[test]
    fn test_set_site_explicit_rejects_empty_string() {
        let mut cfg = Config {
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
            jq: None,
        };
        // An empty --site must not silently route to datadoghq.com via
        // normalize_site's empty-string fallback.
        assert!(cfg.set_site_explicit("".into()).is_err());
        assert_eq!(cfg.site, "datadoghq.com");
        assert!(!cfg.site_explicit);
    }

    /// With no org, no env site, and no session, we fall back to the default
    /// site and report site_explicit=false.
    #[test]
    fn test_from_env_default_site_when_no_org() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::remove_var("DD_SITE");
        std::env::remove_var("DD_ORG");
        std::env::remove_var("DD_ACCESS_TOKEN");
        std::env::remove_var("DD_API_KEY");
        std::env::remove_var("DD_APP_KEY");
        std::env::set_var("PUP_CONFIG_DIR", "/tmp/pup_test_nonexistent");

        let cfg = Config::from_env().unwrap();

        std::env::remove_var("PUP_CONFIG_DIR");

        assert_eq!(cfg.site, "datadoghq.com");
        assert!(!cfg.site_explicit);
    }

    /// Whitespace-only DD_SITE passes env_or's `!is_empty()` filter but must be
    /// rejected with an error. Without this check, normalize_site would silently
    /// reduce it to "datadoghq.com" and set site_explicit=true, blocking --org from
    /// correcting the site and routing commands to the wrong endpoint.
    #[test]
    fn test_from_env_rejects_whitespace_only_dd_site() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::set_var("DD_SITE", "   ");
        std::env::set_var("PUP_CONFIG_DIR", "/tmp/pup_test_nonexistent");
        let result = Config::from_env();
        std::env::remove_var("DD_SITE");
        std::env::remove_var("PUP_CONFIG_DIR");
        let err_msg = result.err().map(|e| e.to_string());
        assert!(
            err_msg
                .as_deref()
                .is_some_and(|m| m.contains("empty") || m.contains("whitespace")),
            "expected error for whitespace-only DD_SITE, got: {err_msg:?}"
        );
    }

    #[test]
    fn test_from_env_rejects_invalid_dd_site() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::set_var("DD_SITE", "evil.com/path");
        std::env::set_var("PUP_CONFIG_DIR", "/tmp/pup_test_nonexistent");
        let result = Config::from_env();
        std::env::remove_var("DD_SITE");
        std::env::remove_var("PUP_CONFIG_DIR");
        let err_msg = result.err().map(|e| e.to_string());
        assert!(
            err_msg
                .as_deref()
                .is_some_and(|m| m.contains("invalid characters")),
            "expected 'invalid characters' error from from_env with bad DD_SITE, got: {err_msg:?}"
        );
    }

    #[test]
    fn test_parse_scopes_basic() {
        assert_eq!(
            parse_scopes("dashboards_read,metrics_read"),
            vec!["dashboards_read", "metrics_read"]
        );
    }

    #[test]
    fn test_parse_scopes_with_spaces() {
        assert_eq!(
            parse_scopes(" dashboards_read , metrics_read "),
            vec!["dashboards_read", "metrics_read"]
        );
    }

    #[test]
    fn test_parse_scopes_empty() {
        assert!(parse_scopes("").is_empty());
        assert!(parse_scopes("  ").is_empty());
    }

    #[test]
    fn test_parse_scopes_single() {
        assert_eq!(parse_scopes("org_management"), vec!["org_management"]);
    }

    #[test]
    fn test_file_config_profiles_scopes() {
        let yaml = r#"
profiles:
  my-org:
    scopes: teams_manage,org_management
  read-only-org:
    scopes: dashboards_read,metrics_read
"#;
        let fc: FileConfig = serde_norway::from_str(yaml).unwrap();
        let profiles = fc.profiles.unwrap();
        assert_eq!(
            profiles["my-org"].scopes.as_deref(),
            Some("teams_manage,org_management")
        );
        assert_eq!(
            profiles["read-only-org"].scopes.as_deref(),
            Some("dashboards_read,metrics_read")
        );
    }

    #[test]
    fn test_file_config_top_level_scopes() {
        let yaml = "scopes: dashboards_read,monitors_read\n";
        let fc: FileConfig = serde_norway::from_str(yaml).unwrap();
        assert_eq!(fc.scopes.as_deref(), Some("dashboards_read,monitors_read"));
    }

    // --- resolve_token (auto-refresh logic) ---------------------------------

    use crate::auth::types::{ClientCredentials, TokenSet};

    fn make_token_set(issued_ago_secs: i64, expires_in: i64, refresh: &str) -> TokenSet {
        TokenSet {
            access_token: "old-access-token".into(),
            refresh_token: refresh.into(),
            token_type: "Bearer".into(),
            expires_in,
            issued_at: chrono::Utc::now().timestamp() - issued_ago_secs,
            scope: String::new(),
            client_id: String::new(),
        }
    }

    fn make_creds() -> ClientCredentials {
        ClientCredentials {
            client_id: "test-client-id".into(),
            client_name: "test-client".into(),
            redirect_uris: vec![],
            registered_at: 0,
            site: "datadoghq.com".into(),
        }
    }

    fn make_refreshed_token_set() -> TokenSet {
        TokenSet {
            access_token: "fresh-access-token".into(),
            refresh_token: "fresh-refresh-token".into(),
            token_type: "Bearer".into(),
            expires_in: 3600,
            issued_at: chrono::Utc::now().timestamp(),
            scope: String::new(),
            client_id: "test-client-id".into(),
        }
    }

    #[test]
    fn test_resolve_token_valid_token() {
        let tokens = make_token_set(0, 3600, "refresh");
        let creds = make_creds();
        let result = super::resolve_token(tokens, Some(&creds), |_, _| {
            panic!("refresh_fn should not be called for valid token");
        });
        match result {
            super::ResolvedToken::Valid(t) => assert_eq!(t, "old-access-token"),
            _ => panic!("expected Valid"),
        }
    }

    #[test]
    fn test_resolve_token_expired_no_refresh_token() {
        let tokens = make_token_set(7200, 3600, "");
        let creds = make_creds();
        let result = super::resolve_token(tokens, Some(&creds), |_, _| {
            panic!("refresh_fn should not be called without refresh token");
        });
        assert!(matches!(result, super::ResolvedToken::Expired));
    }

    #[test]
    fn test_resolve_token_expired_no_client_creds() {
        let tokens = make_token_set(7200, 3600, "refresh");
        let result = super::resolve_token(tokens, None, |_, _| {
            panic!("refresh_fn should not be called without client credentials");
        });
        assert!(matches!(result, super::ResolvedToken::Expired));
    }

    #[test]
    fn test_resolve_token_expired_refresh_fails() {
        let tokens = make_token_set(7200, 3600, "refresh");
        let creds = make_creds();
        let result = super::resolve_token(tokens, Some(&creds), |_, _| None);
        assert!(matches!(result, super::ResolvedToken::Expired));
    }

    #[test]
    fn test_resolve_token_expired_refresh_succeeds() {
        let tokens = make_token_set(7200, 3600, "refresh");
        let creds = make_creds();
        let result = super::resolve_token(tokens, Some(&creds), |rt, c| {
            assert_eq!(rt, "refresh");
            assert_eq!(c.client_id, "test-client-id");
            Some(make_refreshed_token_set())
        });
        match result {
            super::ResolvedToken::Refreshed(t) => {
                assert_eq!(t.access_token, "fresh-access-token");
                assert_eq!(t.refresh_token, "fresh-refresh-token");
            }
            _ => panic!("expected Refreshed"),
        }
    }

    #[test]
    fn test_resolve_token_near_expiry_triggers_refresh() {
        let tokens = make_token_set(3400, 3600, "refresh"); // 200s left < 300s buffer
        let creds = make_creds();
        let result =
            super::resolve_token(
                tokens,
                Some(&creds),
                |_, _| Some(make_refreshed_token_set()),
            );
        assert!(matches!(result, super::ResolvedToken::Refreshed(_)));
    }

    // --- ensure_site_trusted tests ---

    /// Datadog-owned hosts are always trusted: no flag, no env, no config needed.
    #[test]
    fn test_ensure_site_trusted_datadog_owned_hosts_always_ok() {
        for site in [
            "datadoghq.com",
            "us3.datadoghq.com",
            "mycompany.datadoghq.com",
            "navy.oncall.datadoghq.com",
            "datadoghq.eu",
            "ddog-gov.com",
            "datad0g.com",
        ] {
            let mut cfg = make_cfg(None, None, None);
            cfg.site = site.into();
            assert!(
                cfg.ensure_site_trusted(false, false, &[]).is_ok(),
                "{site} should always be trusted"
            );
        }
    }

    /// --trust-site flag on the invocation overrides the prompt for a foreign host.
    #[test]
    fn test_ensure_site_trusted_trust_site_flag_ok() {
        let mut cfg = make_cfg(None, None, None);
        cfg.site = "datadog-proxy.weyland-yutani.internal".into();
        assert!(cfg.ensure_site_trusted(true, false, &[]).is_ok());
    }

    /// A site listed in trusted_sites (after normalization) is trusted without a flag.
    #[test]
    fn test_ensure_site_trusted_trusted_sites_config_ok() {
        let mut cfg = make_cfg(None, None, None);
        cfg.site = normalize_site("datadog-proxy.weyland-yutani.internal");
        let trusted = vec!["datadog-proxy.weyland-yutani.internal".into()];
        assert!(cfg.ensure_site_trusted(false, false, &trusted).is_ok());
    }

    /// A trusted_sites entry is normalized before comparison, so an entry stored
    /// with a scheme/prefix still matches the bare stored site.
    #[test]
    fn test_ensure_site_trusted_trusted_sites_normalized_entry_ok() {
        let mut cfg = make_cfg(None, None, None);
        cfg.site = "myproxy.example.com".into();
        // Entry stored with https:// scheme — normalize_site strips it.
        let trusted = vec!["https://myproxy.example.com/".into()];
        assert!(cfg.ensure_site_trusted(false, false, &trusted).is_ok());
    }

    /// PUP_TRUST_SITE=1 in the environment trusts a foreign host non-interactively.
    #[test]
    fn test_ensure_site_trusted_env_var_ok() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::set_var("PUP_TRUST_SITE", "1");

        let mut cfg = make_cfg(None, None, None);
        cfg.site = "datadog-proxy.weyland-yutani.internal".into();
        let result = cfg.ensure_site_trusted(false, false, &[]);

        std::env::remove_var("PUP_TRUST_SITE");
        assert!(result.is_ok());
    }

    /// Foreign host + no opt-in + non-interactive → fail closed, message names remediations.
    #[test]
    fn test_ensure_site_trusted_foreign_host_no_opt_in_fails_closed() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::remove_var("PUP_TRUST_SITE");

        let mut cfg = make_cfg(None, None, None);
        cfg.site = "datadog-proxy.weyland-yutani.internal".into();

        let err = cfg
            .ensure_site_trusted(false, false, &[])
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("trusted_sites"),
            "error should mention trusted_sites: {err}"
        );
        assert!(
            err.contains("--trust-site"),
            "error should mention --trust-site: {err}"
        );
        assert!(
            err.contains("PUP_TRUST_SITE"),
            "error should mention PUP_TRUST_SITE: {err}"
        );
    }

    /// The non-interactive predicate (used to gate the implicit token refresh in
    /// `load_token_from_storage`) trusts owned hosts and config entries, and
    /// rejects a foreign host with no opt-in.
    #[test]
    fn test_site_trusted_without_prompt() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::remove_var("PUP_TRUST_SITE");

        // Datadog-owned: trusted with no opt-in.
        assert!(super::site_trusted_without_prompt("us3.datadoghq.com", &[]));
        assert!(super::site_trusted_without_prompt(
            "mycompany.datadoghq.com",
            &[]
        ));
        // Foreign host, no opt-in: not trusted (would otherwise refresh silently).
        assert!(!super::site_trusted_without_prompt(
            "datadog-proxy.weyland-yutani.internal",
            &[]
        ));
        // Foreign host listed in trusted_sites (normalized): trusted.
        let trusted = vec!["https://datadog-proxy.weyland-yutani.internal/".into()];
        assert!(super::site_trusted_without_prompt(
            "datadog-proxy.weyland-yutani.internal",
            &trusted
        ));

        // Foreign host trusted via env.
        std::env::set_var("PUP_TRUST_SITE", "1");
        let env_ok =
            super::site_trusted_without_prompt("datadog-proxy.weyland-yutani.internal", &[]);
        std::env::remove_var("PUP_TRUST_SITE");
        assert!(env_ok);
    }

    // --- from_env: default-session fallback (pup#592) ---------------------------

    /// Bare invocation with a no-org EU session resolves to datadoghq.eu, not
    /// datadoghq.com. This is the core #592 regression test.
    #[test]
    fn test_from_env_bare_uses_default_session_site() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::remove_var("DD_SITE");
        std::env::remove_var("DD_ORG");
        std::env::remove_var("DD_ACCESS_TOKEN");
        std::env::remove_var("DD_API_KEY");
        std::env::remove_var("DD_APP_KEY");

        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let tmp = std::env::temp_dir().join(format!("pup_cfg_bare_eu_{nanos}"));
        std::fs::create_dir_all(&tmp).unwrap();
        std::env::set_var("PUP_CONFIG_DIR", &tmp);

        crate::auth::storage::save_session(&SessionEntry {
            site: "datadoghq.eu".into(),
            org: None,
            org_uuid: None,
        })
        .unwrap();

        let cfg = Config::from_env().unwrap();

        std::env::remove_var("PUP_CONFIG_DIR");
        let _ = std::fs::remove_dir_all(&tmp);

        assert_eq!(cfg.site, "datadoghq.eu");
        // site_explicit must remain false — the source was a session, not DD_SITE.
        assert!(!cfg.site_explicit);
    }

    /// When DD_SITE is set explicitly it always wins over the default-session
    /// fallback.
    #[test]
    fn test_from_env_explicit_dd_site_wins_over_default_session() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::remove_var("DD_ORG");
        std::env::remove_var("DD_ACCESS_TOKEN");
        std::env::remove_var("DD_API_KEY");
        std::env::remove_var("DD_APP_KEY");

        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let tmp = std::env::temp_dir().join(format!("pup_cfg_ddsite_wins_{nanos}"));
        std::fs::create_dir_all(&tmp).unwrap();
        std::env::set_var("PUP_CONFIG_DIR", &tmp);

        crate::auth::storage::save_session(&SessionEntry {
            site: "datadoghq.eu".into(),
            org: None,
            org_uuid: None,
        })
        .unwrap();
        std::env::set_var("DD_SITE", "us3.datadoghq.com");

        let cfg = Config::from_env().unwrap();

        std::env::remove_var("DD_SITE");
        std::env::remove_var("PUP_CONFIG_DIR");
        let _ = std::fs::remove_dir_all(&tmp);

        assert_eq!(cfg.site, "us3.datadoghq.com");
        assert!(cfg.site_explicit);
    }

    /// When DD_ORG is set the no-org fallback must not fire — the org-session
    /// resolution path handles site lookup instead.
    #[test]
    fn test_from_env_named_org_skips_default_session_fallback() {
        let _guard = ENV_LOCK.blocking_lock();
        std::env::remove_var("DD_SITE");
        std::env::remove_var("DD_ACCESS_TOKEN");
        std::env::remove_var("DD_API_KEY");
        std::env::remove_var("DD_APP_KEY");

        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let tmp = std::env::temp_dir().join(format!("pup_cfg_named_org_skip_{nanos}"));
        std::fs::create_dir_all(&tmp).unwrap();
        std::env::set_var("PUP_CONFIG_DIR", &tmp);

        // Default session on EU.
        crate::auth::storage::save_session(&SessionEntry {
            site: "datadoghq.eu".into(),
            org: None,
            org_uuid: None,
        })
        .unwrap();
        // DD_ORG is set to a name that has no saved session; the fallback must
        // not adopt the no-org EU session's site.
        std::env::set_var("DD_ORG", "unknown-org");

        let cfg = Config::from_env().unwrap();

        std::env::remove_var("DD_ORG");
        std::env::remove_var("PUP_CONFIG_DIR");
        let _ = std::fs::remove_dir_all(&tmp);

        // No session for unknown-org and the no-org fallback is blocked by
        // org.is_some(), so we fall through to the hard-coded default.
        assert_eq!(cfg.site, "datadoghq.com");
    }
}
