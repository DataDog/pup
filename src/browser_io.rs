//! In-memory stdio and virtual file buffers for the `browser` WASM build.
//!
//! Included with `#[macro_use]` at the crate root (`src/lib.rs`) when compiled
//! with `--features browser`, so unqualified `print!`, `println!`, `eprint!`,
//! and `eprintln!` calls across `output.rs`, `commands/*.rs`, and `main.rs`
//! capture into per-invocation strings without touching native/WASI builds.

use async_trait::async_trait;
use http::Extensions;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware, Middleware, Next};
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{self, Cursor, Write};
use std::time::Duration;
use wasm_bindgen::JsCast;

use crate::config::Config;

static CLI_INVOCATION_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

thread_local! {
    static STDOUT_BUF: RefCell<String> = const { RefCell::new(String::new()) };
    static STDERR_BUF: RefCell<String> = const { RefCell::new(String::new()) };
    static STDIN_BUF: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static FILES_MAP: RefCell<HashMap<String, Vec<u8>>> = RefCell::new(HashMap::new());
}

pub async fn lock_invocation() -> tokio::sync::MutexGuard<'static, ()> {
    CLI_INVOCATION_LOCK.lock().await
}

pub fn push_stdout(text: &str) {
    STDOUT_BUF.with(|buf| buf.borrow_mut().push_str(text));
}

pub fn push_stderr(text: &str) {
    STDERR_BUF.with(|buf| buf.borrow_mut().push_str(text));
}

pub fn setup_invocation(stdin: Option<String>, files: HashMap<String, Vec<u8>>) {
    STDOUT_BUF.with(|buf| buf.borrow_mut().clear());
    STDERR_BUF.with(|buf| buf.borrow_mut().clear());
    STDIN_BUF.with(|buf| {
        *buf.borrow_mut() = stdin.map(String::into_bytes).unwrap_or_default();
    });
    FILES_MAP.with(|map| {
        *map.borrow_mut() = files;
    });
    crate::rate_limit::store_last(None);
}

pub fn take_output() -> (String, String) {
    let out = STDOUT_BUF.with(|buf| std::mem::take(&mut *buf.borrow_mut()));
    let err = STDERR_BUF.with(|buf| std::mem::take(&mut *buf.borrow_mut()));
    STDIN_BUF.with(|buf| buf.borrow_mut().clear());
    FILES_MAP.with(|map| map.borrow_mut().clear());
    (out, err)
}

pub fn stdin_cursor() -> Cursor<Vec<u8>> {
    let bytes = STDIN_BUF.with(|buf| buf.borrow().clone());
    Cursor::new(bytes)
}

pub fn read_file_bytes(path: &str) -> io::Result<Vec<u8>> {
    let normalized = path.strip_prefix("./").unwrap_or(path);
    FILES_MAP.with(|map| {
        let map = map.borrow();
        if let Some(bytes) = map.get(path).or_else(|| map.get(normalized)) {
            return Ok(bytes.clone());
        }
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("no such file in browser shell: {path}"),
        ))
    })
}

