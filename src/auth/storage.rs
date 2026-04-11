use anyhow::{Context, Result};
use std::path::PathBuf;

use super::types::{ClientCredentials, TokenSet};

// ---------------------------------------------------------------------------
// Session registry entry — lightweight label (no secrets)
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct SessionEntry {
    pub site: String,
    pub org: Option<String>,
}

// ---------------------------------------------------------------------------
// Storage trait
// ---------------------------------------------------------------------------

pub trait Storage: Send + Sync {
    #[allow(dead_code)]
    fn backend_type(&self) -> BackendType;
    fn storage_location(&self) -> String;

    fn save_tokens(&self, site: &str, org: Option<&str>, tokens: &TokenSet) -> Result<()>;
    fn load_tokens(&self, site: &str, org: Option<&str>) -> Result<Option<TokenSet>>;
    fn delete_tokens(&self, site: &str, org: Option<&str>) -> Result<()>;

    fn save_client_credentials(&self, site: &str, creds: &ClientCredentials) -> Result<()>;
    fn load_client_credentials(&self, site: &str) -> Result<Option<ClientCredentials>>;
    fn delete_client_credentials(&self, site: &str) -> Result<()>;
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendType {
    Keychain,
    File,
    #[cfg(feature = "browser")]
    LocalStorage,
}

impl std::fmt::Display for BackendType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackendType::Keychain => write!(f, "keychain"),
            BackendType::File => write!(f, "file"),
            #[cfg(feature = "browser")]
            BackendType::LocalStorage => write!(f, "localStorage"),
        }
    }
}

// ---------------------------------------------------------------------------
// File storage (~/.config/pup/)
// ---------------------------------------------------------------------------

pub struct FileStorage {
    base_dir: PathBuf,
}

impl FileStorage {
    pub fn new() -> Result<Self> {
        let base_dir =
            crate::config::config_dir().context("could not determine config directory")?;
        std::fs::create_dir_all(&base_dir)
            .with_context(|| format!("failed to create config dir: {}", base_dir.display()))?;
        Ok(Self { base_dir })
    }
}

impl Storage for FileStorage {
    fn backend_type(&self) -> BackendType {
        BackendType::File
    }

    fn storage_location(&self) -> String {
        self.base_dir.display().to_string()
    }

