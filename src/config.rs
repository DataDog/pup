use anyhow::{bail, Result};
#[cfg(not(feature = "browser"))]
use serde::Deserialize;
#[cfg(not(feature = "browser"))]
use std::collections::HashMap;
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
            .unwrap_or_else(|| "datadoghq.com".into());
        let site = normalize_site(&raw_site);

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
            output_format: env_or("DD_OUTPUT", file_cfg.output)
                .and_then(|s| s.parse().ok())
                .unwrap_or(OutputFormat::Json),
            auto_approve: env_bool("DD_AUTO_APPROVE")
                || env_bool("DD_CLI_AUTO_APPROVE")
                || file_cfg.auto_approve.unwrap_or(false),
            agent_mode: false, // set by caller from --agent flag or useragent detection
            read_only: env_bool("DD_READ_ONLY")
                || env_bool("DD_CLI_READ_ONLY")
                || file_cfg.read_only.unwrap_or(false),
        };

        Ok(cfg)
    }

    /// Create configuration from explicit parameters (no env vars or filesystem).
    /// Used by the browser WASM build where `std::env` is unavailable.
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
        }
    }

    /// Override the site as if the user passed it via DD_SITE / `--site` /
    /// config file. Keeps `site` and `site_explicit` in lockstep so a later
    /// `apply_org_override` does not silently swap a user-pinned site for a
    /// session-derived one.
    pub fn set_site_explicit(&mut self, site: String) {
        self.site = normalize_site(&site);
        self.site_explicit = true;
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

    pub fn has_api_keys(&self) -> bool {
        self.api_key.is_some() && self.app_key.is_some()
    }

    pub fn has_bearer_token(&self) -> bool {
        self.access_token.is_some()
    }

    /// Returns the API host (e.g., "api.datadoghq.com").
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
        if self.site.contains("oncall") {
            self.site.clone()
        } else {
            format!("api.{}", self.site)
        }
    }

    /// Returns the full API base URL (e.g., "https://api.datadoghq.com").
    /// Respects PUP_MOCK_SERVER for testing (native/WASI only).
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
/// Centralized so the binary entry point and the extension dispatcher share
/// one resolution path.
#[cfg(all(not(feature = "browser"), not(target_arch = "wasm32")))]
pub fn apply_org_override(cfg: &mut Config, org: String) {
    cfg.org = Some(org);
    if !cfg.site_explicit {
        if let Some(saved_site) = cfg
            .org
            .as_deref()
            .and_then(crate::auth::storage::find_session_site)
        {
            cfg.site = normalize_site(&saved_site);
        }
    }
    if std::env::var("DD_ACCESS_TOKEN")
        .ok()
        .filter(|s| !s.is_empty())
        .is_none()
    {
        cfg.access_token = load_token_from_storage(&cfg.site, cfg.org.as_deref());
    }
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

/// Normalize a raw site value from user input into a canonical form.
///
/// Strips leading UI prefixes (`www`, `api`, `app`) and passes everything else
/// through unchanged — including custom subdomains and regional prefixes like
/// `us3`, `ap1`, etc.
///
/// Oncall sites (containing `oncall`) are always passed through unchanged.
///
/// Examples:
///   `app.datadoghq.com`        → `datadoghq.com`
///   `www.datadoghq.com`        → `datadoghq.com`
///   `api.datadoghq.com`        → `datadoghq.com`
///   `app.us3.datadoghq.com`    → `us3.datadoghq.com`
///   `us3.datadoghq.com`        → `us3.datadoghq.com`
///   `custom.datadoghq.com`     → `custom.datadoghq.com`
///   `datadoghq.com`            → `datadoghq.com`
pub fn normalize_site(site: &str) -> String {
    if site.contains("oncall") {
        return site.to_string();
    }
    const STRIP_PREFIXES: &[&str] = &["www", "api", "app"];
    let parts: Vec<&str> = site.split('.').collect();
    let start = parts
        .iter()
        .position(|p| !STRIP_PREFIXES.contains(p))
        .unwrap_or(0);
    parts[start..].join(".")
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

    // --- normalize_site custom subdomain passthrough tests ---

    #[test]
    fn test_normalize_site_custom_subdomain_preserved() {
        assert_eq!(
            normalize_site("customname.datadoghq.com"),
            "customname.datadoghq.com"
        );
    }

    #[test]
    fn test_normalize_site_app_then_custom_subdomain() {
        assert_eq!(
            normalize_site("app.customname.datadoghq.com"),
            "customname.datadoghq.com"
        );
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
        let xdg = dirs::home_dir()
            .unwrap()
            .join(".config/pup/config.yaml");
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
        let xdg = dirs::home_dir()
            .unwrap()
            .join(".config/pup/config.yaml");
        assert_eq!(candidates.len(), 1, "only primary when PUP_CONFIG_DIR is set");
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
        };

        super::apply_org_override(&mut cfg, "org-b".into());

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
        };

        super::apply_org_override(&mut cfg, "org-a".into());

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
        };

        super::apply_org_override(&mut cfg, "unknown-org".into());

        std::env::remove_var("PUP_CONFIG_DIR");
        let _ = std::fs::remove_dir_all(&tmp);

        assert_eq!(cfg.org.as_deref(), Some("unknown-org"));
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
        };

        super::apply_org_override(&mut cfg, "any-org".into());

        std::env::remove_var("DD_ACCESS_TOKEN");
        std::env::remove_var("PUP_CONFIG_DIR");
        let _ = std::fs::remove_dir_all(&tmp);

        assert_eq!(cfg.access_token.as_deref(), Some("env-supplied-token"));
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
        };

        cfg.set_site_explicit("app.datadoghq.eu".into());

        assert_eq!(cfg.site, "datadoghq.eu");
        assert!(cfg.site_explicit);
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
}
