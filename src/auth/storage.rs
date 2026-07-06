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
    /// Authoritative org UUID (`dd_oid`) returned by the OAuth issuer.
    /// `#[serde(default)]` lets sessions.json files written before this field
    /// existed deserialize cleanly (treated as None).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub org_uuid: Option<String>,
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

/// OS keychain storage via the `keyring` crate.
///
/// `cache` memoizes the per-site `SiteData` for the lifetime of the process. A
/// single command often reads the same item several times (token load, client
/// credential load, plus command-level reads); without the cache each read is a
/// separate OS keychain access, and on an untrusted binary each one raises its
/// own authorization prompt. Reading once and serving the rest from memory keeps
/// that to a single prompt per site per command. Writes update the cache so a
/// later read in the same process sees fresh data.
#[cfg(not(target_arch = "wasm32"))]
pub struct KeychainStorage {
    cache: Mutex<HashMap<String, SiteData>>,
}

#[cfg(not(target_arch = "wasm32"))]
const SERVICE_NAME: &str = "pup";

#[cfg(not(target_arch = "wasm32"))]
impl KeychainStorage {
    pub fn new() -> Result<Self> {
        // On Windows, WinCred silently uses an in-memory mock unless windows-native
        // is enabled, and even then has a 2560-byte blob limit that SiteData exceeds.
        // Do a real write/read/delete cycle so an unusable backend fails fast here
        // rather than silently losing tokens at runtime.
        #[cfg(target_os = "windows")]
        {
            let entry = keyring::Entry::new(SERVICE_NAME, "__pup_probe__")
                .map_err(|e| anyhow::anyhow!("keychain not available: {e}"))?;
            entry
                .set_password("probe")
                .map_err(|e| anyhow::anyhow!("keychain write failed: {e}"))?;
            entry
                .get_password()
                .map_err(|e| anyhow::anyhow!("keychain read failed: {e}"))?;
            entry
                .delete_credential()
                .map_err(|e| anyhow::anyhow!("keychain probe cleanup failed: {e}"))?;
        }
        // On Linux, also perform a read probe so we fail fast when the
        // Secret Service DBus name is unavailable and can fall back to file
        // storage instead of erroring later during auth flows.
        #[cfg(target_os = "linux")]
        {
            let entry = keyring::Entry::new(SERVICE_NAME, "__pup_probe__")
                .map_err(|e| anyhow::anyhow!("keychain not available: {e}"))?;
            linux_keychain_probe_result(entry.get_password())?;
        }
        // On macOS and other non-Windows targets, constructing an Entry is
        // sufficient to confirm the backend is present; avoid a spurious macOS
        // authorization dialog.
        #[cfg(all(not(target_os = "windows"), not(target_os = "linux")))]
        keyring::Entry::new(SERVICE_NAME, "__pup_probe__")
            .map_err(|e| anyhow::anyhow!("keychain not available: {e}"))?;
        Ok(Self {
            cache: Mutex::new(HashMap::new()),
        })
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "linux"))]
fn linux_keychain_probe_result(
    probe_result: std::result::Result<String, keyring::Error>,
) -> Result<()> {
    match probe_result {
        Ok(_) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(anyhow::anyhow!("keychain not available: {e}")),
    }
}

/// Combined per-site state stored in a single keychain entry.
/// Consolidating tokens + client credentials into one entry reduces macOS
/// authorization dialogs from 2 → 1 per site on first access. KeychainStorage
/// additionally memoizes this per-process (see its `cache`) so repeated reads
/// within one command hit the OS keychain at most once per site.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, serde::Serialize, serde::Deserialize, Default)]
struct SiteData {
    #[serde(default)]
    tokens: OrgTokenMap,
    #[serde(default)]
    client: Option<ClientCredentials>,
}

// Maximum characters per WinCred blob entry.
//
// WinCred stores passwords as UTF-16LE, so each ASCII character occupies 2 bytes.
// CRED_MAX_CREDENTIAL_BLOB_SIZE = 2560 bytes → 1280 ASCII characters maximum.
// We use 1000 to stay well clear of that limit.
//
// SiteData (access token + refresh token + 79 scopes + client credentials) easily
// exceeds 1000 characters, so on Windows we split the JSON across numbered entries.
#[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
const WIN_CHUNK_BYTES: usize = 1000;

#[cfg(not(target_arch = "wasm32"))]
impl KeychainStorage {
    fn state_key(site: &str) -> String {
        format!("state_{}", sanitize(site))
    }

    // --- per-process cache wrappers ----------------------------------------------
    // Collapse repeated reads of the same site within one command to a single OS
    // keychain access. `*_raw` methods below do the actual keychain I/O.

    fn load_state(&self, site: &str) -> Result<SiteData> {
        if let Some(data) = self
            .cache
            .lock()
            .map_err(|_| anyhow::anyhow!("keychain cache poisoned"))?
            .get(site)
        {
            return Ok(data.clone());
        }
        let data = self.load_state_raw(site)?;
        self.cache
            .lock()
            .map_err(|_| anyhow::anyhow!("keychain cache poisoned"))?
            .insert(site.to_string(), data.clone());
        Ok(data)
    }

    fn save_state(&self, site: &str, data: &SiteData) -> Result<()> {
        self.save_state_raw(site, data)?;
        self.cache
            .lock()
            .map_err(|_| anyhow::anyhow!("keychain cache poisoned"))?
            .insert(site.to_string(), data.clone());
        Ok(())
    }

    fn delete_state(&self, site: &str) -> Result<()> {
        self.delete_state_raw(site)?;
        self.cache
            .lock()
            .map_err(|_| anyhow::anyhow!("keychain cache poisoned"))?
            .remove(site);
        Ok(())
    }

    // --- non-Windows: single keychain entry per site ----------------------------

    #[cfg(not(target_os = "windows"))]
    fn load_state_raw(&self, site: &str) -> Result<SiteData> {
        let entry = keyring::Entry::new(SERVICE_NAME, &Self::state_key(site))?;
        match entry.get_password() {
            Ok(json) => Ok(serde_json::from_str(&json).unwrap_or_default()),
            Err(keyring::Error::NoEntry) => Ok(SiteData::default()),
            Err(e) => Err(e.into()),
        }
    }

    #[cfg(not(target_os = "windows"))]
    fn save_state_raw(&self, site: &str, data: &SiteData) -> Result<()> {
        let entry = keyring::Entry::new(SERVICE_NAME, &Self::state_key(site))?;
        let json = serde_json::to_string(data)?;
        entry.set_password(&json).map_err(Into::into)
    }