    fn save_tokens(&self, site: &str, org: Option<&str>, tokens: &TokenSet) -> Result<()> {
        let path = self
            .base_dir
            .join(format!("tokens_{}.json", sanitize(site)));
        let mut map = match std::fs::read_to_string(&path) {
            Ok(json) => parse_token_map(&json).unwrap_or_default(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => OrgTokenMap::new(),
            Err(e) => return Err(e.into()),
        };
        map.insert(org_map_key(org).to_string(), tokens.clone());
        let json = serde_json::to_string_pretty(&map)?;
        std::fs::write(&path, json)
            .with_context(|| format!("failed to write tokens: {}", path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }

    fn load_tokens(&self, site: &str, org: Option<&str>) -> Result<Option<TokenSet>> {
        let path = self
            .base_dir
            .join(format!("tokens_{}.json", sanitize(site)));
        match std::fs::read_to_string(&path) {
            Ok(json) => Ok(parse_token_map(&json)?.remove(org_map_key(org))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn delete_tokens(&self, site: &str, org: Option<&str>) -> Result<()> {
        let path = self
            .base_dir
            .join(format!("tokens_{}.json", sanitize(site)));
        let json = match std::fs::read_to_string(&path) {
            Ok(j) => j,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        let mut map = parse_token_map(&json).unwrap_or_default();
        map.remove(org_map_key(org));
        if map.is_empty() {
            match std::fs::remove_file(&path) {
                Ok(()) | Err(_) => Ok(()),
            }
        } else {
            let json = serde_json::to_string_pretty(&map)?;
            std::fs::write(&path, json)
                .with_context(|| format!("failed to write tokens: {}", path.display()))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
            }
            Ok(())
        }
    }

    fn save_client_credentials(&self, site: &str, creds: &ClientCredentials) -> Result<()> {
        let path = self
            .base_dir
            .join(format!("client_{}.json", sanitize(site)));
        let json = serde_json::to_string_pretty(creds)?;
        std::fs::write(&path, json)
            .with_context(|| format!("failed to write credentials: {}", path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }

    fn load_client_credentials(&self, site: &str) -> Result<Option<ClientCredentials>> {
        let path = self
            .base_dir
            .join(format!("client_{}.json", sanitize(site)));
        match std::fs::read_to_string(&path) {
            Ok(json) => Ok(Some(serde_json::from_str(&json)?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn delete_client_credentials(&self, site: &str) -> Result<()> {
        let path = self
            .base_dir
            .join(format!("client_{}.json", sanitize(site)));
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

// ---------------------------------------------------------------------------
// Keychain storage (via keyring crate) — native only
// ---------------------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
pub struct KeychainStorage;

#[cfg(not(target_arch = "wasm32"))]
const SERVICE_NAME: &str = "pup";

#[cfg(not(target_arch = "wasm32"))]
impl KeychainStorage {
    pub fn new() -> Result<Self> {
        let probe_key = format!("__pup_probe_{}", std::process::id());
        let probe = keyring::Entry::new(SERVICE_NAME, &probe_key)
            .map_err(|e| anyhow::anyhow!("keychain not available: {e}"))?;

        probe
            .set_password("ok")
            .map_err(|e| anyhow::anyhow!("keychain probe write failed: {e}"))?;
        let _ = probe
            .get_password()
            .map_err(|e| anyhow::anyhow!("keychain probe read failed: {e}"))?;
        let _ = probe.delete_credential();

        Ok(Self)
    }
}

/// Combined per-site state stored in a single keychain entry.
/// Consolidating tokens + client credentials into one entry reduces macOS
/// authorization dialogs from 2 → 1 per site on first access.
#[cfg(not(target_arch = "wasm32"))]
#[derive(serde::Serialize, serde::Deserialize, Default)]
struct SiteData {
    #[serde(default)]
    tokens: OrgTokenMap,
    #[serde(default)]
    client: Option<ClientCredentials>,
}

#[cfg(not(target_arch = "wasm32"))]
impl KeychainStorage {
    fn state_key(site: &str) -> String {
        format!("state_{}", sanitize(site))
    }

    fn load_state(&self, site: &str) -> Result<SiteData> {
        let entry = keyring::Entry::new(SERVICE_NAME, &Self::state_key(site))?;
        match entry.get_password() {
            Ok(json) => Ok(serde_json::from_str(&json).unwrap_or_default()),
            Err(keyring::Error::NoEntry) => Ok(SiteData::default()),
            Err(e) => Err(e.into()),
        }
    }

    fn save_state(&self, site: &str, data: &SiteData) -> Result<()> {
        let entry = keyring::Entry::new(SERVICE_NAME, &Self::state_key(site))?;
        let json = serde_json::to_string(data)?;
        entry.set_password(&json).map_err(Into::into)
    }

    fn delete_state(&self, site: &str) -> Result<()> {
        let entry = keyring::Entry::new(SERVICE_NAME, &Self::state_key(site))?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Storage for KeychainStorage {
    fn backend_type(&self) -> BackendType {
        BackendType::Keychain
    }

    fn storage_location(&self) -> String {
        "OS keychain".to_string()
    }

    fn save_tokens(&self, site: &str, org: Option<&str>, tokens: &TokenSet) -> Result<()> {
        let mut data = self.load_state(site)?;
        data.tokens
            .insert(org_map_key(org).to_string(), tokens.clone());
        self.save_state(site, &data)
    }

    fn load_tokens(&self, site: &str, org: Option<&str>) -> Result<Option<TokenSet>> {
        Ok(self.load_state(site)?.tokens.remove(org_map_key(org)))
    }

    fn delete_tokens(&self, site: &str, org: Option<&str>) -> Result<()> {
        let mut data = self.load_state(site)?;
        data.tokens.remove(org_map_key(org));
        if data.tokens.is_empty() && data.client.is_none() {
            self.delete_state(site)
        } else {
            self.save_state(site, &data)
        }
    }

    fn save_client_credentials(&self, site: &str, creds: &ClientCredentials) -> Result<()> {
        let mut data = self.load_state(site)?;
        data.client = Some(creds.clone());
        self.save_state(site, &data)
    }

    fn load_client_credentials(&self, site: &str) -> Result<Option<ClientCredentials>> {
        Ok(self.load_state(site)?.client)
    }

    fn delete_client_credentials(&self, site: &str) -> Result<()> {
        let mut data = self.load_state(site)?;
        data.client = None;
        if data.tokens.is_empty() && data.client.is_none() {
            self.delete_state(site)
        } else {
            self.save_state(site, &data)
        }
    }
}

// ---------------------------------------------------------------------------
// Touch ID keychain storage — macOS only
//
// Uses the modern SecItemAdd/SecItemCopyMatching API (not the legacy
// SecKeychain API that the `keyring` crate uses) so that macOS presents
// Touch ID as the authentication method instead of a password dialog.
//
// Access control: kSecAccessControlUserPresence — macOS offers Touch ID
// first, falling back to the login password if Touch ID is unavailable or
// the user cancels. The prompt appears on every keychain access.
//
// Requires the binary to be code-signed (standard for Homebrew releases).
// If the binary is unsigned (e.g. a local dev build), Touch ID access
// control silently degrades to a standard keychain item so the tool
// remains functional.
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
pub struct TouchIdStorage;

/// errSecMissingEntitlement (-34018): thrown by SecItemAdd when using
/// biometric access control on an unsigned binary.
#[cfg(target_os = "macos")]
const ERR_MISSING_ENTITLEMENT: i32 = -34018;

#[cfg(target_os = "macos")]
impl TouchIdStorage {
    pub fn new() -> Self {
        Self
    }

    fn load_state(&self, site: &str) -> Result<SiteData> {
        use security_framework::passwords::{generic_password, PasswordOptions};
        use security_framework_sys::base::errSecItemNotFound;

        let opts =
            PasswordOptions::new_generic_password(SERVICE_NAME, &KeychainStorage::state_key(site));
        match generic_password(opts) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes).unwrap_or_default()),
            Err(e) if e.code() == errSecItemNotFound => Ok(SiteData::default()),
            Err(e) => Err(anyhow::anyhow!("keychain read failed: {e}")),
        }
    }

    fn save_state(&self, site: &str, data: &SiteData) -> Result<()> {
        use security_framework::passwords::{
            delete_generic_password_options, set_generic_password_options, AccessControlOptions,
            PasswordOptions,
        };
        use security_framework_sys::base::errSecDuplicateItem;

        let json = serde_json::to_vec(data)?;
        let key = KeychainStorage::state_key(site);

        // Attempt 1: create with Touch ID access control.
        let mut opts = PasswordOptions::new_generic_password(SERVICE_NAME, &key);
        opts.set_access_control_options(AccessControlOptions::USER_PRESENCE);
        match set_generic_password_options(&json, opts) {
            Ok(()) => return Ok(()),
            // Duplicate item — delete and re-create to apply access control.
            Err(ref e) if e.code() == errSecDuplicateItem => {
                let del_opts = PasswordOptions::new_generic_password(SERVICE_NAME, &key);
                delete_generic_password_options(del_opts).ok();

                let mut opts2 = PasswordOptions::new_generic_password(SERVICE_NAME, &key);
                opts2.set_access_control_options(AccessControlOptions::USER_PRESENCE);
                match set_generic_password_options(&json, opts2) {
                    Ok(()) => return Ok(()),
                    // Still no entitlement after re-create — fall through to plain write.
                    Err(ref e) if e.code() == ERR_MISSING_ENTITLEMENT => {}
                    Err(e) => return Err(anyhow::anyhow!("keychain write failed: {e}")),
                }
            }
            // Binary not code-signed: degrade gracefully to a plain item.
            Err(ref e) if e.code() == ERR_MISSING_ENTITLEMENT => {}
            Err(e) => return Err(anyhow::anyhow!("keychain write failed: {e}")),
        }

        // Attempt 2: write without access control (unsigned binary fallback).
        let opts = PasswordOptions::new_generic_password(SERVICE_NAME, &key);
        set_generic_password_options(&json, opts)
            .map_err(|e| anyhow::anyhow!("keychain write failed: {e}"))
    }

    fn delete_state(&self, site: &str) -> Result<()> {
        use security_framework::passwords::{delete_generic_password_options, PasswordOptions};
        use security_framework_sys::base::errSecItemNotFound;

        let opts =
            PasswordOptions::new_generic_password(SERVICE_NAME, &KeychainStorage::state_key(site));
        match delete_generic_password_options(opts) {
            Ok(()) => Ok(()),
            Err(e) if e.code() == errSecItemNotFound => Ok(()),
            Err(e) => Err(anyhow::anyhow!("keychain delete failed: {e}")),
        }
    }
}

#[cfg(target_os = "macos")]
impl Storage for TouchIdStorage {
    fn backend_type(&self) -> BackendType {
        BackendType::Keychain
    }

    fn storage_location(&self) -> String {
        "OS keychain (Touch ID)".to_string()
    }

    fn save_tokens(&self, site: &str, org: Option<&str>, tokens: &TokenSet) -> Result<()> {
        let mut data = self.load_state(site)?;
        data.tokens
            .insert(org_map_key(org).to_string(), tokens.clone());
        self.save_state(site, &data)
    }

    fn load_tokens(&self, site: &str, org: Option<&str>) -> Result<Option<TokenSet>> {
        Ok(self.load_state(site)?.tokens.remove(org_map_key(org)))
    }

    fn delete_tokens(&self, site: &str, org: Option<&str>) -> Result<()> {
        let mut data = self.load_state(site)?;
        data.tokens.remove(org_map_key(org));
        if data.tokens.is_empty() && data.client.is_none() {
            self.delete_state(site)
        } else {
            self.save_state(site, &data)
        }
    }

    fn save_client_credentials(&self, site: &str, creds: &ClientCredentials) -> Result<()> {
        let mut data = self.load_state(site)?;
        data.client = Some(creds.clone());
        self.save_state(site, &data)
    }

    fn load_client_credentials(&self, site: &str) -> Result<Option<ClientCredentials>> {
        Ok(self.load_state(site)?.client)
    }

    fn delete_client_credentials(&self, site: &str) -> Result<()> {
        let mut data = self.load_state(site)?;
        data.client = None;
        if data.tokens.is_empty() && data.client.is_none() {
            self.delete_state(site)
        } else {
            self.save_state(site, &data)
        }
    }
}

// ---------------------------------------------------------------------------
// In-memory storage (WASM) — no persistent storage available
// ---------------------------------------------------------------------------

#[cfg(target_arch = "wasm32")]
pub struct InMemoryStorage;

#[cfg(target_arch = "wasm32")]
impl Storage for InMemoryStorage {
    fn backend_type(&self) -> BackendType {
        BackendType::File
    }

    fn storage_location(&self) -> String {
        "in-memory (WASM)".to_string()
    }

    fn save_tokens(&self, _site: &str, _org: Option<&str>, _tokens: &TokenSet) -> Result<()> {
        anyhow::bail!("token storage not available in WASM — use DD_ACCESS_TOKEN env var")
    }

    fn load_tokens(&self, _site: &str, _org: Option<&str>) -> Result<Option<TokenSet>> {
        Ok(None)
    }

    fn delete_tokens(&self, _site: &str, _org: Option<&str>) -> Result<()> {
        Ok(())
    }

    fn save_client_credentials(&self, _site: &str, _creds: &ClientCredentials) -> Result<()> {
        anyhow::bail!("client credential storage not available in WASM")
    }

    fn load_client_credentials(&self, _site: &str) -> Result<Option<ClientCredentials>> {
        Ok(None)
    }

    fn delete_client_credentials(&self, _site: &str) -> Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// LocalStorage backend (browser WASM) — persists tokens across page reloads
// ---------------------------------------------------------------------------

#[cfg(feature = "browser")]
pub struct LocalStorageBackend;

#[cfg(feature = "browser")]
impl LocalStorageBackend {
    fn storage() -> Result<web_sys::Storage> {
        let window = web_sys::window().ok_or_else(|| anyhow::anyhow!("no global window object"))?;
        window
            .local_storage()
            .map_err(|_| anyhow::anyhow!("localStorage not available"))?
            .ok_or_else(|| anyhow::anyhow!("localStorage returned None"))
    }

    fn get_item(key: &str) -> Result<Option<String>> {
        let storage = Self::storage()?;
        storage
            .get_item(key)
            .map_err(|_| anyhow::anyhow!("failed to read from localStorage"))
    }

    fn set_item(key: &str, value: &str) -> Result<()> {
        let storage = Self::storage()?;
        storage
            .set_item(key, value)
            .map_err(|_| anyhow::anyhow!("failed to write to localStorage"))
    }

    fn remove_item(key: &str) -> Result<()> {
        let storage = Self::storage()?;
        storage
            .remove_item(key)
            .map_err(|_| anyhow::anyhow!("failed to remove from localStorage"))
    }
}

#[cfg(feature = "browser")]
impl Storage for LocalStorageBackend {
    fn backend_type(&self) -> BackendType {
        BackendType::LocalStorage
    }

    fn storage_location(&self) -> String {
        "browser localStorage".to_string()
    }

    fn save_tokens(&self, site: &str, org: Option<&str>, tokens: &TokenSet) -> Result<()> {
        let key = format!("pup_tokens_{}", sanitize(site));
        let mut map = match Self::get_item(&key)? {
            Some(json) => parse_token_map(&json).unwrap_or_default(),
            None => OrgTokenMap::new(),
        };
        map.insert(org_map_key(org).to_string(), tokens.clone());
        let json = serde_json::to_string(&map)?;
        Self::set_item(&key, &json)
    }

    fn load_tokens(&self, site: &str, org: Option<&str>) -> Result<Option<TokenSet>> {
        let key = format!("pup_tokens_{}", sanitize(site));
        match Self::get_item(&key)? {
            Some(json) => Ok(parse_token_map(&json)?.remove(org_map_key(org))),
            None => Ok(None),
        }
    }

    fn delete_tokens(&self, site: &str, org: Option<&str>) -> Result<()> {
        let key = format!("pup_tokens_{}", sanitize(site));
        let mut map = match Self::get_item(&key)? {
            Some(json) => parse_token_map(&json).unwrap_or_default(),
            None => return Ok(()),
        };
        map.remove(org_map_key(org));
        if map.is_empty() {
            Self::remove_item(&key)
        } else {
            let json = serde_json::to_string(&map)?;
            Self::set_item(&key, &json)
        }
    }

    fn save_client_credentials(&self, site: &str, creds: &ClientCredentials) -> Result<()> {
        let key = format!("pup_client_{}", sanitize(site));
        let json = serde_json::to_string(creds)?;
        Self::set_item(&key, &json)
    }

    fn load_client_credentials(&self, site: &str) -> Result<Option<ClientCredentials>> {
        let key = format!("pup_client_{}", sanitize(site));
        match Self::get_item(&key)? {
            Some(json) => Ok(Some(serde_json::from_str(&json)?)),
            None => Ok(None),
        }
    }

    fn delete_client_credentials(&self, site: &str) -> Result<()> {
        let key = format!("pup_client_{}", sanitize(site));
        Self::remove_item(&key)
    }
}

// ---------------------------------------------------------------------------
// Factory — auto-detect backend, with fallback
// ---------------------------------------------------------------------------

use std::sync::Mutex;

static STORAGE: Mutex<Option<Box<dyn Storage>>> = Mutex::new(None);

pub fn get_storage() -> Result<&'static Mutex<Option<Box<dyn Storage>>>> {
    let mut guard = STORAGE.lock().unwrap();
    if guard.is_none() {
        let backend = detect_backend();
        *guard = Some(backend);
    }
    drop(guard);
    Ok(&STORAGE)
}

#[cfg(not(target_arch = "wasm32"))]
fn detect_backend() -> Box<dyn Storage> {
    // Check DD_TOKEN_STORAGE env var
    if let Ok(val) = std::env::var("DD_TOKEN_STORAGE") {
        match val.as_str() {
            "file" => return Box::new(FileStorage::new().expect("failed to create file storage")),
            "keychain" => return Box::new(KeychainStorage::new().expect("keychain not available")),
            _ => eprintln!("Warning: unknown DD_TOKEN_STORAGE={val:?}, auto-detecting"),
        }
    }

    // On macOS, use Touch ID-capable storage by default.
    #[cfg(target_os = "macos")]
    return Box::new(TouchIdStorage::new());

    // Windows Credential Manager has a strict size limit per secret.
    // The combined auth state can exceed that limit for some orgs, so keep
    // file storage as the safe default unless the user explicitly forces keychain.
    #[cfg(target_os = "windows")]
    return Box::new(FileStorage::new().expect("failed to create file storage"));

    // On other platforms, use keyring when available and fall back to file storage.
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    match KeychainStorage::new() {
        Ok(ks) => Box::new(ks),
        Err(_) => {
            eprintln!("Warning: OS keychain not available, using file storage (~/.config/pup/)");
            Box::new(FileStorage::new().expect("failed to create file storage"))
        }
    }
}

#[cfg(all(target_arch = "wasm32", not(feature = "browser")))]
fn detect_backend() -> Box<dyn Storage> {
    Box::new(InMemoryStorage)
}

#[cfg(feature = "browser")]
fn detect_backend() -> Box<dyn Storage> {
    Box::new(LocalStorageBackend)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn sanitize(site: &str) -> String {
    site.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect()
}

// ---------------------------------------------------------------------------
// OrgTokenMap — one keychain/file entry per site, keyed by org label
// ---------------------------------------------------------------------------

/// All orgs for a site are stored under a single key as a JSON map.
/// The no-org (default) session uses this sentinel as its map key.
const DEFAULT_ORG_KEY: &str = "__default__";

type OrgTokenMap = std::collections::HashMap<String, TokenSet>;

fn org_map_key(org: Option<&str>) -> &str {
    match org {
        Some(o) if !o.is_empty() => o,
        _ => DEFAULT_ORG_KEY,
    }
}

/// Parse a stored blob as an OrgTokenMap, migrating the legacy single-TokenSet
/// format (written by pup < multi-org) to {"__default__": <tokens>} transparently.
fn parse_token_map(json: &str) -> Result<OrgTokenMap> {
    // New format: {"__default__": {...}, "prod-child": {...}}
    if let Ok(map) = serde_json::from_str::<OrgTokenMap>(json) {
        return Ok(map);
    }
    // Old format: bare TokenSet — promote to map under __default__
    if let Ok(tokens) = serde_json::from_str::<TokenSet>(json) {
        let mut map = OrgTokenMap::new();
        map.insert(DEFAULT_ORG_KEY.to_string(), tokens);
        return Ok(map);
    }
    anyhow::bail!("token storage contains unrecognised format")
}

// ---------------------------------------------------------------------------
// Session registry — tracks named org sessions (no secrets stored here)
// ---------------------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
fn sessions_path() -> Option<std::path::PathBuf> {
    crate::config::config_dir().map(|d| d.join("sessions.json"))
}

/// List all stored sessions from the registry file.
/// Returns an empty vec if the file does not exist.
#[cfg(not(target_arch = "wasm32"))]
pub fn list_sessions() -> Result<Vec<SessionEntry>> {
    let path = match sessions_path() {
        Some(p) => p,
        None => return Ok(vec![]),
    };
    match std::fs::read_to_string(&path) {
        Ok(json) => Ok(serde_json::from_str(&json).unwrap_or_default()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(e) => Err(e.into()),
    }
}

/// Upsert a session entry into the registry.
#[cfg(not(target_arch = "wasm32"))]
pub fn save_session(site: &str, org: Option<&str>) -> Result<()> {
    let mut sessions = list_sessions()?;
    let entry = SessionEntry {
        site: site.to_string(),
        org: org.map(String::from),
    };
    // Dedup: remove any existing entry with same site+org, then append
    sessions.retain(|s| !(s.site == entry.site && s.org == entry.org));
    sessions.push(entry);
    write_sessions(&sessions)
}

/// Remove a session entry from the registry.
#[cfg(not(target_arch = "wasm32"))]
pub fn remove_session(site: &str, org: Option<&str>) -> Result<()> {
    let mut sessions = list_sessions()?;
    sessions.retain(|s| !(s.site == site && s.org.as_deref() == org));
    write_sessions(&sessions)
}

#[cfg(not(target_arch = "wasm32"))]
fn write_sessions(sessions: &[SessionEntry]) -> Result<()> {
    let path = match sessions_path() {
        Some(p) => p,
        None => return Ok(()),
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(sessions)?;
    std::fs::write(&path, &json)
        .with_context(|| format!("failed to write sessions: {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- helpers ------------------------------------------------------------

    fn make_token(access: &str) -> TokenSet {
        TokenSet {
            access_token: access.to_string(),
            refresh_token: "refresh".into(),
            token_type: "Bearer".into(),
            expires_in: 9_999_999_999, // far future — never expired
            issued_at: 0,
            scope: String::new(),
            client_id: String::new(),
        }
    }

    /// Temporary directory that removes itself on drop.
    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0);
            let dir = std::env::temp_dir().join(format!("pup_test_{}_{}", label, nanos));
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

    // --- org_map_key --------------------------------------------------------

    #[test]
    fn test_org_map_key_none() {
        assert_eq!(org_map_key(None), DEFAULT_ORG_KEY);
    }

    #[test]
    fn test_org_map_key_empty_string() {
        assert_eq!(org_map_key(Some("")), DEFAULT_ORG_KEY);
    }

    #[test]
    fn test_org_map_key_named() {
        assert_eq!(org_map_key(Some("prod-child")), "prod-child");
    }

    // --- parse_token_map ----------------------------------------------------

    #[test]
    fn test_parse_token_map_new_format() {
        let map: OrgTokenMap = [(DEFAULT_ORG_KEY.to_string(), make_token("tok1"))]
            .into_iter()
            .collect();
        let json = serde_json::to_string(&map).unwrap();
        let parsed = parse_token_map(&json).unwrap();
        assert_eq!(parsed[DEFAULT_ORG_KEY].access_token, "tok1");
    }

    #[test]
    fn test_parse_token_map_multiple_orgs() {
        let map: OrgTokenMap = [
            (DEFAULT_ORG_KEY.to_string(), make_token("default_tok")),
            ("prod".to_string(), make_token("prod_tok")),
        ]
        .into_iter()
        .collect();
        let json = serde_json::to_string(&map).unwrap();
        let parsed = parse_token_map(&json).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[DEFAULT_ORG_KEY].access_token, "default_tok");
        assert_eq!(parsed["prod"].access_token, "prod_tok");
    }

    #[test]
    fn test_parse_token_map_legacy_migration() {
        // Old format: bare TokenSet at the root (written by pup before multi-org)
        let json = serde_json::to_string(&make_token("legacy_tok")).unwrap();
        let parsed = parse_token_map(&json).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[DEFAULT_ORG_KEY].access_token, "legacy_tok");
    }

    #[test]
    fn test_parse_token_map_invalid_json() {
        assert!(parse_token_map("not json at all").is_err());
        assert!(parse_token_map("{\"bad\": true}").is_err());
    }

    // --- FileStorage — token map behaviour ----------------------------------

    #[test]
    fn test_file_storage_save_load_default_org() {
        let tmp = TempDir::new("fs_default");
        let store = FileStorage {
            base_dir: tmp.path().clone(),
        };
        store
            .save_tokens("datadoghq.com", None, &make_token("default_tok"))
            .unwrap();
        let loaded = store.load_tokens("datadoghq.com", None).unwrap().unwrap();
        assert_eq!(loaded.access_token, "default_tok");
    }

    #[test]
    fn test_file_storage_save_load_named_org() {
        let tmp = TempDir::new("fs_named");
        let store = FileStorage {
            base_dir: tmp.path().clone(),
        };
        store
            .save_tokens("datadoghq.com", Some("prod-child"), &make_token("prod_tok"))
            .unwrap();
        let loaded = store
            .load_tokens("datadoghq.com", Some("prod-child"))
            .unwrap()
            .unwrap();
        assert_eq!(loaded.access_token, "prod_tok");
    }

    #[test]
    fn test_file_storage_multiple_orgs_one_file() {
        let tmp = TempDir::new("fs_multi");
        let store = FileStorage {
            base_dir: tmp.path().clone(),
        };

        store
            .save_tokens("datadoghq.com", None, &make_token("default_tok"))
            .unwrap();
        store
            .save_tokens("datadoghq.com", Some("prod"), &make_token("prod_tok"))
            .unwrap();
        store
            .save_tokens("datadoghq.com", Some("staging"), &make_token("staging_tok"))
            .unwrap();

        // Only one file on disk for this site
        let files: Vec<_> = std::fs::read_dir(tmp.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(files.len(), 1);

        // All three orgs load independently
        assert_eq!(
            store
                .load_tokens("datadoghq.com", None)
                .unwrap()
                .unwrap()
                .access_token,
            "default_tok"
        );
        assert_eq!(
            store
                .load_tokens("datadoghq.com", Some("prod"))
                .unwrap()
                .unwrap()
                .access_token,
            "prod_tok"
        );
        assert_eq!(
            store
                .load_tokens("datadoghq.com", Some("staging"))
                .unwrap()
                .unwrap()
                .access_token,
            "staging_tok"
        );
    }

    #[test]
    fn test_file_storage_org_isolation() {
        // Loading a different org must not return another org's token
        let tmp = TempDir::new("fs_isolation");
        let store = FileStorage {
            base_dir: tmp.path().clone(),
        };

        store
            .save_tokens("datadoghq.com", Some("prod"), &make_token("prod_tok"))
            .unwrap();
        assert!(store.load_tokens("datadoghq.com", None).unwrap().is_none());
        assert!(store
            .load_tokens("datadoghq.com", Some("staging"))
            .unwrap()
            .is_none());
    }

    #[test]
    fn test_file_storage_delete_last_org_removes_file() {
        let tmp = TempDir::new("fs_del_last");
        let store = FileStorage {
            base_dir: tmp.path().clone(),
        };

        store
            .save_tokens("datadoghq.com", None, &make_token("tok"))
            .unwrap();
        store.delete_tokens("datadoghq.com", None).unwrap();

        let file_path = tmp.path().join("tokens_datadoghq_com.json");
        assert!(
            !file_path.exists(),
            "file should be removed when last org is deleted"
        );
    }

    #[test]
    fn test_file_storage_delete_one_org_keeps_others() {
        let tmp = TempDir::new("fs_del_one");
        let store = FileStorage {
            base_dir: tmp.path().clone(),
        };

        store
            .save_tokens("datadoghq.com", None, &make_token("default_tok"))
            .unwrap();
        store
            .save_tokens("datadoghq.com", Some("prod"), &make_token("prod_tok"))
            .unwrap();
        store.delete_tokens("datadoghq.com", Some("prod")).unwrap();

        // Default session survives
        assert_eq!(
            store
                .load_tokens("datadoghq.com", None)
                .unwrap()
                .unwrap()
                .access_token,
            "default_tok"
        );
        // Deleted org is gone
        assert!(store
            .load_tokens("datadoghq.com", Some("prod"))
            .unwrap()
            .is_none());
    }

    #[test]
    fn test_file_storage_delete_nonexistent_is_ok() {
        let tmp = TempDir::new("fs_del_none");
        let store = FileStorage {
            base_dir: tmp.path().clone(),
        };
        assert!(store.delete_tokens("datadoghq.com", None).is_ok());
    }

    #[test]
    fn test_file_storage_legacy_migration() {
        let tmp = TempDir::new("fs_legacy");
        let store = FileStorage {
            base_dir: tmp.path().clone(),
        };

        // Write old-format file: bare TokenSet, no map wrapper
        let legacy_json = serde_json::to_string_pretty(&make_token("legacy_tok")).unwrap();
        let path = tmp.path().join("tokens_datadoghq_com.json");
        std::fs::write(&path, legacy_json).unwrap();

        // Existing default session loads transparently
        let loaded = store.load_tokens("datadoghq.com", None).unwrap().unwrap();
        assert_eq!(loaded.access_token, "legacy_tok");

        // Named org not found in the old-format file
        assert!(store
            .load_tokens("datadoghq.com", Some("prod"))
            .unwrap()
            .is_none());
    }

    // --- Session registry ---------------------------------------------------

    #[test]
    fn test_session_registry_empty() {
        let _lock = crate::test_utils::ENV_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let tmp = TempDir::new("sess_empty");
        std::env::set_var("PUP_CONFIG_DIR", tmp.path());
        let sessions = list_sessions().unwrap();
        std::env::remove_var("PUP_CONFIG_DIR");
        assert!(sessions.is_empty());
    }

    #[test]
    fn test_session_registry_save_and_list() {
        let _lock = crate::test_utils::ENV_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let tmp = TempDir::new("sess_save");
        std::env::set_var("PUP_CONFIG_DIR", tmp.path());

        save_session("datadoghq.com", None).unwrap();
        save_session("datadoghq.com", Some("prod-child")).unwrap();
        let sessions = list_sessions().unwrap();
        std::env::remove_var("PUP_CONFIG_DIR");

        assert_eq!(sessions.len(), 2);
        assert!(sessions
            .iter()
            .any(|s| s.site == "datadoghq.com" && s.org.is_none()));
        assert!(sessions
            .iter()
            .any(|s| s.site == "datadoghq.com" && s.org.as_deref() == Some("prod-child")));
    }

    #[test]
    fn test_session_registry_dedup() {
        let _lock = crate::test_utils::ENV_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let tmp = TempDir::new("sess_dedup");
        std::env::set_var("PUP_CONFIG_DIR", tmp.path());

        save_session("datadoghq.com", Some("prod")).unwrap();
        save_session("datadoghq.com", Some("prod")).unwrap(); // duplicate
        let sessions = list_sessions().unwrap();
        std::env::remove_var("PUP_CONFIG_DIR");

        assert_eq!(sessions.len(), 1);
    }

    #[test]
    fn test_session_registry_remove() {
        let _lock = crate::test_utils::ENV_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let tmp = TempDir::new("sess_remove");
        std::env::set_var("PUP_CONFIG_DIR", tmp.path());

        save_session("datadoghq.com", None).unwrap();
        save_session("datadoghq.com", Some("prod")).unwrap();
        remove_session("datadoghq.com", Some("prod")).unwrap();
        let sessions = list_sessions().unwrap();
        std::env::remove_var("PUP_CONFIG_DIR");

        assert_eq!(sessions.len(), 1);
        assert!(sessions[0].org.is_none());
    }

    #[test]
    fn test_session_registry_remove_nonexistent() {
        let _lock = crate::test_utils::ENV_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let tmp = TempDir::new("sess_rm_none");
        std::env::set_var("PUP_CONFIG_DIR", tmp.path());
        let result = remove_session("datadoghq.com", Some("nonexistent"));
        std::env::remove_var("PUP_CONFIG_DIR");
        assert!(result.is_ok());
    }
}
