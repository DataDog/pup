#[cfg(not(target_arch = "wasm32"))]
use anyhow::bail;
use anyhow::Result;
#[cfg(not(target_arch = "wasm32"))]
use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[cfg(not(target_arch = "wasm32"))]
use tokio::sync::oneshot;

#[cfg(not(target_arch = "wasm32"))]
use super::dcr::DCR_REDIRECT_PORTS;

#[cfg(not(target_arch = "wasm32"))]
pub struct CallbackResult {
    pub code: String,
    pub state: String,
    pub error: Option<String>,
    pub error_description: Option<String>,
    /// Authoritative org UUID from the OAuth server (`dd_oid`).
    pub dd_oid: Option<String>,
    /// Display name of the consented org (`dd_org_name`).
    pub dd_org_name: Option<String>,
    /// Actual Datadog site/region from the OAuth callback (`domain`).
    pub domain: Option<String>,
}

#[cfg(not(target_arch = "wasm32"))]
pub struct CallbackServer {
    port: u16,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl CallbackServer {
    /// Prepare the callback server.
    ///
    /// When `pinned_port` is `Some`, bind exactly that port and error out if
    /// it's busy — no fallback. This is the deterministic mode used by
    /// `--callback-port` / `PUP_OAUTH_CALLBACK_PORT`, where users have set up
    /// SSH port forwarding for one specific port and a silent fallback to
    /// another port would orphan the OAuth callback.
    ///
    /// When `pinned_port` is `None`, scan `DCR_REDIRECT_PORTS` first-available-wins.
    pub async fn new(pinned_port: Option<u16>) -> Result<Self> {
        if let Some(port) = pinned_port {
            // Defense-in-depth: callers (currently `commands::auth::login` via
            // `resolve_callback_port`) restrict pinned ports to the DCR
            // allowlist. If a future caller bypasses that path, fail in debug
            // builds rather than silently bind a port the OAuth server hasn't
            // registered as a redirect URI.
            debug_assert!(
                DCR_REDIRECT_PORTS.contains(&port),
                "pinned callback port {port} is not in DCR_REDIRECT_PORTS {DCR_REDIRECT_PORTS:?}"
            );
            return tokio::net::TcpListener::bind(("127.0.0.1", port))
                .await
                .map(|_| Self {
                    port,
                    shutdown_tx: None,
                })
                .map_err(|e| {
                    anyhow::anyhow!(
                        "could not bind requested callback port {port}: {e}. \
                         Pick an unused port or omit --callback-port to scan {DCR_REDIRECT_PORTS:?}."
                    )
                });
        }

        for &port in DCR_REDIRECT_PORTS {
            if tokio::net::TcpListener::bind(("127.0.0.1", port))
                .await
                .is_ok()
            {
                return Ok(Self {
                    port,
                    shutdown_tx: None,
                });
            }
        }
        bail!(
            "could not bind to any DCR redirect port ({:?})",
            DCR_REDIRECT_PORTS
        );
    }

    #[allow(dead_code)]
    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn redirect_uri(&self) -> String {
        format!("http://127.0.0.1:{}/oauth/callback", self.port)
    }

    /// Start the server and wait for the OAuth callback.
    pub async fn wait_for_callback(
        &mut self,
        timeout: std::time::Duration,
    ) -> Result<CallbackResult> {
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let (result_tx, result_rx) = oneshot::channel::<CallbackResult>();
        self.shutdown_tx = Some(shutdown_tx);

        let port = self.port;
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;

        tokio::spawn(async move {
            let result_tx = std::sync::Mutex::new(Some(result_tx));
            tokio::select! {
                _ = accept_loop(listener, result_tx) => {}
                _ = shutdown_rx => {}
            }
        });

        match tokio::time::timeout(timeout, result_rx).await {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(_)) => bail!("callback channel closed unexpectedly"),
            Err(_) => bail!("OAuth callback timed out after {timeout:?}"),
        }
    }

