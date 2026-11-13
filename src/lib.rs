//! Browser WASM entry point for Pup — exposes a `PupClient` JS class via wasm-bindgen.
//!
//! Built with: `wasm-pack build --target web --no-default-features --features browser -- --lib`
//!
//! ```js
//! import init, { PupClient, PupClientOptions } from '@datadog/pup-wasm';
//! await init();
//!
//! const opts = new PupClientOptions('datadoghq.com');
//! opts.access_token = 'your-token';
//! const pup = new PupClient(opts);
//!
//! const monitors = await pup.monitors_list(null, 'env:prod', 50);
//! ```
//!
//! On a page served from the Datadog app domain, use the session cookie instead:
//!
//! ```js
//! const opts = new PupClientOptions('datadoghq.com');
//! opts.session_origin = location.origin;
//! opts.csrf_token = document.getElementById('_authentication_token')?.value;
//! ```
//!
//! Setting `access_token` alongside `session_origin` sends it as a Bearer token
//! to the same origin without the cookie.

#![cfg_attr(
    feature = "browser",
    allow(dead_code, unused_imports, unused_variables)
)]

#[cfg(feature = "browser")]
#[macro_use]
mod browser_io;
#[cfg(feature = "browser")]
mod api;
#[cfg(feature = "browser")]
mod auth;
#[cfg(feature = "browser")]
mod client;
#[cfg(feature = "browser")]
mod commands;
#[cfg(feature = "browser")]
mod config;
#[cfg(feature = "browser")]
mod filter;
#[cfg(feature = "browser")]
mod formatter;
#[cfg(feature = "browser")]
mod generated;
#[cfg(feature = "browser")]
mod output;
#[cfg(feature = "browser")]
mod pagination;
#[cfg(feature = "browser")]
mod rate_limit;
#[cfg(feature = "browser")]
mod raw_client;
#[cfg(feature = "browser")]
mod settings;
#[cfg(feature = "browser")]
mod useragent;
#[cfg(feature = "browser")]
mod util;
#[cfg(feature = "browser")]
mod util_ext;
#[cfg(feature = "browser")]
mod version;

#[cfg(feature = "browser")]
include!("main.rs");

#[cfg(feature = "browser")]
use wasm_bindgen::prelude::*;

#[cfg(feature = "browser")]
fn parse_files_json(
    files_json: Option<String>,
) -> Result<std::collections::HashMap<String, Vec<u8>>, JsError> {
    let Some(raw) = files_json.filter(|s| !s.is_empty()) else {
        return Ok(std::collections::HashMap::new());
    };
    let map: std::collections::HashMap<String, String> = serde_json::from_str(&raw)
        .map_err(|e| JsError::new(&format!("invalid files_json: {e}")))?;
    Ok(map.into_iter().map(|(k, v)| (k, v.into_bytes())).collect())
}

/// Execute `pup` with no credentials (offline `--help`, `version`, `agent schema`,
/// `format`, `docs api`, etc.; API commands return pup's real auth error).
#[cfg(feature = "browser")]
#[wasm_bindgen]
pub async fn run_unauthenticated_cli(
    site: String,
    args: Vec<String>,
    stdin: Option<String>,
    files_json: Option<String>,
) -> Result<JsValue, JsError> {
    let files = parse_files_json(files_json)?;
    browser_io::setup_invocation(stdin, files);
    let cfg = config::Config::from_params(site, None, None, None);
    let exit_code = run_browser_cli(args, cfg).await;
    let (stdout, stderr) = browser_io::take_output();
    to_js(&serde_json::json!({
        "exit_code": exit_code,
        "stdout": stdout,
        "stderr": stderr,
    }))
}

// ---------------------------------------------------------------------------
// PupClientOptions — JS-constructible configuration
// ---------------------------------------------------------------------------

#[cfg(feature = "browser")]
#[wasm_bindgen]
pub struct PupClientOptions {
    site: String,
    #[wasm_bindgen(getter_with_clone)]
    pub access_token: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub api_key: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub app_key: Option<String>,
    /// Same-origin base URL: the page origin (e.g. `location.origin`).
    /// Authenticates with the session cookie, or with `access_token` as a
    /// Bearer token if set. Cannot be combined with `api_key` / `app_key`.
    #[wasm_bindgen(getter_with_clone)]
    pub session_origin: Option<String>,
    /// CSRF token sent as `x-csrf-token` in session mode (needed for writes).
    #[wasm_bindgen(getter_with_clone)]
    pub csrf_token: Option<String>,
}

#[cfg(feature = "browser")]
#[wasm_bindgen]
impl PupClientOptions {
    #[wasm_bindgen(constructor)]
    pub fn new(site: &str) -> Self {
        Self {
            site: site.to_string(),
            access_token: None,
            api_key: None,
            app_key: None,
            session_origin: None,
            csrf_token: None,
        }
    }
}

