//! MCP gateway commands: list and call the tools of an organization's
//! upstream MCP servers through the gateway's REST API.
//!
//! The gateway is served on the `ai-gateway` domain, not the API host, so these
//! commands build their own URLs rather than use the `raw_client` request
//! helpers, which always target `api_base_url()`.

use anyhow::{bail, Result};

use crate::config::Config;
use crate::formatter;
use crate::raw_client;
use crate::util_ext::percent_encode;

const TOOLS_PATH: &str = "/mcp/api/tools";

const SCOPE_HINT: &str =
    "The MCP gateway needs the mcp_read OAuth scope, and mcp_write to call tools. \
     pup does not request them by default; log in with:\n  \
     pup auth login --extra-scopes mcp_read,mcp_write";

/// `ai-gateway.{site}` on Datadog sites. Literal hosts and the test mock
/// server are used verbatim, as for every other command.
fn base_url(cfg: &Config) -> String {
    #[cfg(all(not(feature = "browser"), any(debug_assertions, test)))]
    {
        if std::env::var("PUP_MOCK_SERVER").is_ok() {
            return cfg.api_base_url();
        }
    }
    if crate::settings::uses_datadog_subdomains(&cfg.site) {
        format!("https://ai-gateway.{}", cfg.site)
    } else {
        cfg.api_base_url()
    }
}

/// The gateway takes the scope as one comma-separated `slug` value.
fn slug_query(slugs: &[String]) -> Option<String> {
    let joined = slugs
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(",");
    (!joined.is_empty()).then_some(joined)
}

async fn send(
    cfg: &Config,
    method: reqwest::Method,
    path: &str,
    query: &[(&str, &str)],
    body: Option<serde_json::Value>,
) -> Result<serde_json::Value> {
    let url = format!("{}{}", base_url(cfg), path);
    let mut req = reqwest::Client::new().request(method.clone(), &url);
    if !query.is_empty() {
        req = req.query(query);
    }
    req = raw_client::apply_auth(req, cfg, method.as_str(), path)?
        .header("Accept", "application/json")
        .header("User-Agent", crate::useragent::get());
    if let Some(body) = body {
        req = req.json(&body);
    }

    let resp = req.send().await?;
    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let headers = resp.headers().clone();
        let text = resp.text().await.unwrap_or_default();
        let err = raw_client::http_error(status, method.as_str(), url, text, &headers);
        if matches!(status, 401 | 403) {
            bail!("{err}\n\n{SCOPE_HINT}");
        }
        return Err(err.into());
    }
    raw_client::parse_response_bytes(&resp.bytes().await?)
}

/// Lists one page of tools. In mux mode (no slug, or several) names are
/// `slug__tool`; with exactly one slug the server passes through under its
/// tools' own names.
pub async fn tools_list(cfg: &Config, slugs: &[String], cursor: Option<&str>) -> Result<()> {
    let scope = slug_query(slugs);
    let mut query = Vec::new();
    if let Some(scope) = scope.as_deref() {
        query.push(("slug", scope));
    }
    if let Some(cursor) = cursor {
        query.push(("cursor", cursor));
    }
    let resp = send(cfg, reqwest::Method::GET, TOOLS_PATH, &query, None).await?;
    formatter::output(cfg, &resp)
}

/// Calls a tool by the name `tools list` returned for the same slugs. A
/// result with `isError` set is the tool's own answer and is printed as is.
pub async fn tools_call(
    cfg: &Config,
    name: &str,
    slugs: &[String],
    args: Option<&str>,
    file: Option<&str>,
) -> Result<()> {
    let arguments = read_arguments(args, file)?;
    let scope = slug_query(slugs);
    let query: Vec<(&str, &str)> = scope.as_deref().map(|s| ("slug", s)).into_iter().collect();
    let path = format!("{TOOLS_PATH}/{}", percent_encode(name));
    let resp = send(cfg, reqwest::Method::POST, &path, &query, Some(arguments)).await?;
    formatter::output(cfg, &resp)
}