    pub fn stop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for CallbackServer {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn accept_loop(
    listener: tokio::net::TcpListener,
    result_tx: std::sync::Mutex<Option<oneshot::Sender<CallbackResult>>>,
) {
    loop {
        let Ok((mut stream, _)) = listener.accept().await else {
            return;
        };

        let mut buf = vec![0u8; 4096];
        let Ok(n) = stream.read(&mut buf).await else {
            continue;
        };

        let request = String::from_utf8_lossy(&buf[..n]);
        let Some(path_line) = request.lines().next() else {
            continue;
        };
        let parts: Vec<&str> = path_line.split_whitespace().collect();
        if parts.len() < 2 || !parts[1].starts_with("/oauth/callback") {
            let response = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n";
            let _ = stream.write_all(response.as_bytes()).await;
            continue;
        }

        let query_string = parts[1].split('?').nth(1).unwrap_or("");
        let params: std::collections::HashMap<String, String> =
            url::form_urlencoded::parse(query_string.as_bytes())
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();

        let code = params.get("code").cloned().unwrap_or_default();
        let state = params.get("state").cloned().unwrap_or_default();
        let error = params.get("error").cloned();
        let error_description = params.get("error_description").cloned();
        let dd_oid = params.get("dd_oid").cloned();
        let dd_org_name = params.get("dd_org_name").cloned();
        // `site` (full URL) and `domain` (bare host) are sent alongside the
        // OAuth response so the success page can render a link back.
        let site = params.get("site").cloned();
        let domain = params.get("domain").cloned();

        let (status, body) = if error.is_some() {
            ("400 Bad Request", error_page(&error, &error_description))
        } else {
            (
                "200 OK",
                success_page(
                    site.as_deref(),
                    domain.as_deref(),
                    dd_org_name.as_deref(),
                    dd_oid.as_deref(),
                ),
            )
        };
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes()).await;

        let result = CallbackResult {
            code,
            state,
            error,
            error_description,
            dd_oid,
            dd_org_name,
            domain,
        };
        if let Some(tx) = result_tx.lock().unwrap().take() {
            let _ = tx.send(result);
        }
        return;
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn success_page(
    site: Option<&str>,
    domain: Option<&str>,
    org_name: Option<&str>,
    org_uuid: Option<&str>,
) -> String {
    // Only accept https `site` as the link href; anything else falls back to
    // plain text so a malformed or hostile callback can't inject a different
    // scheme into the rendered HTML.
    let provider_html = match (site.filter(|s| s.starts_with("https://")), domain) {
        (Some(href), Some(host)) => format!(
            r#"Datadog (<a href="{}">{}</a>)"#,
            html_escape(href),
            html_escape(host),
        ),
        (Some(href), None) => format!(r#"<a href="{0}">{0}</a>"#, html_escape(href)),
        (None, Some(host)) => format!("Datadog ({})", html_escape(host)),
        (None, None) => "Datadog".to_string(),
    };

    let org_line = org_name
        .map(|n| {
            format!(
                r#"<p class="org">in org <strong>{}</strong></p>"#,
                html_escape(n)
            )
        })
        .unwrap_or_default();
    let uuid_line = org_uuid
        .map(|u| format!(r#"<p class="uuid">({})</p>"#, html_escape(u)))
        .unwrap_or_default();

    format!(
        r#"<!DOCTYPE html>
<html><head><meta charset="utf-8"><title>Pup - Authentication Successful</title>
<style>body{{font-family:system-ui;display:flex;justify-content:center;align-items:center;min-height:100vh;margin:0;padding:1rem;background:#f5f5f5;box-sizing:border-box}}
.card{{background:white;padding:2rem;border-radius:8px;box-shadow:0 2px 4px rgba(0,0,0,0.1);text-align:center;max-width:32rem}}
h1{{color:#632ca6;margin:0 0 .5rem}}
p{{color:#555;margin:.4rem 0}}
.org{{margin-top:.2rem}}
.uuid{{margin-top:.75rem;color:#999;font-size:.75em;font-family:ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;letter-spacing:.01em;word-break:break-all}}
a{{color:#632ca6}}
.close{{margin-top:1.25rem;color:#777;font-size:.9em}}</style></head>
<body><div class="card"><h1>Authentication Successful</h1>
<p>Connected to {provider_html}</p>
{org_line}{uuid_line}<p class="close">You can close this window and return to pup.</p></div></body></html>"#
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(not(target_arch = "wasm32"))]
fn error_page(error: &Option<String>, desc: &Option<String>) -> String {
    let err = error.as_deref().unwrap_or("unknown_error");
    let desc = desc.as_deref().unwrap_or("An unknown error occurred.");
    format!(
        r#"<!DOCTYPE html>
<html><head><meta charset="utf-8"><title>Pup - Authentication Failed</title>
<style>body{{font-family:system-ui;display:flex;justify-content:center;align-items:center;height:100vh;margin:0;background:#f5f5f5}}
.card{{background:white;padding:2rem;border-radius:8px;box-shadow:0 2px 4px rgba(0,0,0,0.1);text-align:center}}
h1{{color:#c00}}p{{color:#555}}</style></head>
<body><div class="card"><h1>Authentication Failed</h1>
<p><strong>{err}</strong></p><p>{desc}</p>
<p>Please close this window and try again.</p></div></body></html>"#
    )
}

/// Parse a pasted OAuth callback URL into a `CallbackResult`. Used as a
/// fallback for users on remote machines where the laptop browser cannot
/// reach the workspace's `127.0.0.1` listener.
#[cfg(not(target_arch = "wasm32"))]
fn parse_callback_url(input: &str) -> Result<CallbackResult> {
    let url = url::Url::parse(input.trim()).map_err(|e| anyhow::anyhow!("not a valid URL: {e}"))?;
    let mut code = None;
    let mut state = None;
    let mut error = None;
    let mut error_description = None;
    let mut dd_oid = None;
    let mut dd_org_name = None;
    let mut domain = None;
    for (k, v) in url.query_pairs() {
        match k.as_ref() {
            "code" => code = Some(v.into_owned()),
            "state" => state = Some(v.into_owned()),
            "error" => error = Some(v.into_owned()),
            "error_description" => error_description = Some(v.into_owned()),
            "dd_oid" => dd_oid = Some(v.into_owned()),
            "dd_org_name" => dd_org_name = Some(v.into_owned()),
            "domain" => domain = Some(v.into_owned()),
            _ => {}
        }
    }
    if error.is_none() && (code.is_none() || state.is_none()) {
        bail!("URL is missing 'code' and 'state' query parameters");
    }
    Ok(CallbackResult {
        code: code.unwrap_or_default(),
        state: state.unwrap_or_default(),
        error,
        error_description,
        dd_oid,
        dd_org_name,
        domain,
    })
}

/// Read pasted callback URLs from stdin until one parses, then return it.
/// Thin wrapper around `read_callback_url_from_reader` so the loop logic
/// stays unit-testable against a synthetic reader.
#[cfg(not(target_arch = "wasm32"))]
pub async fn read_callback_url_from_stdin() -> Result<CallbackResult> {
    read_callback_url_from_reader(tokio::io::BufReader::new(tokio::io::stdin())).await
}

/// Read pasted callback URLs from `reader` until one parses, then return it.
/// Errors are printed and the loop continues, so a typo doesn't end the
/// login session: the HTTP listener may still fire.
///
/// On EOF without a valid URL the future stays pending forever rather than
/// resolving to an error. This matters when the function is raced against
/// the HTTP listener via `tokio::select!`: a closed or piped stdin must not
/// short-circuit the HTTP branch.
#[cfg(not(target_arch = "wasm32"))]
async fn read_callback_url_from_reader<R: tokio::io::AsyncBufRead + Unpin>(
    reader: R,
) -> Result<CallbackResult> {
    use tokio::io::AsyncBufReadExt;
    let mut lines = reader.lines();
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        match parse_callback_url(&line) {
            Ok(result) => return Ok(result),
            Err(e) => eprintln!("⚠️  {e}. Paste the full callback URL again:"),
        }
    }
    std::future::pending().await
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn parse_callback_url_extracts_code_and_state() {
        let r = parse_callback_url("http://127.0.0.1:8000/oauth/callback?code=abc123&state=xyz789")
            .unwrap();
        assert_eq!(r.code, "abc123");
        assert_eq!(r.state, "xyz789");
        assert!(r.error.is_none());
        assert!(r.error_description.is_none());
        assert!(r.dd_oid.is_none());
        assert!(r.dd_org_name.is_none());
        assert!(r.domain.is_none());
    }

    #[test]
    fn parse_callback_url_extracts_dd_oid_org_name_and_domain() {
        // Real callback shape from a Datadog OAuth flow — the issuer appends
        // dd_oid, (URL-encoded) dd_org_name, and domain alongside code and state.
        let r = parse_callback_url(
            "http://127.0.0.1:8000/oauth/callback\
             ?code=abc&state=xyz\
             &dd_oid=00000000-1111-2222-3333-444444444444\
             &dd_org_name=Datadog+HQ\
             &domain=us3.datadoghq.com",
        )
        .unwrap();
        assert_eq!(
            r.dd_oid.as_deref(),
            Some("00000000-1111-2222-3333-444444444444")
        );
        assert_eq!(r.dd_org_name.as_deref(), Some("Datadog HQ"));
        assert_eq!(r.domain.as_deref(), Some("us3.datadoghq.com"));
    }

    #[test]
    fn parse_callback_url_extracts_error() {
        let r = parse_callback_url(
            "http://127.0.0.1:8000/oauth/callback?error=access_denied&error_description=user%20cancelled",
        )
        .unwrap();
        assert_eq!(r.error.as_deref(), Some("access_denied"));
        assert_eq!(r.error_description.as_deref(), Some("user cancelled"));
    }

    #[test]
    fn parse_callback_url_trims_whitespace() {
        let r = parse_callback_url("  http://127.0.0.1:8000/oauth/callback?code=abc&state=xyz\n")
            .unwrap();
        assert_eq!(r.code, "abc");
        assert_eq!(r.state, "xyz");
    }

    #[test]
    fn parse_callback_url_rejects_missing_params() {
        assert!(parse_callback_url("http://127.0.0.1:8000/oauth/callback").is_err());
        assert!(parse_callback_url("http://127.0.0.1:8000/oauth/callback?code=abc").is_err());
        assert!(parse_callback_url("http://127.0.0.1:8000/oauth/callback?state=xyz").is_err());
    }

    #[test]
    fn parse_callback_url_rejects_garbage() {
        assert!(parse_callback_url("not a url").is_err());
        assert!(parse_callback_url("").is_err());
    }

    #[test]
    fn parse_callback_url_accepts_any_host() {
        // Tolerant of broker-style or non-localhost redirect URIs as long as
        // the query carries the right params.
        let r = parse_callback_url("https://oauth.example.com/cli/callback?code=abc&state=xyz")
            .unwrap();
        assert_eq!(r.code, "abc");
        assert_eq!(r.state, "xyz");
    }

    fn reader(input: &str) -> tokio::io::BufReader<&[u8]> {
        tokio::io::BufReader::new(input.as_bytes())
    }

    #[tokio::test]
    async fn read_callback_url_returns_first_valid_line() {
        let r = read_callback_url_from_reader(reader(
            "http://127.0.0.1:8000/oauth/callback?code=abc&state=xyz\n",
        ))
        .await
        .unwrap();
        assert_eq!(r.code, "abc");
        assert_eq!(r.state, "xyz");
    }

    #[tokio::test]
    async fn read_callback_url_skips_blank_lines() {
        let r = read_callback_url_from_reader(reader(
            "\n\n   \nhttp://127.0.0.1:8000/oauth/callback?code=abc&state=xyz\n",
        ))
        .await
        .unwrap();
        assert_eq!(r.code, "abc");
    }

    #[tokio::test]
    async fn read_callback_url_loops_through_parse_errors_until_valid() {
        // Garbage and a half-complete URL precede the valid one; the loop
        // must keep going until a parse succeeds, not bail on first error.
        let r = read_callback_url_from_reader(reader(
            "not a url\n\
             http://127.0.0.1:8000/oauth/callback?code=alpha\n\
             http://127.0.0.1:8000/oauth/callback?code=beta&state=charlie\n",
        ))
        .await
        .unwrap();
        assert_eq!(r.code, "beta");
        assert_eq!(r.state, "charlie");
    }

    #[tokio::test]
    async fn read_callback_url_stays_pending_on_eof_without_match() {
        // Reader closes after delivering only garbage. The future must NOT
        // resolve to an error — that would let it short-circuit a `select!`
        // race against the HTTP listener. Verify by timing out.
        let fut = read_callback_url_from_reader(reader("garbage\nmore garbage\n"));
        let timed = tokio::time::timeout(std::time::Duration::from_millis(50), fut).await;
        assert!(
            timed.is_err(),
            "expected pending (timeout), but future resolved"
        );
    }

    #[tokio::test]
    async fn read_callback_url_stays_pending_on_immediate_eof() {
        // Closed/empty stdin (ex: `cmd </dev/null`) must not short-circuit.
        let fut = read_callback_url_from_reader(reader(""));
        let timed = tokio::time::timeout(std::time::Duration::from_millis(50), fut).await;
        assert!(
            timed.is_err(),
            "expected pending (timeout), but future resolved"
        );
    }

    /// Find a DCR-allowlist port that's currently free. Returns `None` if all
    /// four are in use, in which case a pinned-port test should skip rather
    /// than fail — the production caller would see the same condition and
    /// surface a real error.
    async fn find_free_dcr_port() -> Option<u16> {
        for &p in DCR_REDIRECT_PORTS {
            if let Ok(l) = tokio::net::TcpListener::bind(("127.0.0.1", p)).await {
                drop(l);
                return Some(p);
            }
        }
        None
    }

    #[tokio::test]
    async fn callback_server_pinned_port_binds_exact_port() {
        // Pinned mode must use the exact requested port and surface it via
        // redirect_uri(). Use a free DCR allowlist port so the debug_assert
        // in CallbackServer::new is satisfied — production callers always
        // pass an allowlist port via resolve_callback_port.
        let Some(port) = find_free_dcr_port().await else {
            // All four DCR ports busy on this machine; nothing to test.
            return;
        };
        let server = CallbackServer::new(Some(port)).await.unwrap();
        assert_eq!(server.port(), port);
        assert_eq!(
            server.redirect_uri(),
            format!("http://127.0.0.1:{port}/oauth/callback")
        );
    }

    #[tokio::test]
    async fn callback_server_pinned_port_errors_when_busy() {
        // If the user pinned a port and we can't bind it, fail loudly — silently
        // falling through to the scan list defeats the purpose of the flag for
        // the SSH-tunneled workflow.
        let Some(port) = find_free_dcr_port().await else {
            return;
        };
        let blocker = tokio::net::TcpListener::bind(("127.0.0.1", port))
            .await
            .unwrap();

        let err = match CallbackServer::new(Some(port)).await {
            Ok(_) => panic!("expected bind to fail on busy port {port}"),
            Err(e) => e,
        };
        // Keep blocker alive past the bind attempt so the port stays busy
        // for the duration of CallbackServer::new(). Drop only after we've
        // captured the error.
        drop(blocker);
        let msg = format!("{err}");
        assert!(
            msg.contains(&port.to_string()),
            "error should name the requested port: {msg}"
        );
        assert!(
            msg.contains("--callback-port"),
            "error should hint at the flag: {msg}"
        );
    }

    #[test]
    fn success_page_renders_org_name_and_uuid() {
        let html = success_page(
            Some("https://app.tatooine.example"),
            Some("tatooine.example"),
            Some("Mos Eisley Cantina"),
            Some("00000000-1111-2222-3333-444444444444"),
        );
        assert!(html.contains("Authentication Successful"));
        assert!(html.contains("Connected to Datadog"));
        assert!(html.contains(r#"href="https://app.tatooine.example""#));
        assert!(html.contains("Mos Eisley Cantina"));
        assert!(html.contains("in org"));
        // UUID is rendered inline as fine-print so operators can read or copy
        // it without any interaction.
        assert!(html.contains(r#"<p class="uuid""#));
        assert!(html.contains("00000000-1111-2222-3333-444444444444"));
    }

    #[test]
    fn success_page_omits_detail_when_callback_lacks_metadata() {
        // Older issuers (or unusual flows) may not emit dd_org_name/dd_oid/site/domain;
        // the page should still render cleanly without dangling labels.
        let html = success_page(None, None, None, None);
        assert!(html.contains("Authentication Successful"));
        assert!(html.contains("Connected to Datadog"));
        assert!(!html.contains("in org"), "no org name -> no 'in org' line");
        assert!(
            !html.contains(r#"<p class="uuid""#),
            "no uuid -> no fine-print line"
        );
    }

    #[test]
    fn success_page_escapes_org_name() {
        // Defense in depth: a hostile dd_org_name shouldn't be able to inject
        // script tags into the success page rendered by the local listener.
        let html = success_page(None, None, Some("<script>alert(1)</script>"), None);
        assert!(!html.contains("<script>alert(1)</script>"));
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
    }

    #[test]
    fn success_page_rejects_non_https_site_as_link() {
        // An unexpected `site` value (anything other than https://...) must not
        // become a clickable link. The bare host falls back to plain text.
        let html = success_page(
            Some("javascript:alert(1)"),
            Some("deathstar.tatooine.example"),
            None,
            None,
        );
        assert!(!html.contains("javascript:alert(1)"));
        assert!(!html.contains("href=\"javascript"));
        assert!(html.contains("deathstar.tatooine.example"));
    }

    #[test]
    fn success_page_uses_vanity_subdomain_in_link() {
        // SAML/SSO vanity subdomains route through their own host; the link
        // should reflect whatever site the issuer reported, not always app.<site>.
        let html = success_page(
            Some("https://yavin4.tatooine.example"),
            Some("tatooine.example"),
            Some("Rebel Alliance"),
            None,
        );
        assert!(html.contains(r#"href="https://yavin4.tatooine.example""#));
        assert!(html.contains("Rebel Alliance"));
    }

    #[tokio::test]
    async fn read_callback_url_passes_through_oauth_error_redirect() {
        let r = read_callback_url_from_reader(reader(
            "http://127.0.0.1:8000/oauth/callback?error=access_denied&error_description=denied\n",
        ))
        .await
        .unwrap();
        assert_eq!(r.error.as_deref(), Some("access_denied"));
        assert_eq!(r.error_description.as_deref(), Some("denied"));
    }
}