pub fn read_file_string(path: &str) -> io::Result<String> {
    let bytes = read_file_bytes(path)?;
    String::from_utf8(bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Monotonic timestamp in milliseconds from `globalThis.performance.now()`,
/// falling back to `Date.now()` in JS realms without `performance`.
pub fn monotonic_now_ms() -> f64 {
    let global = js_sys::global();
    if let Ok(perf) = js_sys::Reflect::get(&global, &wasm_bindgen::JsValue::from_str("performance"))
    {
        if let Ok(now_fn) = js_sys::Reflect::get(&perf, &wasm_bindgen::JsValue::from_str("now")) {
            if let Some(func) = now_fn.dyn_ref::<js_sys::Function>() {
                if let Ok(val) = func.call0(&perf) {
                    if let Some(ms) = val.as_f64() {
                        return ms;
                    }
                }
            }
        }
    }
    js_sys::Date::now()
}

/// Non-blocking timer sleep backed by `globalThis.setTimeout`.
pub async fn sleep(duration: Duration) {
    let millis = duration.as_millis().min(i32::MAX as u128) as i32;
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        let global = js_sys::global();
        if let Ok(val) =
            js_sys::Reflect::get(&global, &wasm_bindgen::JsValue::from_str("setTimeout"))
        {
            if let Some(set_timeout) = val.dyn_ref::<js_sys::Function>() {
                if set_timeout
                    .call2(&global, &resolve, &wasm_bindgen::JsValue::from(millis))
                    .is_ok()
                {
                    return;
                }
            }
        }
        let _ = resolve.call0(&wasm_bindgen::JsValue::UNDEFINED);
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}

pub struct StdoutWriter;

impl Write for StdoutWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let s = String::from_utf8_lossy(buf);
        push_stdout(&s);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct BrowserBearerAuthMiddleware {
    token: String,
}

#[async_trait(?Send)]
impl Middleware for BrowserBearerAuthMiddleware {
    async fn handle(
        &self,
        mut req: reqwest_middleware::reqwest::Request,
        extensions: &mut Extensions,
        next: Next<'_>,
    ) -> reqwest_middleware::Result<reqwest_middleware::reqwest::Response> {
        req.headers_mut().insert(
            reqwest_middleware::reqwest::header::AUTHORIZATION,
            format!("Bearer {}", self.token).parse().unwrap(),
        );
        next.run(req, extensions).await
    }
}

struct BrowserSessionMiddleware {
    access_token: Option<String>,
    csrf_token: Option<String>,
}

#[async_trait(?Send)]
impl Middleware for BrowserSessionMiddleware {
    async fn handle(
        &self,
        req: reqwest_middleware::reqwest::Request,
        extensions: &mut Extensions,
        next: Next<'_>,
    ) -> reqwest_middleware::Result<reqwest_middleware::reqwest::Response> {
        let client = reqwest_middleware::reqwest::Client::new();
        let builder = reqwest_middleware::reqwest::RequestBuilder::from_parts(client, req);
        let builder = if let Some(token) = &self.access_token {
            builder.fetch_credentials_omit().header(
                reqwest_middleware::reqwest::header::AUTHORIZATION,
                format!("Bearer {token}"),
            )
        } else {
            let b = builder.fetch_credentials_same_origin();
            match &self.csrf_token {
                Some(csrf) => b.header("x-csrf-token", csrf.as_str()),
                None => b,
            }
        };
        let rebuilt = builder
            .build()
            .map_err(reqwest_middleware::Error::Reqwest)?;
        next.run(rebuilt, extensions).await
    }
}

struct BrowserRateLimitCaptureMiddleware;

#[async_trait(?Send)]
impl Middleware for BrowserRateLimitCaptureMiddleware {
    async fn handle(
        &self,
        req: reqwest_middleware::reqwest::Request,
        extensions: &mut Extensions,
        next: Next<'_>,
    ) -> reqwest_middleware::Result<reqwest_middleware::reqwest::Response> {
        let resp = next.run(req, extensions).await?;
        crate::rate_limit::store_last(crate::rate_limit::extract_from_headers(resp.headers()));
        Ok(resp)
    }
}

/// Builds the browser WASM `reqwest_middleware` client for SDK API calls
/// (including spec-generated commands invoked via `crate::make_api!`).
pub fn make_dd_client(cfg: &Config, send_bearer: bool) -> Option<ClientWithMiddleware> {
    let reqwest_client = reqwest_middleware::reqwest::Client::new();
    let mut builder = ClientBuilder::new(reqwest_client).with(BrowserRateLimitCaptureMiddleware);
    if let Some(session) = &cfg.browser_session {
        builder = builder.with(BrowserSessionMiddleware {
            access_token: cfg.access_token.clone().filter(|_| send_bearer),
            csrf_token: session.csrf_token.clone(),
        });
    } else if send_bearer {
        if let Some(token) = cfg.access_token.as_ref() {
            builder = builder.with(BrowserBearerAuthMiddleware {
                token: token.clone(),
            });
        }
    }
    Some(builder.build())
}

macro_rules! print {
    ($($arg:tt)*) => {{
        $crate::browser_io::push_stdout(&::std::format!($($arg)*));
    }};
}

macro_rules! println {
    () => {{
        $crate::browser_io::push_stdout("\n");
    }};
    ($($arg:tt)*) => {{
        let mut s = ::std::format!($($arg)*);
        s.push('\n');
        $crate::browser_io::push_stdout(&s);
    }};
}

macro_rules! eprint {
    ($($arg:tt)*) => {{
        $crate::browser_io::push_stderr(&::std::format!($($arg)*));
    }};
}

macro_rules! eprintln {
    () => {{
        $crate::browser_io::push_stderr("\n");
    }};
    ($($arg:tt)*) => {{
        let mut s = ::std::format!($($arg)*);
        s.push('\n');
        $crate::browser_io::push_stderr(&s);
    }};
}