// ---------------------------------------------------------------------------
// PupClient — the main API surface
// ---------------------------------------------------------------------------

#[cfg(feature = "browser")]
#[wasm_bindgen]
pub struct PupClient {
    cfg: config::Config,
}

#[cfg(feature = "browser")]
#[wasm_bindgen]
impl PupClient {
    /// Create a new PupClient from options.
    #[wasm_bindgen(constructor)]
    pub fn new(opts: PupClientOptions) -> Result<PupClient, JsError> {
        let session = match opts.session_origin {
            Some(origin) => {
                if opts.api_key.is_some() || opts.app_key.is_some() {
                    return Err(JsError::new(
                        "session_origin cannot be combined with api_key or app_key",
                    ));
                }
                let origin = settings::validate_session_origin(&origin)
                    .map_err(|e| JsError::new(&e.to_string()))?;
                Some(settings::BrowserSession {
                    origin,
                    csrf_token: opts.csrf_token,
                })
            }
            None => None,
        };
        let mut cfg =
            config::Config::from_params(opts.site, opts.access_token, opts.api_key, opts.app_key);
        cfg.browser_session = session;
        cfg.validate_auth()
            .map_err(|e| JsError::new(&e.to_string()))?;
        Ok(PupClient { cfg })
    }

    /// Return the pup library version.
    pub fn version(&self) -> String {
        version::VERSION.to_string()
    }

    // -----------------------------------------------------------------------
    // Monitors
    // -----------------------------------------------------------------------

    /// List monitors, optionally filtered by name and/or tags.
    pub async fn monitors_list(
        &self,
        name: Option<String>,
        tags: Option<String>,
        page_size: Option<i64>,
    ) -> Result<JsValue, JsError> {
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(n) = &name {
            query.push(("name", n.clone()));
        }
        if let Some(t) = &tags {
            query.push(("monitor_tags", t.clone()));
        }
        if let Some(ps) = page_size {
            query.push(("page_size", ps.to_string()));
        }
        self.do_get("/api/v1/monitor", &query).await
    }

    /// Get a single monitor by ID.
    pub async fn monitors_get(&self, monitor_id: i64) -> Result<JsValue, JsError> {
        self.do_get(&format!("/api/v1/monitor/{monitor_id}"), &[])
            .await
    }

    /// Create a monitor from a JSON body.
    pub async fn monitors_create(&self, body_json: String) -> Result<JsValue, JsError> {
        self.do_post("/api/v1/monitor", &body_json).await
    }

    /// Delete a monitor by ID.
    pub async fn monitors_delete(&self, monitor_id: i64) -> Result<JsValue, JsError> {
        self.do_delete(&format!("/api/v1/monitor/{monitor_id}"))
            .await
    }

    // -----------------------------------------------------------------------
    // Dashboards
    // -----------------------------------------------------------------------

    /// List all dashboards.
    pub async fn dashboards_list(&self) -> Result<JsValue, JsError> {
        self.do_get("/api/v1/dashboard", &[]).await
    }

    /// Get a single dashboard by ID.
    pub async fn dashboards_get(&self, dashboard_id: String) -> Result<JsValue, JsError> {
        self.do_get(&format!("/api/v1/dashboard/{dashboard_id}"), &[])
            .await
    }

    // -----------------------------------------------------------------------
    // Logs
    // -----------------------------------------------------------------------

    /// Search logs with a JSON body (v2 API).
    pub async fn logs_search(&self, body_json: String) -> Result<JsValue, JsError> {
        self.do_post("/api/v2/logs/events/search", &body_json).await
    }

    // -----------------------------------------------------------------------
    // Metrics
    // -----------------------------------------------------------------------

    /// Query timeseries metrics.
    pub async fn metrics_query(
        &self,
        query: String,
        from: i64,
        to: i64,
    ) -> Result<JsValue, JsError> {
        let q = [
            ("query", query),
            ("from", from.to_string()),
            ("to", to.to_string()),
        ];
        self.do_get("/api/v1/query", &q).await
    }

    /// List metric names, optionally filtered.
    pub async fn metrics_list(&self, filter: Option<String>) -> Result<JsValue, JsError> {
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(f) = &filter {
            query.push(("filter", f.clone()));
        }
        self.do_get("/api/v1/metrics", &query).await
    }

    // -----------------------------------------------------------------------
    // SLOs
    // -----------------------------------------------------------------------

    /// List all SLOs.
    pub async fn slos_list(&self) -> Result<JsValue, JsError> {
        self.do_get("/api/v1/slo", &[]).await
    }

    /// Get a single SLO by ID.
    pub async fn slos_get(&self, slo_id: String) -> Result<JsValue, JsError> {
        self.do_get(&format!("/api/v1/slo/{slo_id}"), &[]).await
    }

    // -----------------------------------------------------------------------
    // Incidents
    // -----------------------------------------------------------------------

    /// List incidents.
    pub async fn incidents_list(&self) -> Result<JsValue, JsError> {
        self.do_get("/api/v2/incidents", &[]).await
    }