/// MCP requires tool arguments to be a JSON object; none means `{}`.
fn read_arguments(args: Option<&str>, file: Option<&str>) -> Result<serde_json::Value> {
    let value: serde_json::Value = match (args, file) {
        (Some(_), Some(_)) => bail!("--args and --file cannot be used together"),
        (Some(args), None) => serde_json::from_str(args)
            .map_err(|e| anyhow::anyhow!("--args is not valid JSON: {e}"))?,
        (None, Some(file)) => crate::util::read_json_file(file)?,
        (None, None) => return Ok(serde_json::json!({})),
    };
    if !value.is_object() {
        bail!("tool arguments must be a JSON object");
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use crate::test_support::*;
    use mockito::Matcher;

    const TOOLS_BODY: &str = r#"{"tools":[{"name":"github__search","inputSchema":{"type":"object"}}],"nextCursor":"abc"}"#;

    #[tokio::test]
    async fn test_tools_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let m = s
            .mock("GET", "/mcp/api/tools")
            .match_query(Matcher::Missing)
            .with_status(200)
            .with_body(TOOLS_BODY)
            .create_async()
            .await;
        let result = super::tools_list(&cfg, &[], None).await;
        assert!(result.is_ok(), "tools list failed: {:?}", result.err());
        m.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_tools_list_scoped_and_paged() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let m = s
            .mock("GET", "/mcp/api/tools")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("slug".into(), "github,jira".into()),
                Matcher::UrlEncoded("cursor".into(), "abc".into()),
            ]))
            .with_status(200)
            .with_body(TOOLS_BODY)
            .create_async()
            .await;
        let slugs = vec!["github".to_string(), " jira ".to_string(), "".to_string()];
        let result = super::tools_list(&cfg, &slugs, Some("abc")).await;
        assert!(
            result.is_ok(),
            "scoped tools list failed: {:?}",
            result.err()
        );
        m.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_tools_list_unknown_slug() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        s.mock("GET", Matcher::Any)
            .with_status(404)
            .with_body(r#"{"error":{"code":-32602,"message":"unknown server slug"}}"#)
            .create_async()
            .await;
        let err = super::tools_list(&cfg, &["nope".to_string()], None)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("unknown server slug"), "{err}");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_tools_list_forbidden_explains_scopes() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        s.mock("GET", Matcher::Any)
            .with_status(403)
            .with_body(r#"{"errors":["Forbidden"]}"#)
            .create_async()
            .await;
        let err = super::tools_list(&cfg, &[], None).await.unwrap_err();
        assert!(
            err.to_string()
                .contains("--extra-scopes mcp_read,mcp_write"),
            "{err}"
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_tools_call() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let m = s
            .mock("POST", "/mcp/api/tools/github__search")
            .match_query(Matcher::UrlEncoded("slug".into(), "github,jira".into()))
            .match_body(Matcher::Json(serde_json::json!({"q": "x"})))
            .with_status(200)
            .with_body(r#"{"content":[{"type":"text","text":"ok"}]}"#)
            .create_async()
            .await;
        let slugs = vec!["github,jira".to_string()];
        let result =
            super::tools_call(&cfg, "github__search", &slugs, Some(r#"{"q":"x"}"#), None).await;
        assert!(result.is_ok(), "tools call failed: {:?}", result.err());
        m.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_tools_call_without_arguments_sends_empty_object() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let m = s
            .mock("POST", "/mcp/api/tools/search")
            .match_query(Matcher::UrlEncoded("slug".into(), "github".into()))
            .match_body(Matcher::Json(serde_json::json!({})))
            .with_status(200)
            .with_body(r#"{"content":[]}"#)
            .create_async()
            .await;
        let result = super::tools_call(&cfg, "search", &["github".to_string()], None, None).await;
        assert!(result.is_ok(), "tools call failed: {:?}", result.err());
        m.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_tools_call_from_file() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let m = s
            .mock("POST", "/mcp/api/tools/github__search")
            .match_body(Matcher::Json(serde_json::json!({"q": "from file"})))
            .with_status(200)
            .with_body(r#"{"content":[]}"#)
            .create_async()
            .await;
        let path = write_temp_json("mcp_gateway_args.json", r#"{"q":"from file"}"#);
        let result = super::tools_call(
            &cfg,
            "github__search",
            &[],
            None,
            Some(path.to_str().unwrap()),
        )
        .await;
        let _ = std::fs::remove_file(&path);
        assert!(result.is_ok(), "tools call failed: {:?}", result.err());
        m.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_tools_call_rejects_bad_arguments_before_sending() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let m = s.mock("POST", Matcher::Any).expect(0).create_async().await;
        for args in ["[1,2]", "\"x\"", "not json"] {
            let result = super::tools_call(&cfg, "github__search", &[], Some(args), None).await;
            assert!(result.is_err(), "{args} should be rejected");
        }
        let both = super::tools_call(&cfg, "github__search", &[], Some("{}"), Some("f.json")).await;
        assert!(both.is_err(), "--args with --file should be rejected");
        m.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_tools_call_upstream_error() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        s.mock("POST", Matcher::Any)
            .with_status(502)
            .with_body(r#"{"error":{"code":-32603,"message":"upstream server failed"}}"#)
            .create_async()
            .await;
        let err = super::tools_call(&cfg, "github__search", &[], None, None)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("502"), "{err}");
        assert!(
            !err.to_string().contains("--extra-scopes"),
            "only auth failures get the scope hint"
        );
        cleanup_env();
    }

    #[test]
    fn test_base_url_uses_ai_gateway_domain() {
        let _lock = crate::test_utils::ENV_LOCK.blocking_lock();
        std::env::remove_var("PUP_MOCK_SERVER");
        let mut cfg = test_config("http://unused");
        std::env::remove_var("PUP_MOCK_SERVER");
        for (site, want) in [
            ("datadoghq.com", "https://ai-gateway.datadoghq.com"),
            ("datad0g.com", "https://ai-gateway.datad0g.com"),
            ("us3.datadoghq.com", "https://ai-gateway.us3.datadoghq.com"),
            ("datadoghq.eu", "https://ai-gateway.datadoghq.eu"),
        ] {
            cfg.site = site.into();
            assert_eq!(super::base_url(&cfg), want, "{site}");
        }
        cfg.site = "mygateway.example.com".into();
        assert_eq!(super::base_url(&cfg), "https://mygateway.example.com");
    }

    #[test]
    fn test_slug_query() {
        assert_eq!(super::slug_query(&[]), None);
        assert_eq!(super::slug_query(&[" ".into()]), None);
        assert_eq!(
            super::slug_query(&["github".into(), "jira, grafana".into()]),
            Some("github,jira, grafana".into())
        );
    }
}
