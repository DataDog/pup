//! Post-setup verification: confirm that telemetry for a service is arriving.

use std::time::{Duration, Instant};

use anyhow::{bail, Result};
use serde_json::{json, Value};

use crate::config::{auth_host_for, Config};
use crate::formatter;
use crate::raw_client;
use crate::util_ext;

const POLL_INTERVAL: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Product {
    Apm,
    Logs,
    Rum,
}

impl Product {
    fn name(self) -> &'static str {
        match self {
            Product::Apm => "apm",
            Product::Logs => "logs",
            Product::Rum => "rum",
        }
    }

    fn aggregate_path(self) -> &'static str {
        match self {
            Product::Apm => "/api/v2/spans/analytics/aggregate",
            Product::Logs => "/api/v2/logs/analytics/aggregate",
            Product::Rum => "/api/v2/rum/analytics/aggregate",
        }
    }

    fn explorer_path(self) -> &'static str {
        match self {
            Product::Apm => "/apm/traces",
            Product::Logs => "/logs",
            Product::Rum => "/rum/sessions",
        }
    }

    fn aggregate_body(self, query: &str, from_ms: i64, to_ms: i64) -> Value {
        let filter = json!({
            "query": query,
            "from": from_ms.to_string(),
            "to": to_ms.to_string(),
        });
        match self {
            Product::Apm => json!({
                "data": {
                    "type": "aggregate_request",
                    "attributes": {
                        "compute": [{"aggregation": "count"}],
                        "filter": filter,
                    },
                },
            }),
            Product::Logs | Product::Rum => json!({
                "compute": [{"aggregation": "count", "type": "total"}],
                "filter": filter,
            }),
        }
    }

    // Spans return a list of buckets keyed by `compute`; logs and RUM return
    // `data.buckets[].computes`. An empty bucket list means zero matches.
    fn extract_count(self, resp: &Value) -> u64 {
        let computes = match self {
            Product::Apm => resp.pointer("/data/0/attributes/compute"),
            Product::Logs | Product::Rum => resp.pointer("/data/buckets/0/computes"),
        };
        computes
            .and_then(|c| c.get("c0"))
            .and_then(Value::as_f64)
            .map_or(0, |n| n as u64)
    }
}

pub struct VerifyArgs {
    pub product: Product,
    pub service: String,
    pub env: Option<String>,
    pub from: String,
    pub wait: String,
}

pub async fn verify(cfg: &Config, args: VerifyArgs) -> Result<()> {
    run_verify(cfg, args, POLL_INTERVAL).await
}

async fn run_verify(cfg: &Config, args: VerifyArgs, interval: Duration) -> Result<()> {
    if args.service.trim().is_empty() {
        bail!("--service must not be empty");
    }
    let lookback_ms = util_ext::parse_duration_to_millis(&args.from)?;
    let wait = Duration::from_millis(util_ext::parse_duration_to_millis(&args.wait)?.max(0) as u64);
    let query = build_query(&args.service, args.env.as_deref());

    let started = Instant::now();
    let count = loop {
        let count = probe(cfg, args.product, &query, lookback_ms).await?;
        if count > 0 || started.elapsed() + interval > wait {
            break count;
        }
        tokio::time::sleep(interval).await;
    };

    let result = json!({
        "product": args.product.name(),
        "service": args.service,
        "env": args.env,
        "found": count > 0,
        "count": count,
        "query": query,
        "lookback": args.from,
        "elapsed_seconds": started.elapsed().as_secs(),
        "url": explorer_url(cfg, args.product, &query),
    });
    formatter::output(cfg, &result)?;
    if count == 0 {
        bail!(
            "no {} data found for service {:?} in the last {}",
            args.product.name(),
            args.service,
            args.from
        );
    }
    Ok(())
}