    #[cfg(not(target_os = "windows"))]
    fn delete_state_raw(&self, site: &str) -> Result<()> {
        let entry = keyring::Entry::new(SERVICE_NAME, &Self::state_key(site))?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    // --- Windows: chunked entries to stay within WinCred's 2560-byte blob limit --
    //
    // Chunk count is stored under "<key>_c"; chunks under "<key>_0", "<key>_1", …
    // On load the legacy single-entry format is tried as a fallback so that any
    // data stored before this scheme was introduced is still readable.

    #[cfg(target_os = "windows")]
    fn load_state_raw(&self, site: &str) -> Result<SiteData> {
        let base = Self::state_key(site);
        let count_entry = keyring::Entry::new(SERVICE_NAME, &format!("{base}_c"))?;
        let n: usize = match count_entry.get_password() {
            Ok(s) => s
                .trim()
                .parse()
                .map_err(|_| anyhow::anyhow!("corrupt keychain chunk count"))?,
            Err(keyring::Error::NoEntry) => {
                // No chunk count — try the legacy single-entry format.
                let entry = keyring::Entry::new(SERVICE_NAME, &base)?;
                return match entry.get_password() {
                    Ok(json) => Ok(serde_json::from_str(&json).unwrap_or_default()),
                    Err(keyring::Error::NoEntry) => Ok(SiteData::default()),
                    Err(e) => Err(e.into()),
                };
            }
            Err(e) => return Err(e.into()),
        };
        if n == 0 {
            return Ok(SiteData::default());
        }
        let mut json = String::new();
        for i in 0..n {
            let entry = keyring::Entry::new(SERVICE_NAME, &format!("{base}_{i}"))?;
            match entry.get_password() {
                Ok(chunk) => json.push_str(&chunk),
                // A missing chunk means partial WinCred corruption (e.g. manual
                // deletion). Return empty state so the caller sees "not logged in"
                // rather than partial or garbled data.
                Err(keyring::Error::NoEntry) => return Ok(SiteData::default()),
                Err(e) => return Err(e.into()),
            }
        }
        Ok(serde_json::from_str(&json).unwrap_or_default())
    }

    #[cfg(target_os = "windows")]
    fn save_state_raw(&self, site: &str, data: &SiteData) -> Result<()> {
        let base = Self::state_key(site);
        let json = serde_json::to_string(data)?;
        // Tokens, scope words, and JSON punctuation are all ASCII in practice.
        // If a future field introduces non-ASCII UTF-8, from_utf8 below will
        // return an error rather than silently producing garbled data.
        let chunks = json
            .as_bytes()
            .chunks(WIN_CHUNK_BYTES)
            .map(|b| std::str::from_utf8(b).map_err(|e| anyhow::anyhow!("chunk encoding: {e}")))
            .collect::<Result<Vec<_>>>()?;
        let n = chunks.len();

        // Read the old count so we can delete any stale extra entries after writing.
        let count_entry = keyring::Entry::new(SERVICE_NAME, &format!("{base}_c"))?;
        let old_n: usize = count_entry
            .get_password()
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);

        // Write chunks before committing the count. A crash between chunk writes
        // leaves no count entry (or the prior count), so load_state reads the old
        // data or returns default rather than assembling partial chunks.
        for (i, chunk) in chunks.iter().enumerate() {
            keyring::Entry::new(SERVICE_NAME, &format!("{base}_{i}"))?.set_password(chunk)?;
        }
        count_entry.set_password(&n.to_string())?;
        // Remove stale chunks left over from a prior write that had more entries.
        for i in n..old_n {
            // Best-effort: ignore errors on stale-chunk cleanup.
            let _ = keyring::Entry::new(SERVICE_NAME, &format!("{base}_{i}"))
                .and_then(|e| e.delete_credential());
        }
        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn delete_state_raw(&self, site: &str) -> Result<()> {
        let base = Self::state_key(site);
        let count_entry = keyring::Entry::new(SERVICE_NAME, &format!("{base}_c"))?;
        let n: usize = match count_entry.get_password() {
            Ok(s) => {
                let n = s
                    .trim()
                    .parse()
                    .map_err(|_| anyhow::anyhow!("corrupt keychain chunk count"))?;
                match count_entry.delete_credential() {
                    Ok(()) | Err(keyring::Error::NoEntry) => {}
                    Err(e) => return Err(e.into()),
                }
                n
            }
            Err(keyring::Error::NoEntry) => {
                // Legacy single-entry format — delete and return.
                let entry = keyring::Entry::new(SERVICE_NAME, &base)?;
                match entry.delete_credential() {
                    Ok(()) | Err(keyring::Error::NoEntry) => return Ok(()),
                    Err(e) => return Err(e.into()),
                }
            }
            Err(e) => return Err(e.into()),
        };
        for i in 0..n {
            let entry = keyring::Entry::new(SERVICE_NAME, &format!("{base}_{i}"))?;
            match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
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
        if data.tokens.is_empty() {
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

use std::collections::HashMap;
use std::sync::Mutex;

static STORAGE: Mutex<Option<Box<dyn Storage>>> = Mutex::new(None);

pub fn get_storage() -> Result<&'static Mutex<Option<Box<dyn Storage>>>> {
    let mut guard = STORAGE
        .lock()
        .map_err(|_| anyhow::anyhow!("storage lock poisoned"))?;
    if guard.is_none() {
        let backend = detect_backend();
        *guard = Some(backend);
    }
    drop(guard);
    Ok(&STORAGE)
}

#[cfg(not(target_arch = "wasm32"))]
fn detect_backend() -> Box<dyn Storage> {
    detect_backend_with(KeychainStorage::new)
}

// Separated from detect_backend so tests can inject a failing keychain probe
// and exercise all failure paths without OS-level credential-store mocking.
#[cfg(not(target_arch = "wasm32"))]
fn detect_backend_with(try_keychain: impl Fn() -> Result<KeychainStorage>) -> Box<dyn Storage> {
    // Precedence: DD_TOKEN_STORAGE env var > config file token_storage > auto-detect.
    let storage_hint = std::env::var("DD_TOKEN_STORAGE")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(read_config_token_storage);

    if let Some(ref val) = storage_hint {
        match val.as_str() {
            "file" => return Box::new(FileStorage::new().expect("failed to create file storage")),
            // Explicit opt-in: panic with a clear message if the backend is
            // unavailable rather than silently falling back to a different store.
            "keychain" => return Box::new(try_keychain().expect("keychain not available")),
            _ => eprintln!(
                "Warning: unknown token storage backend {val:?} (set via DD_TOKEN_STORAGE or config token_storage; valid: \"file\", \"keychain\"), auto-detecting"
            ),
        }
    }

    // Auto-detect: probe the OS keychain and fall back to file storage if unavailable.
    // The same standard OS keychain is used on every platform (on Windows the chunked
    // WinCred scheme keeps blobs within the 2560-byte limit).
    match try_keychain() {
        Ok(ks) => Box::new(ks),
        Err(e) => {
            // On Linux, the default backend is Secret Service (DBus). If that is
            // unavailable, fall back to the kernel keyring (keyutils) which does
            // not require a desktop session or running daemon.
            #[cfg(target_os = "linux")]
            {
                eprintln!("Warning: Secret Service not available ({e}), trying kernel keyring");
                keyring::set_default_credential_builder(
                    keyring::keyutils::default_credential_builder(),
                );
                match try_keychain() {
                    Ok(ks) => Box::new(ks),
                    Err(e2) => {
                        eprintln!(
                            "Warning: kernel keyring also unavailable ({e2}), using file storage (~/.config/pup/)"
                        );
                        Box::new(FileStorage::new().expect("failed to create file storage"))
                    }
                }
            }
            #[cfg(not(target_os = "linux"))]
            {
                eprintln!(
                    "Warning: OS keychain not available ({e}), using file storage (~/.config/pup/)"
                );
                Box::new(FileStorage::new().expect("failed to create file storage"))
            }
        }
    }
}

/// Read `token_storage` from the pup config file, returning the raw string value
/// if present. Used as a fallback when `DD_TOKEN_STORAGE` env var is not set.
#[cfg(not(target_arch = "wasm32"))]
fn read_config_token_storage() -> Option<String> {
    #[derive(serde::Deserialize, Default)]
    struct StorageHint {
        token_storage: Option<String>,
    }

    // Mirror config_file_candidates() from config.rs. We inline the path logic here
    // rather than calling into config.rs because config.rs calls get_storage(), and
    // calling config functions during storage initialisation could be confusing;
    // config_dir() is safe (it only reads env/filesystem), so we use it directly.
    let config_dir = crate::config::config_dir()?;
    #[cfg_attr(not(target_os = "macos"), allow(unused_mut))]
    let mut candidates = vec![config_dir.join("config.yaml")];
    // On macOS also check the XDG-style path (~/.config/pup/) as a fallback,
    // mirroring the behaviour of config_file_candidates().
    #[cfg(target_os = "macos")]
    if std::env::var("PUP_CONFIG_DIR")
        .ok()
        .filter(|s| !s.is_empty())
        .is_none()
    {
        if let Some(home) = dirs::home_dir() {
            let xdg = home.join(".config/pup/config.yaml");
            if !candidates.contains(&xdg) {
                candidates.push(xdg);
            }
        }
    }

    // Mirror load_config_file(): use the first readable file, parse only that one.
    // This avoids the subtle case where the primary file exists but lacks
    // `token_storage`, causing us to fall through to the XDG fallback while the
    // rest of pup's config comes from the primary file.
    let contents = candidates
        .into_iter()
        .find_map(|path| std::fs::read_to_string(path).ok())?;

    match serde_norway::from_str::<StorageHint>(&contents) {
        Ok(hint) => hint.token_storage,
        Err(e) => {
            eprintln!("Warning: could not parse pup config (token_storage ignored): {e}");
            None
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

/// Upsert a session entry into the registry. Dedups on `org` alone; the
/// new entry's site and `org_uuid` win so re-auth to a different site
/// replaces the existing entry rather than accumulating a second one.
#[cfg(not(target_arch = "wasm32"))]
pub fn save_session(entry: &SessionEntry) -> Result<()> {
    let mut sessions = list_sessions()?;
    sessions.retain(|s| s.org != entry.org);
    sessions.push(entry.clone());
    write_sessions(&sessions)
}

/// Remove a session entry from the registry.
#[cfg(not(target_arch = "wasm32"))]
pub fn remove_session(_site: &str, org: Option<&str>) -> Result<()> {
    let mut sessions = list_sessions()?;
    sessions.retain(|s| s.org.as_deref() != org);
    write_sessions(&sessions)
}

/// Look up a single session entry by org name.
#[cfg(not(target_arch = "wasm32"))]
pub fn find_session(org: Option<&str>) -> Option<SessionEntry> {
    list_sessions()
        .ok()?
        .into_iter()
        .find(|s| s.org.as_deref() == org)
}

/// Look up the site for a named org session. Returns None if no session exists
/// for that org. The save_session invariant ensures at most one session per org
/// name, so the lookup is always unambiguous for current data. Legacy sessions.json
/// files written by older pup versions could contain two rows for the same named
/// org on different sites; in that case we return the first match and self-heal
/// on the next `pup auth login --org <name>`.
#[cfg(not(target_arch = "wasm32"))]
pub fn find_session_site(org: &str) -> Option<String> {
    list_sessions()
        .ok()?
        .into_iter()
        .find(|s| s.org.as_deref() == Some(org))
        .map(|s| s.site)
}

/// Return the site for the single no-org ("default") session, if there is
/// exactly one. Zero → None. Multiple (legacy data before the single-slot
/// invariant was enforced) → warns and returns None so the caller falls
/// through to `datadoghq.com`; the next bare login self-heals to one entry.
#[cfg(not(target_arch = "wasm32"))]
pub fn find_default_session_site() -> Option<String> {
    let mut sites: Vec<String> = list_sessions()
        .ok()?
        .into_iter()
        .filter(|s| s.org.is_none())
        .map(|s| s.site)
        .collect();
    sites.sort();
    sites.dedup();
    match sites.len() {
        1 => sites.pop(),
        0 => None,
        _ => {
            // Legacy: multiple no-org rows on different sites. A bare login
            // enforces the single-slot invariant (see prune_other_default_sessions),
            // so this path only fires for pre-existing data and self-heals on the
            // next bare login (a named `--org` login does not prune).
            eprintln!(
                "Warning: multiple default (no-org) sessions on different \
                 sites ({}); not auto-selecting. Set DD_SITE or re-run \
                 pup auth login.",
                sites.join(", ")
            );
            None
        }
    }
}

/// Remove session registry rows for `(site != keep_site, org=None)` and
/// best-effort delete their stored tokens, enforcing the single-slot
/// invariant for the no-org ("default") session. Called after a bare login
/// so that switching regions replaces the previous unnamed default rather
/// than accumulating ambiguous entries that confuse `find_default_session_site`.
///
/// Note: `save_session(None)` now also removes all prior no-org rows as part
/// of the single-slot invariant, so in the normal bare-login flow `to_prune`
/// will be empty when this runs. This function remains as a defensive guard
/// for legacy on-disk data (sessions.json written by older pup versions)
/// that may have multiple no-org rows for different sites, and to clean up
/// any orphaned tokens from the displaced session.
///
/// Named-org sessions on any site are never touched.
#[cfg(not(target_arch = "wasm32"))]
pub fn prune_other_default_sessions(keep_site: &str) -> Result<()> {
    let sessions = list_sessions()?;
    let to_prune: Vec<String> = sessions
        .into_iter()
        .filter(|s| s.org.is_none() && s.site != keep_site)
        .map(|s| s.site)
        .collect();
    if to_prune.is_empty() {
        return Ok(());
    }
    // Remove the session rows FIRST: rows are the authoritative source of truth
    // for resolution, so the only residue a later failure can leave is an orphan
    // token (never read — tokens are only loaded by an explicit (site, org) key).
    // Doing token deletion first would risk the opposite: a failed write_sessions
    // leaving a tokenless extra no-org row, which find_default_session_site treats
    // as ambiguous and falls back to datadoghq.com — the exact #592 failure.
    let mut sessions = list_sessions()?;
    sessions.retain(|s| !(s.org.is_none() && to_prune.contains(&s.site)));
    write_sessions(&sessions)?;
    // Best-effort token cleanup: initialise the storage backend only now that the
    // rows are gone. delete_tokens(site, None) removes only the no-org token slot
    // for that site; named-org tokens on the same site are untouched.
    let guard = get_storage()?;
    let mut lock = guard.lock().unwrap();
    let store = lock.as_mut().unwrap();
    for site in &to_prune {
        let _ = store.delete_tokens(site, None);
    }
    Ok(())
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

    // --- SessionEntry serde -------------------------------------------------

    #[test]
    fn test_session_entry_legacy_json_deserializes_with_no_org_uuid() {
        // sessions.json files written before the org_uuid field existed must
        // continue to deserialize. #[serde(default)] gives them None.
        let legacy = r#"[{"site":"datadoghq.com","org":"prod-child"}]"#;
        let parsed: Vec<SessionEntry> = serde_json::from_str(legacy).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].site, "datadoghq.com");
        assert_eq!(parsed[0].org.as_deref(), Some("prod-child"));
        assert!(parsed[0].org_uuid.is_none());
    }

    #[test]
    fn test_session_entry_roundtrip_with_uuid() {
        let entry = SessionEntry {
            site: "datadoghq.com".into(),
            org: Some("prod-child".into()),
            org_uuid: Some("00000000-1111-2222-3333-444444444444".into()),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let parsed: SessionEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, entry);
    }

    #[test]
    fn test_session_entry_omits_uuid_when_none() {
        // skip_serializing_if keeps existing on-disk shapes byte-stable for
        // sessions that were never tagged with a UUID.
        let entry = SessionEntry {
            site: "datadoghq.com".into(),
            org: None,
            org_uuid: None,
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(!json.contains("org_uuid"), "got: {json}");
    }

    // --- Session registry ---------------------------------------------------

    #[test]
    fn test_session_registry_empty() {
        let _lock = crate::test_utils::ENV_LOCK.blocking_lock();
        let tmp = TempDir::new("sess_empty");
        std::env::set_var("PUP_CONFIG_DIR", tmp.path());
        let sessions = list_sessions().unwrap();
        std::env::remove_var("PUP_CONFIG_DIR");
        assert!(sessions.is_empty());
    }

    #[test]
    fn test_session_registry_save_and_list() {
        let _lock = crate::test_utils::ENV_LOCK.blocking_lock();
        let tmp = TempDir::new("sess_save");
        std::env::set_var("PUP_CONFIG_DIR", tmp.path());

        save_session(&SessionEntry {
            site: "datadoghq.com".into(),
            org: None,
            org_uuid: None,
        })
        .unwrap();
        save_session(&SessionEntry {
            site: "datadoghq.com".into(),
            org: Some("prod-child".into()),
            org_uuid: None,
        })
        .unwrap();
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
        let _lock = crate::test_utils::ENV_LOCK.blocking_lock();
        let tmp = TempDir::new("sess_dedup");
        std::env::set_var("PUP_CONFIG_DIR", tmp.path());

        save_session(&SessionEntry {
            site: "datadoghq.com".into(),
            org: Some("prod".into()),
            org_uuid: None,
        })
        .unwrap();
        save_session(&SessionEntry {
            site: "datadoghq.com".into(),
            org: Some("prod".into()),
            org_uuid: None,
        })
        .unwrap(); // duplicate
        let sessions = list_sessions().unwrap();
        std::env::remove_var("PUP_CONFIG_DIR");

        assert_eq!(sessions.len(), 1);
    }

    #[test]
    fn test_session_registry_remove() {
        let _lock = crate::test_utils::ENV_LOCK.blocking_lock();
        let tmp = TempDir::new("sess_remove");
        std::env::set_var("PUP_CONFIG_DIR", tmp.path());

        save_session(&SessionEntry {
            site: "datadoghq.com".into(),
            org: None,
            org_uuid: None,
        })
        .unwrap();
        save_session(&SessionEntry {
            site: "datadoghq.com".into(),
            org: Some("prod".into()),
            org_uuid: None,
        })
        .unwrap();
        remove_session("datadoghq.com", Some("prod")).unwrap();
        let sessions = list_sessions().unwrap();
        std::env::remove_var("PUP_CONFIG_DIR");

        assert_eq!(sessions.len(), 1);
        assert!(sessions[0].org.is_none());
    }

    #[test]
    fn test_session_registry_remove_nonexistent() {
        let _lock = crate::test_utils::ENV_LOCK.blocking_lock();
        let tmp = TempDir::new("sess_rm_none");
        std::env::set_var("PUP_CONFIG_DIR", tmp.path());
        let result = remove_session("datadoghq.com", Some("nonexistent"));
        std::env::remove_var("PUP_CONFIG_DIR");
        assert!(result.is_ok());
    }

    #[test]
    fn test_find_session_site_unique_match() {
        let _lock = crate::test_utils::ENV_LOCK.blocking_lock();
        let tmp = TempDir::new("find_sess_unique");
        std::env::set_var("PUP_CONFIG_DIR", tmp.path());

        save_session(&SessionEntry {
            site: "custom.datadoghq.com".into(),
            org: Some("prod-child".into()),
            org_uuid: None,
        })
        .unwrap();
        save_session(&SessionEntry {
            site: "datadoghq.com".into(),
            org: None,
            org_uuid: None,
        })
        .unwrap();
        let result = find_session_site("prod-child");
        std::env::remove_var("PUP_CONFIG_DIR");

        assert_eq!(result.as_deref(), Some("custom.datadoghq.com"));
    }

    #[test]
    fn test_find_session_site_no_match() {
        let _lock = crate::test_utils::ENV_LOCK.blocking_lock();
        let tmp = TempDir::new("find_sess_none");
        std::env::set_var("PUP_CONFIG_DIR", tmp.path());

        save_session(&SessionEntry {
            site: "datadoghq.com".into(),
            org: Some("prod-child".into()),
            org_uuid: None,
        })
        .unwrap();
        let result = find_session_site("nonexistent");
        std::env::remove_var("PUP_CONFIG_DIR");

        assert!(result.is_none());
    }

    #[test]
    fn test_save_session_same_name_different_site_overwrites() {
        let _lock = crate::test_utils::ENV_LOCK.blocking_lock();
        let tmp = TempDir::new("find_sess_overwrite");
        std::env::set_var("PUP_CONFIG_DIR", tmp.path());

        // First login: org "myorg" on .com.
        save_session(&SessionEntry {
            site: "datadoghq.com".into(),
            org: Some("myorg".into()),
            org_uuid: None,
        })
        .unwrap();
        // Re-login: same org name but different site → overwrites the first.
        save_session(&SessionEntry {
            site: "datadoghq.eu".into(),
            org: Some("myorg".into()),
            org_uuid: None,
        })
        .unwrap();

        let sessions = list_sessions().unwrap();
        let result = find_session_site("myorg");
        std::env::remove_var("PUP_CONFIG_DIR");

        // Only one session remains, on the new site.
        assert_eq!(sessions.len(), 1);
        assert_eq!(result.as_deref(), Some("datadoghq.eu"));
    }

    #[test]
    fn test_find_session_site_skips_default_session() {
        let _lock = crate::test_utils::ENV_LOCK.blocking_lock();
        let tmp = TempDir::new("find_sess_default");
        std::env::set_var("PUP_CONFIG_DIR", tmp.path());

        // The unnamed (org=None) session must not match any --org lookup.
        save_session(&SessionEntry {
            site: "datadoghq.eu".into(),
            org: None,
            org_uuid: None,
        })
        .unwrap();
        let result = find_session_site("anything");
        std::env::remove_var("PUP_CONFIG_DIR");

        assert!(result.is_none());
    }

    // --- find_default_session_site -----------------------------------------------

    #[test]
    fn test_find_default_session_site_no_sessions() {
        let _lock = crate::test_utils::ENV_LOCK.blocking_lock();
        let tmp = TempDir::new("fds_none");
        std::env::set_var("PUP_CONFIG_DIR", tmp.path());
        let result = find_default_session_site();
        std::env::remove_var("PUP_CONFIG_DIR");
        assert!(result.is_none());
    }

    #[test]
    fn test_find_default_session_site_one_no_org_row() {
        let _lock = crate::test_utils::ENV_LOCK.blocking_lock();
        let tmp = TempDir::new("fds_one");
        std::env::set_var("PUP_CONFIG_DIR", tmp.path());
        save_session(&SessionEntry {
            site: "datadoghq.eu".into(),
            org: None,
            org_uuid: None,
        })
        .unwrap();
        // Named-org sessions on other sites must not interfere.
        save_session(&SessionEntry {
            site: "datadoghq.com".into(),
            org: Some("prod".into()),
            org_uuid: None,
        })
        .unwrap();
        let result = find_default_session_site();
        std::env::remove_var("PUP_CONFIG_DIR");
        assert_eq!(result.as_deref(), Some("datadoghq.eu"));
    }

    #[test]
    fn test_find_default_session_site_multiple_no_org_rows_returns_none() {
        // Legacy on-disk data: two no-org rows written by an older pup version →
        // ambiguous; warn + return None. Construct directly via write_sessions
        // because save_session now enforces the single-slot invariant.
        let _lock = crate::test_utils::ENV_LOCK.blocking_lock();
        let tmp = TempDir::new("fds_multi");
        std::env::set_var("PUP_CONFIG_DIR", tmp.path());
        write_sessions(&[
            SessionEntry {
                site: "datadoghq.com".into(),
                org: None,
                org_uuid: None,
            },
            SessionEntry {
                site: "datadoghq.eu".into(),
                org: None,
                org_uuid: None,
            },
        ])
        .unwrap();
        let result = find_default_session_site();
        std::env::remove_var("PUP_CONFIG_DIR");
        assert!(result.is_none());
    }

    // --- prune_other_default_sessions -------------------------------------------

    #[test]
    fn test_prune_removes_other_no_org_sites() {
        let _lock = crate::test_utils::ENV_LOCK.blocking_lock();
        let tmp = TempDir::new("prune_removes");
        std::env::set_var("PUP_CONFIG_DIR", tmp.path());
        std::env::set_var("DD_TOKEN_STORAGE", "file");

        // Old default session on .com; we are now logging into .eu.
        save_session(&SessionEntry {
            site: "datadoghq.com".into(),
            org: None,
            org_uuid: None,
        })
        .unwrap();
        // Named-org session on .com — must survive.
        save_session(&SessionEntry {
            site: "datadoghq.com".into(),
            org: Some("prod".into()),
            org_uuid: None,
        })
        .unwrap();
        // New default session we are keeping.
        save_session(&SessionEntry {
            site: "datadoghq.eu".into(),
            org: None,
            org_uuid: None,
        })
        .unwrap();

        prune_other_default_sessions("datadoghq.eu").unwrap();

        let sessions = list_sessions().unwrap();
        std::env::remove_var("DD_TOKEN_STORAGE");
        std::env::remove_var("PUP_CONFIG_DIR");

        // No-org .com row is gone.
        assert!(!sessions
            .iter()
            .any(|s| s.site == "datadoghq.com" && s.org.is_none()));
        // Keep-site no-org row survives.
        assert!(sessions
            .iter()
            .any(|s| s.site == "datadoghq.eu" && s.org.is_none()));
        // Named-org row on .com survives.
        assert!(sessions
            .iter()
            .any(|s| s.site == "datadoghq.com" && s.org.as_deref() == Some("prod")));
    }

    #[test]
    fn test_prune_no_op_when_only_keep_site() {
        let _lock = crate::test_utils::ENV_LOCK.blocking_lock();
        let tmp = TempDir::new("prune_noop");
        std::env::set_var("PUP_CONFIG_DIR", tmp.path());
        std::env::set_var("DD_TOKEN_STORAGE", "file");

        save_session(&SessionEntry {
            site: "datadoghq.com".into(),
            org: None,
            org_uuid: None,
        })
        .unwrap();

        // Pruning for the same site should be a no-op.
        prune_other_default_sessions("datadoghq.com").unwrap();

        let sessions = list_sessions().unwrap();
        std::env::remove_var("DD_TOKEN_STORAGE");
        std::env::remove_var("PUP_CONFIG_DIR");

        assert_eq!(sessions.len(), 1);
        assert!(sessions[0].org.is_none());
        assert_eq!(sessions[0].site, "datadoghq.com");
    }

    #[test]
    fn test_prune_named_org_sessions_untouched() {
        // prune_other_default_sessions must never remove named-org sessions
        // on any site, including the kept site and other sites.
        let _lock = crate::test_utils::ENV_LOCK.blocking_lock();
        let tmp = TempDir::new("prune_named");
        std::env::set_var("PUP_CONFIG_DIR", tmp.path());
        std::env::set_var("DD_TOKEN_STORAGE", "file");

        save_session(&SessionEntry {
            site: "datadoghq.com".into(),
            org: Some("prod".into()),
            org_uuid: None,
        })
        .unwrap();
        save_session(&SessionEntry {
            site: "datadoghq.eu".into(),
            org: Some("staging".into()),
            org_uuid: None,
        })
        .unwrap();

        prune_other_default_sessions("datadoghq.eu").unwrap();

        let sessions = list_sessions().unwrap();
        std::env::remove_var("DD_TOKEN_STORAGE");
        std::env::remove_var("PUP_CONFIG_DIR");

        // Both named-org sessions survive.
        assert_eq!(sessions.len(), 2);
        assert!(sessions
            .iter()
            .any(|s| s.site == "datadoghq.com" && s.org.as_deref() == Some("prod")));
        assert!(sessions
            .iter()
            .any(|s| s.site == "datadoghq.eu" && s.org.as_deref() == Some("staging")));
    }

    #[test]
    fn test_prune_self_heals_legacy_multiple_no_org_rows() {
        // The central migration promise: legacy on-disk data with two no-org rows
        // is ambiguous (find_default_session_site warns + returns None), but a bare
        // login's prune collapses it to one, after which resolution recovers.
        // Construct directly via write_sessions because save_session now enforces
        // the single-slot invariant.
        let _lock = crate::test_utils::ENV_LOCK.blocking_lock();
        let tmp = TempDir::new("prune_self_heal");
        std::env::set_var("PUP_CONFIG_DIR", tmp.path());
        std::env::set_var("DD_TOKEN_STORAGE", "file");

        write_sessions(&[
            SessionEntry {
                site: "datadoghq.com".into(),
                org: None,
                org_uuid: None,
            },
            SessionEntry {
                site: "datadoghq.eu".into(),
                org: None,
                org_uuid: None,
            },
        ])
        .unwrap();

        // Ambiguous before: two no-org rows → None.
        assert!(find_default_session_site().is_none());

        // A bare login to .eu prunes the .com no-org row.
        prune_other_default_sessions("datadoghq.eu").unwrap();

        // Recovered: exactly one no-org row, resolution returns it.
        let healed = find_default_session_site();
        std::env::remove_var("DD_TOKEN_STORAGE");
        std::env::remove_var("PUP_CONFIG_DIR");
        assert_eq!(healed.as_deref(), Some("datadoghq.eu"));
    }

    #[test]
    fn test_prune_deletes_only_other_no_org_token() {
        // Token hygiene + safety: prune deletes the displaced site's no-org token
        // but leaves the kept site's no-org token and any named-org token intact.
        // Construct two no-org rows directly via write_sessions because save_session
        // now enforces the single-slot invariant (this is the legacy-data path).
        let _lock = crate::test_utils::ENV_LOCK.blocking_lock();
        let tmp = TempDir::new("prune_tokens");
        std::env::set_var("PUP_CONFIG_DIR", tmp.path());
        std::env::set_var("DD_TOKEN_STORAGE", "file");

        let guard = get_storage().unwrap();
        {
            let lock = guard.lock().unwrap();
            let store = lock.as_ref().unwrap();
            store
                .save_tokens("datadoghq.com", None, &make_token("com-default"))
                .unwrap();
            store
                .save_tokens("datadoghq.com", Some("prod"), &make_token("com-prod"))
                .unwrap();
            store
                .save_tokens("datadoghq.eu", None, &make_token("eu-default"))
                .unwrap();
        }
        write_sessions(&[
            SessionEntry {
                site: "datadoghq.com".into(),
                org: None,
                org_uuid: None,
            },
            SessionEntry {
                site: "datadoghq.eu".into(),
                org: None,
                org_uuid: None,
            },
        ])
        .unwrap();

        prune_other_default_sessions("datadoghq.eu").unwrap();

        let lock = guard.lock().unwrap();
        let store = lock.as_ref().unwrap();
        let com_default = store.load_tokens("datadoghq.com", None).unwrap();
        let com_prod = store.load_tokens("datadoghq.com", Some("prod")).unwrap();
        let eu_default = store.load_tokens("datadoghq.eu", None).unwrap();
        drop(lock);
        std::env::remove_var("DD_TOKEN_STORAGE");
        std::env::remove_var("PUP_CONFIG_DIR");

        // Displaced no-org token gone; named-org token on the same site and the
        // kept site's no-org token both survive.
        assert!(
            com_default.is_none(),
            "displaced no-org .com token should be deleted"
        );
        assert!(com_prod.is_some(), "named-org .com token must survive");
        assert!(eu_default.is_some(), "kept-site no-org token must survive");
    }

    // --- detect_backend ---------------------------------------------------------

    #[test]
    #[cfg(all(not(target_arch = "wasm32"), target_os = "linux"))]
    fn test_linux_keychain_probe_result_accepts_no_entry() {
        let result = linux_keychain_probe_result(Err(keyring::Error::NoEntry));
        assert!(result.is_ok());
    }

    #[test]
    #[cfg(all(not(target_arch = "wasm32"), target_os = "linux"))]
    fn test_linux_keychain_probe_result_rejects_platform_failure() {
        let err = linux_keychain_probe_result(Err(keyring::Error::PlatformFailure(Box::new(
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "org.freedesktop.DBus.Error.ServiceUnknown",
            ),
        ))))
        .unwrap_err()
        .to_string();
        assert!(err.contains("keychain not available"));
        assert!(
            err.contains("ServiceUnknown"),
            "expected dbus service error in message, got: {err}"
        );
    }

    // Exercises the FileStorage fallback when the auto-detect keychain probe fails,
    // without requiring OS-level credential-store mocking.
    // On Linux this also exercises the kernel-keyring intermediate fallback (which
    // also fails because the injected probe always returns Err).
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn test_detect_backend_with_probe_failure_falls_back_to_file() {
        let _lock = crate::test_utils::ENV_LOCK.blocking_lock();
        let tmp = TempDir::new("detect_fallback");
        std::env::set_var("PUP_CONFIG_DIR", tmp.path());
        std::env::remove_var("DD_TOKEN_STORAGE");
        let backend = detect_backend_with(|| Err(anyhow::anyhow!("probe failed")));
        std::env::remove_var("PUP_CONFIG_DIR");
        assert_eq!(backend.backend_type(), BackendType::File);
    }

    // On Linux, when the Secret Service probe fails but kernel keyring succeeds,
    // the backend should be Keychain (keyutils-backed).
    #[test]
    #[cfg(all(not(target_arch = "wasm32"), target_os = "linux"))]
    fn test_detect_backend_with_secret_service_failure_falls_back_to_keyutils() {
        let _lock = crate::test_utils::ENV_LOCK.blocking_lock();
        let tmp = TempDir::new("detect_keyutils");
        std::env::set_var("PUP_CONFIG_DIR", tmp.path());
        std::env::remove_var("DD_TOKEN_STORAGE");
        let call_count = std::sync::atomic::AtomicU32::new(0);
        let backend = detect_backend_with(|| {
            let n = call_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if n == 0 {
                // First call: simulate Secret Service unavailable
                Err(anyhow::anyhow!("secret service unavailable"))
            } else {
                // Second call: keyutils backend probe succeeds
                KeychainStorage::new()
            }
        });
        std::env::remove_var("PUP_CONFIG_DIR");
        assert_eq!(backend.backend_type(), BackendType::Keychain);
    }

    // When DD_TOKEN_STORAGE=keychain is explicitly set but the backend is
    // unavailable, the process panics with a clear message rather than silently
    // falling back (explicit opt-in should fail loudly).
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn test_detect_backend_with_dd_keychain_panics_when_unavailable() {
        let _lock = crate::test_utils::ENV_LOCK.blocking_lock();
        let tmp = TempDir::new("detect_kc_panic");
        std::env::set_var("PUP_CONFIG_DIR", tmp.path());
        std::env::set_var("DD_TOKEN_STORAGE", "keychain");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            detect_backend_with(|| Err(anyhow::anyhow!("probe failed")))
        }));
        std::env::remove_var("DD_TOKEN_STORAGE");
        std::env::remove_var("PUP_CONFIG_DIR");
        assert!(
            result.is_err(),
            "expected panic when DD_TOKEN_STORAGE=keychain but keychain unavailable"
        );
    }

    #[test]
    fn test_detect_backend_dd_token_storage_file() {
        let _lock = crate::test_utils::ENV_LOCK.blocking_lock();
        let tmp = TempDir::new("detect_file");
        std::env::set_var("PUP_CONFIG_DIR", tmp.path());
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let backend = detect_backend();
        std::env::remove_var("DD_TOKEN_STORAGE");
        std::env::remove_var("PUP_CONFIG_DIR");
        assert_eq!(backend.backend_type(), BackendType::File);
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_detect_backend_windows_default_is_keychain() {
        // Windows defaults to KeychainStorage (chunked WinCred) now that the
        // chunked scheme keeps blobs within the 2560-byte platform limit.
        let _lock = crate::test_utils::ENV_LOCK.blocking_lock();
        let tmp = TempDir::new("detect_win");
        std::env::set_var("PUP_CONFIG_DIR", tmp.path());
        std::env::remove_var("DD_TOKEN_STORAGE");
        let backend = detect_backend();
        std::env::remove_var("PUP_CONFIG_DIR");
        assert_eq!(backend.backend_type(), BackendType::Keychain);
    }

    // DD_TOKEN_STORAGE=keychain on Windows should return a Keychain backend
    // (exercises the chunked WinCred scheme). Requires a functional WinCred — only
    // compiled and run on Windows CI. A negative test (broken WinCred → Err) would
    // need OS-level mocking that is not supported by this test framework.
    #[test]
    #[cfg(target_os = "windows")]
    fn test_detect_backend_dd_token_storage_keychain_windows() {
        let _lock = crate::test_utils::ENV_LOCK.blocking_lock();
        let tmp = TempDir::new("detect_kc_win");
        std::env::set_var("PUP_CONFIG_DIR", tmp.path());
        std::env::set_var("DD_TOKEN_STORAGE", "keychain");
        let backend = detect_backend();
        std::env::remove_var("DD_TOKEN_STORAGE");
        std::env::remove_var("PUP_CONFIG_DIR");
        assert_eq!(backend.backend_type(), BackendType::Keychain);
    }

    // macOS auto-detect uses the standard OS keychain (KeychainStorage).
    #[test]
    #[cfg(target_os = "macos")]
    fn test_detect_backend_macos_default_is_keychain() {
        let _lock = crate::test_utils::ENV_LOCK.blocking_lock();
        let tmp = TempDir::new("detect_macos_default");
        std::env::set_var("PUP_CONFIG_DIR", tmp.path());
        std::env::remove_var("DD_TOKEN_STORAGE");
        let backend = detect_backend();
        std::env::remove_var("PUP_CONFIG_DIR");
        assert_eq!(
            backend.storage_location(),
            "OS keychain",
            "macOS auto-detect should use KeychainStorage by default"
        );
    }

    // Config file `token_storage: file` overrides auto-detect even when the keychain
    // probe would succeed. The probe succeeds here so that without config-file support
    // the backend would be Keychain — the File result therefore proves the config was read.
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn test_detect_backend_config_file_token_storage() {
        let _lock = crate::test_utils::ENV_LOCK.blocking_lock();
        let tmp = TempDir::new("detect_cfg_file");
        std::fs::write(tmp.path().join("config.yaml"), "token_storage: file\n").unwrap();
        std::env::set_var("PUP_CONFIG_DIR", tmp.path());
        std::env::remove_var("DD_TOKEN_STORAGE");
        // Probe succeeds: without config support auto-detect would return Keychain.
        // Getting File here proves the config file was read and respected.
        let backend = detect_backend_with(KeychainStorage::new);
        std::env::remove_var("PUP_CONFIG_DIR");
        assert_eq!(
            backend.backend_type(),
            BackendType::File,
            "config file token_storage: file should win over a working keychain"
        );
    }

    // Malformed config YAML is silently ignored (no panic) and falls through to auto-detect.
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn test_detect_backend_config_file_malformed_yaml_falls_back_to_autodetect() {
        let _lock = crate::test_utils::ENV_LOCK.blocking_lock();
        let tmp = TempDir::new("detect_cfg_malformed");
        std::fs::write(
            tmp.path().join("config.yaml"),
            "token_storage: [\nbad yaml\n",
        )
        .unwrap();
        std::env::set_var("PUP_CONFIG_DIR", tmp.path());
        std::env::remove_var("DD_TOKEN_STORAGE");
        // Should not panic; malformed YAML falls through to auto-detect (probe fails → file).
        let backend = detect_backend_with(|| Err(anyhow::anyhow!("probe")));
        std::env::remove_var("PUP_CONFIG_DIR");
        assert_eq!(backend.backend_type(), BackendType::File);
    }

    // An unrecognised config file token_storage value warns and falls through to auto-detect.
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn test_detect_backend_config_file_unknown_value_falls_back_to_autodetect() {
        let _lock = crate::test_utils::ENV_LOCK.blocking_lock();
        let tmp = TempDir::new("detect_cfg_unknown");
        std::fs::write(
            tmp.path().join("config.yaml"),
            "token_storage: bogus_value\n",
        )
        .unwrap();
        std::env::set_var("PUP_CONFIG_DIR", tmp.path());
        std::env::remove_var("DD_TOKEN_STORAGE");
        let backend = detect_backend_with(|| Err(anyhow::anyhow!("probe")));
        std::env::remove_var("PUP_CONFIG_DIR");
        assert_eq!(backend.backend_type(), BackendType::File);
    }

    // An unrecognised DD_TOKEN_STORAGE env value warns and falls through to auto-detect.
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn test_detect_backend_unknown_env_value_falls_back_to_autodetect() {
        let _lock = crate::test_utils::ENV_LOCK.blocking_lock();
        let tmp = TempDir::new("detect_unknown_env");
        std::env::set_var("PUP_CONFIG_DIR", tmp.path());
        std::env::set_var("DD_TOKEN_STORAGE", "bogus_value");
        // Probe fails → should fall through all the way to file.
        let backend = detect_backend_with(|| Err(anyhow::anyhow!("probe")));
        std::env::remove_var("DD_TOKEN_STORAGE");
        std::env::remove_var("PUP_CONFIG_DIR");
        assert_eq!(backend.backend_type(), BackendType::File);
    }

    // KeychainStorage memoizes per-site state: a value placed in the cache is served
    // without touching the OS keychain. (If the cache were bypassed, load_state_raw
    // would read a nonexistent entry and return None.)
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn test_keychain_storage_load_uses_cache() {
        let store = KeychainStorage {
            cache: Mutex::new(HashMap::new()),
        };
        let mut data = SiteData::default();
        data.tokens
            .insert(DEFAULT_ORG_KEY.to_string(), make_token("cached_tok"));
        store
            .cache
            .lock()
            .unwrap()
            .insert("cache-test.example".to_string(), data);

        let got = store
            .load_tokens("cache-test.example", None)
            .unwrap()
            .expect("cached token should be returned without a keychain read");
        assert_eq!(got.access_token, "cached_tok");
    }

    // DD_TOKEN_STORAGE env var overrides config file token_storage.
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn test_detect_backend_env_overrides_config_file() {
        let _lock = crate::test_utils::ENV_LOCK.blocking_lock();
        let tmp = TempDir::new("detect_env_wins");
        // Config says "keychain" but env says "file" — env wins.
        std::fs::write(tmp.path().join("config.yaml"), "token_storage: keychain\n").unwrap();
        std::env::set_var("PUP_CONFIG_DIR", tmp.path());
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let backend = detect_backend();
        std::env::remove_var("DD_TOKEN_STORAGE");
        std::env::remove_var("PUP_CONFIG_DIR");
        assert_eq!(backend.backend_type(), BackendType::File);
    }

    // A KeychainStorage with an empty cache, bypassing the new()-time probe.
    #[cfg(target_os = "windows")]
    fn test_keychain() -> KeychainStorage {
        KeychainStorage {
            cache: Mutex::new(HashMap::new()),
        }
    }

    // Returns a token whose serialised SiteData exceeds WIN_CHUNK_BYTES (1000),
    // guaranteeing that KeychainStorage will write at least four WinCred chunks.
    // A 3000-char access token + JSON overhead ≈ 3200 bytes → 4 chunks minimum.
    #[cfg(target_os = "windows")]
    fn make_multi_chunk_token() -> TokenSet {
        make_token(&"a".repeat(3000))
    }

    // Verify that a large SiteData (above WIN_CHUNK_BYTES) round-trips correctly
    // through the chunked storage scheme.
    #[test]
    #[cfg(target_os = "windows")]
    fn test_keychain_storage_chunked_roundtrip() {
        let store = test_keychain();
        let site = "chunked_test.datadoghq.com";

        let token = make_multi_chunk_token();
        store.save_tokens(site, None, &token).unwrap();

        // Verify that the multi-chunk path was actually exercised (chunk _1 exists).
        let base = format!("state_{}", sanitize(site));
        assert!(
            keyring::Entry::new(SERVICE_NAME, &format!("{base}_1"))
                .unwrap()
                .get_password()
                .is_ok(),
            "chunk _1 must exist — payload must exceed WIN_CHUNK_BYTES"
        );

        // Clear the in-memory cache so the load actually reads back the WinCred chunks.
        store.cache.lock().unwrap().clear();
        let loaded = store.load_tokens(site, None).unwrap().unwrap();
        assert_eq!(loaded.access_token, token.access_token);

        store.delete_tokens(site, None).unwrap();
        assert!(store.load_tokens(site, None).unwrap().is_none());
    }

    // Confirm that shrinking a save (fewer chunks than the previous write) cleans
    // up the stale extra entries rather than leaving orphaned WinCred blobs.
    #[test]
    #[cfg(target_os = "windows")]
    fn test_keychain_storage_chunked_shrink_cleans_stale_entries() {
        let store = test_keychain();
        let site = "chunked_shrink.datadoghq.com";

        // First write: large token → multiple chunks.
        store
            .save_tokens(site, None, &make_multi_chunk_token())
            .unwrap();

        // Second write: tiny token → single chunk.
        store.save_tokens(site, None, &make_token("small")).unwrap();
        // Clear the in-memory cache so the load reads back from WinCred.
        store.cache.lock().unwrap().clear();
        let loaded = store.load_tokens(site, None).unwrap().unwrap();
        assert_eq!(loaded.access_token, "small");

        // Assert that the stale chunk _1 from the first write was deleted.
        let base = format!("state_{}", sanitize(site));
        assert!(
            matches!(
                keyring::Entry::new(SERVICE_NAME, &format!("{base}_1"))
                    .unwrap()
                    .get_password(),
                Err(keyring::Error::NoEntry)
            ),
            "stale chunk _1 must be removed after shrinking to a single chunk"
        );

        store.delete_tokens(site, None).unwrap();
    }

    // When a chunk entry is absent (e.g. manual WinCred deletion), load_state
    // should return empty state rather than partial data.
    #[test]
    #[cfg(target_os = "windows")]
    fn test_keychain_storage_chunked_missing_chunk_returns_default() {
        let store = test_keychain();
        let site = "chunked_missing.datadoghq.com";

        // Write a token large enough to produce at least 2 WinCred chunks.
        store
            .save_tokens(site, None, &make_multi_chunk_token())
            .unwrap();

        // Verify the payload produced at least 4 chunks before we corrupt one,
        // so this test fails explicitly if WIN_CHUNK_BYTES ever grows large enough
        // to collapse the payload back to a single chunk.
        let base = format!("state_{}", sanitize(site));
        let count: usize = keyring::Entry::new(SERVICE_NAME, &format!("{base}_c"))
            .unwrap()
            .get_password()
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert!(count >= 2, "expected at least 2 chunks, got {count}");

        // Delete chunk _1 to simulate partial WinCred corruption.
        keyring::Entry::new(SERVICE_NAME, &format!("{base}_1"))
            .unwrap()
            .delete_credential()
            .unwrap();

        // Clear the in-memory cache so the load reflects the corrupted WinCred state.
        store.cache.lock().unwrap().clear();
        // Load should return None (empty state) not partial data.
        assert!(store.load_tokens(site, None).unwrap().is_none());

        let _ = store.delete_tokens(site, None);
    }
}