    /// Get a single incident by ID.
    pub async fn incidents_get(&self, incident_id: String) -> Result<JsValue, JsError> {
        self.do_get(&format!("/api/v2/incidents/{incident_id}"), &[])
            .await
    }

    // -----------------------------------------------------------------------
    // Events
    // -----------------------------------------------------------------------

    /// Search events with a JSON body (v2 API).
    pub async fn events_search(&self, body_json: String) -> Result<JsValue, JsError> {
        self.do_post("/api/v2/events/search", &body_json).await
    }

    // -----------------------------------------------------------------------
    // Generic raw HTTP methods — for any endpoint
    // -----------------------------------------------------------------------

    /// Perform a raw GET request to any Datadog API path.
    pub async fn raw_get(&self, path: String) -> Result<JsValue, JsError> {
        self.do_get(&path, &[]).await
    }

    /// Perform a raw POST request with a JSON body string.
    pub async fn raw_post(&self, path: String, body_json: String) -> Result<JsValue, JsError> {
        self.do_post(&path, &body_json).await
    }

    /// Perform a raw PUT request with a JSON body string.
    pub async fn raw_put(&self, path: String, body_json: String) -> Result<JsValue, JsError> {
        self.do_put(&path, &body_json).await
    }

    /// Perform a raw PATCH request with a JSON body string.
    pub async fn raw_patch(&self, path: String, body_json: String) -> Result<JsValue, JsError> {
        self.do_patch(&path, &body_json).await
    }

    /// Perform a raw DELETE request.
    pub async fn raw_delete(&self, path: String) -> Result<JsValue, JsError> {
        self.do_delete(&path).await
    }

    /// Execute any `pup` command (reads, writes, `--jq`, `-o table`, `--file`,
    /// piped stdin) against this client's session or token configuration.
    pub async fn run_cli(
        &self,
        args: Vec<String>,
        stdin: Option<String>,
        files_json: Option<String>,
    ) -> Result<JsValue, JsError> {
        let files = parse_files_json(files_json)?;
        browser_io::setup_invocation(stdin, files);
        let exit_code = run_browser_cli(args, self.cfg.clone()).await;
        let (stdout, stderr) = browser_io::take_output();
        to_js(&serde_json::json!({
            "exit_code": exit_code,
            "stdout": stdout,
            "stderr": stderr,
        }))
    }
}

// ---------------------------------------------------------------------------
// Internal helpers (not exported to JS)
// ---------------------------------------------------------------------------

#[cfg(feature = "browser")]
impl PupClient {
    async fn do_get(&self, path: &str, query: &[(&str, String)]) -> Result<JsValue, JsError> {
        let val = api::get(&self.cfg, path, query)
            .await
            .map_err(|e| JsError::new(&e.to_string()))?;
        to_js(&val)
    }

    async fn do_post(&self, path: &str, body_json: &str) -> Result<JsValue, JsError> {
        let body: serde_json::Value = serde_json::from_str(body_json)
            .map_err(|e| JsError::new(&format!("invalid JSON body: {e}")))?;
        let val = api::post(&self.cfg, path, &body)
            .await
            .map_err(|e| JsError::new(&e.to_string()))?;
        to_js(&val)
    }

    async fn do_put(&self, path: &str, body_json: &str) -> Result<JsValue, JsError> {
        let body: serde_json::Value = serde_json::from_str(body_json)
            .map_err(|e| JsError::new(&format!("invalid JSON body: {e}")))?;
        let val = api::put(&self.cfg, path, &body)
            .await
            .map_err(|e| JsError::new(&e.to_string()))?;
        to_js(&val)
    }

    async fn do_patch(&self, path: &str, body_json: &str) -> Result<JsValue, JsError> {
        let body: serde_json::Value = serde_json::from_str(body_json)
            .map_err(|e| JsError::new(&format!("invalid JSON body: {e}")))?;
        let val = api::patch(&self.cfg, path, &body)
            .await
            .map_err(|e| JsError::new(&e.to_string()))?;
        to_js(&val)
    }

    async fn do_delete(&self, path: &str) -> Result<JsValue, JsError> {
        let val = api::delete(&self.cfg, path)
            .await
            .map_err(|e| JsError::new(&e.to_string()))?;
        to_js(&val)
    }
}

/// Convert a serde_json::Value to a plain JS value via `JSON.parse`, matching
/// what `await response.json()` returns: objects (not `Map`s), and integers
/// above 2^53 (e.g. log IDs) rounded like any JS client instead of erroring.
#[cfg(feature = "browser")]
fn to_js(val: &serde_json::Value) -> Result<JsValue, JsError> {
    let text = serde_json::to_string(val)
        .map_err(|e| JsError::new(&format!("serialization error: {e}")))?;
    js_sys::JSON::parse(&text).map_err(|_| JsError::new("serialization error: JSON.parse failed"))
}