async fn probe(cfg: &Config, product: Product, query: &str, lookback_ms: i64) -> Result<u64> {
    let to_ms = util_ext::now_millis();
    let body = product.aggregate_body(query, to_ms - lookback_ms, to_ms);
    let resp = raw_client::raw_post(cfg, product.aggregate_path(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to query {} data: {e:?}", product.name()))?;
    Ok(product.extract_count(&resp))
}

fn build_query(service: &str, env: Option<&str>) -> String {
    match env {
        Some(env) => format!("service:{service} env:{env}"),
        None => format!("service:{service}"),
    }
}

fn explorer_url(cfg: &Config, product: Product, query: &str) -> String {
    format!(
        "https://{}{}?query={}",
        auth_host_for(&cfg.site),
        product.explorer_path(),
        util_ext::percent_encode(query)
    )
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::test_support::*;

    fn args(product: Product, service: &str, wait: &str) -> VerifyArgs {
        VerifyArgs {
            product,
            service: service.into(),
            env: Some("prod".into()),
            from: "15m".into(),
            wait: wait.into(),
        }
    }

    #[test]
    fn test_build_query_with_and_without_env() {
        assert_eq!(build_query("web", Some("prod")), "service:web env:prod");
        assert_eq!(build_query("web", None), "service:web");
    }

    #[test]
    fn test_extract_count_shapes() {
        let spans = json!({"data": [{"attributes": {"compute": {"c0": 42.0}}}]});
        let logs = json!({"data": {"buckets": [{"computes": {"c0": 7}}]}});
        let empty = json!({"data": {"buckets": []}});
        assert_eq!(Product::Apm.extract_count(&spans), 42);
        assert_eq!(Product::Logs.extract_count(&logs), 7);
        assert_eq!(Product::Rum.extract_count(&empty), 0);
        assert_eq!(Product::Apm.extract_count(&json!({"data": []})), 0);
    }

    #[test]
    fn test_explorer_url_encodes_query() {
        let cfg = test_config("http://unused");
        assert_eq!(
            explorer_url(&cfg, Product::Logs, "service:web env:prod"),
            "https://app.datadoghq.com/logs?query=service%3Aweb%20env%3Aprod"
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_verify_found() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let mock = server
            .mock("POST", "/api/v2/spans/analytics/aggregate")
            .match_body(mockito::Matcher::Regex(
                r#""query":"service:web env:prod""#.into(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": [{"attributes": {"compute": {"c0": 3}}}]}"#)
            .expect(1)
            .create_async()
            .await;

        let result = verify(&cfg, args(Product::Apm, "web", "0s")).await;
        assert!(result.is_ok(), "expected found: {:?}", result.err());
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_verify_not_found_returns_error() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = server
            .mock("POST", "/api/v2/logs/analytics/aggregate")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": {"buckets": []}}"#)
            .create_async()
            .await;

        let err = verify(&cfg, args(Product::Logs, "missing", "0s"))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("no logs data found"), "{err}");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_verify_polls_until_data_arrives() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let empty = server
            .mock("POST", "/api/v2/rum/analytics/aggregate")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": {"buckets": []}}"#)
            .expect(1)
            .create_async()
            .await;
        let found = server
            .mock("POST", "/api/v2/rum/analytics/aggregate")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": {"buckets": [{"computes": {"c0": 5}}]}}"#)
            .expect(1)
            .create_async()
            .await;

        let result = run_verify(
            &cfg,
            args(Product::Rum, "web", "5s"),
            Duration::from_millis(10),
        )
        .await;
        assert!(
            result.is_ok(),
            "expected found on second poll: {:?}",
            result.err()
        );
        empty.assert_async().await;
        found.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_verify_api_error() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = server
            .mock("POST", "/api/v2/spans/analytics/aggregate")
            .with_status(403)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors": ["Forbidden"]}"#)
            .create_async()
            .await;

        let err = verify(&cfg, args(Product::Apm, "web", "0s"))
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("failed to query apm data"),
            "{err}"
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_verify_rejects_empty_service_and_bad_duration() {
        let _lock = lock_env().await;
        let cfg = test_config("http://unused");
        assert!(verify(&cfg, args(Product::Apm, " ", "0s")).await.is_err());
        let mut bad = args(Product::Apm, "web", "0s");
        bad.wait = "soon".into();
        assert!(verify(&cfg, bad).await.is_err());
        cleanup_env();
    }
}
