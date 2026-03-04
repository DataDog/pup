use anyhow::Result;
#[cfg(not(target_arch = "wasm32"))]
use datadog_api_client::datadogV2::api_logs::{ListLogsOptionalParams, LogsAPI};
#[cfg(not(target_arch = "wasm32"))]
use datadog_api_client::datadogV2::model::{
    LogsListRequest, LogsListRequestPage, LogsQueryFilter, LogsSort,
};

#[cfg(not(target_arch = "wasm32"))]
use crate::client;
use crate::{config::Config, formatter};

#[cfg(not(target_arch = "wasm32"))]
async fn post(cfg: &Config, path: &str, body: serde_json::Value) -> Result<serde_json::Value> {
    client::raw_post(cfg, path, body).await
}

#[cfg(not(target_arch = "wasm32"))]
async fn post_lenient(
    cfg: &Config,
    path: &str,
    body: serde_json::Value,
) -> Result<serde_json::Value> {
    client::raw_post_lenient(cfg, path, body).await
}

#[cfg(target_arch = "wasm32")]
async fn post(cfg: &Config, path: &str, body: serde_json::Value) -> Result<serde_json::Value> {
    crate::api::post(cfg, path, &body).await
}

#[cfg(target_arch = "wasm32")]
async fn post_lenient(
    cfg: &Config,
    path: &str,
    body: serde_json::Value,
) -> Result<serde_json::Value> {
    crate::api::post_lenient(cfg, path, &body).await
}

#[cfg(not(target_arch = "wasm32"))]
async fn delete(cfg: &Config, path: &str) -> Result<()> {
    client::raw_delete(cfg, path).await
}

#[cfg(target_arch = "wasm32")]
async fn delete(cfg: &Config, path: &str) -> Result<()> {
    crate::api::delete(cfg, path).await
}

#[cfg(not(target_arch = "wasm32"))]
async fn get(cfg: &Config, path: &str, query: &[(&str, &str)]) -> Result<serde_json::Value> {
    client::raw_get(cfg, path, query).await
}

#[cfg(target_arch = "wasm32")]
async fn get(cfg: &Config, path: &str, query: &[(&str, &str)]) -> Result<serde_json::Value> {
    crate::api::get(cfg, path, query).await
}

fn extract_api_errors(resp: &serde_json::Value) -> Option<String> {
    let errors = resp.get("errors")?.as_array()?;
    if errors.is_empty() {
        return None;
    }
    let messages: Vec<String> = errors
        .iter()
        .map(|err| {
            // Try to parse `detail` as nested JSON and grab `message`.
            if let Some(detail) = err.get("detail").and_then(|d| d.as_str()) {
                if let Ok(inner) = serde_json::from_str::<serde_json::Value>(detail) {
                    if let Some(msg) = inner.get("message").and_then(|m| m.as_str()) {
                        return msg.to_string();
                    }
                }
                // detail is a plain string, not nested JSON
                return detail.to_string();
            }
            // Fall back to title
            err.get("title")
                .and_then(|t| t.as_str())
                .unwrap_or("unknown error")
                .to_string()
        })
        .collect();
    Some(messages.join("; "))
}

const PROBES_PATH: &str = "/api/ui/remote_config/products/live_debugging/probes/log";

pub async fn probes_list(cfg: &Config, service: Option<&str>) -> Result<()> {
    let query: Vec<(&str, &str)> = service.iter().map(|s| ("service", *s)).collect();
    let data = get(cfg, PROBES_PATH, &query).await?;
    formatter::output(cfg, &data)
}

pub async fn probes_get(cfg: &Config, id: &str) -> Result<()> {
    let data = get(cfg, &format!("{PROBES_PATH}/{id}"), &[]).await?;
    formatter::output(cfg, &data)
}

pub struct ProbeCreateParams<'a> {
    pub service: &'a str,
    pub env: &'a str,
    pub probe_location: &'a str,
    pub language: &'a str,
    pub template: Option<&'a str>,
    pub condition: Option<&'a str>,
    pub snapshot: bool,
    pub rate: u32,
    pub budget: u32,
    pub ttl: Option<&'a str>,
}

struct ResolvedProbe<'a> {
    service: &'a str,
    env: &'a str,
    type_name: &'a str,
    method_name: &'a str,
    language: &'a str,
    template_str: String,
    segments: serde_json::Value,
    when: Option<serde_json::Value>,
    snapshot: bool,
    rate: u32,
    budget: u32,
    expires_ms: Option<i64>,
}

fn default_template(type_name: &str, method_name: &str) -> (String, serde_json::Value) {
    let tmpl = format!("Executed {type_name}.{method_name}, it took {{@duration}}ms");
    let segs = serde_json::json!([
        { "str": format!("Executed {type_name}.{method_name}, it took ") },
        { "dsl": "@duration", "json": { "ref": "@duration" } },
        { "str": "ms" }
    ]);
    (tmpl, segs)
}

fn build_probe_payload(p: &ResolvedProbe<'_>) -> serde_json::Value {
    let mut probe = serde_json::json!({
        "capture": { "max_reference_depth": 3 },
        "capture_snapshot": p.snapshot,
        "template": p.template_str,
        "segments": p.segments,
        "sampling": { "snapshots_per_second": p.rate },
        "language": p.language,
        "where": {
            "type_name": p.type_name,
            "method_name": p.method_name
        },
        "evaluate_at": "EXIT",
        "tags": [],
        "version": 0
    });

    if let Some(w) = &p.when {
        probe["when"] = w.clone();
    }

    let mut payload = serde_json::json!({
        "data": {
            "id": "",
            "type": "di_log_probe",
            "attributes": {
                "id": "",
                "disabled": false,
                "version": 0,
                "metadata": {
                    "service_name": p.service,
                    "type": "LOG_PROBE",
                    "enablement": {
                        "queries": [{
                            "text": format!("env:{}", p.env),
                            "limit": 0,
                            "tags": [{
                                "key": "env",
                                "values": [{
                                    "value": p.env,
                                    "is_excluded": false
                                }]
                            }]
                        }]
                    },
                    "budget": {
                        "limit": p.budget,
                        "window": "total",
                        "exceeded": false
                    }
                },
                "probe": probe
            }
        }
    });

    if let Some(exp) = p.expires_ms {
        payload["data"]["attributes"]["expires"] = serde_json::json!(exp);
    }

    payload
}

pub async fn probes_create(cfg: &Config, params: ProbeCreateParams<'_>) -> Result<()> {
    let ProbeCreateParams {
        service,
        env,
        probe_location,
        language,
        template,
        condition,
        snapshot,
        rate,
        budget,
        ttl,
    } = params;
    let expires_ms = if let Some(ttl_str) = ttl {
        Some(crate::util::now_millis() + crate::util::parse_duration_to_millis(ttl_str)?)
    } else {
        None
    };

    let (type_name, method_name) = probe_location
        .rsplit_once(':')
        .ok_or_else(|| anyhow::anyhow!("probe_location must be in format TYPE:METHOD"))?;

    // Parse condition if provided
    let when = if let Some(cond) = condition {
        let body = serde_json::json!({
            "data": {
                "type": "parse-expression",
                "attributes": {
                    "expr": cond,
                    "type": "condition"
                }
            }
        });
        let resp = post_lenient(cfg, "/api/ui/debugger/parse-expression", body).await?;
        if let Some(msg) = extract_api_errors(&resp) {
            anyhow::bail!("invalid condition: {msg}");
        }
        let dsl = resp["data"]["attributes"]["dsl"].clone();
        let json_val = resp["data"]["attributes"]["json"].clone();
        Some(serde_json::json!({ "dsl": dsl, "json": json_val }))
    } else {
        None
    };

    // Build segments and template string
    let (template_str, segments) = if let Some(tmpl) = template {
        let body = serde_json::json!({
            "data": {
                "type": "parse-template",
                "attributes": {
                    "template": tmpl
                }
            }
        });
        let resp = post_lenient(cfg, "/api/ui/debugger/parse-template", body).await?;
        if let Some(msg) = extract_api_errors(&resp) {
            anyhow::bail!("invalid template: {msg}");
        }
        let segs = resp["data"]["attributes"]["segments"].clone();
        (tmpl.to_string(), segs)
    } else {
        default_template(type_name, method_name)
    };

    let payload = build_probe_payload(&ResolvedProbe {
        service,
        env,
        type_name,
        method_name,
        language,
        template_str,
        segments,
        when,
        snapshot,
        rate,
        budget,
        expires_ms,
    });

    let resp = post(cfg, PROBES_PATH, payload).await?;
    formatter::output(cfg, &resp)
}

pub async fn probes_delete(cfg: &Config, id: &str) -> Result<()> {
    delete(cfg, &format!("{PROBES_PATH}/{id}")).await?;
    delete_output(cfg, id)
}

fn delete_output(cfg: &Config, id: &str) -> Result<()> {
    if cfg.agent_mode {
        formatter::output(cfg, &serde_json::json!({"id": id, "deleted": true}))
    } else {
        println!("Probe {id} deleted.");
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn probes_watch(
    cfg: &Config,
    id: &str,
    limit: Option<u32>,
    timeout: u64,
    from: Option<&str>,
    wait: u64,
) -> Result<()> {
    // Wait for the probe to appear in probe-statuses.  With --wait 0 (the
    // default) we check once and fail immediately if not found.
    let status_path = format!("/api/ui/debugger/probe-statuses?ids={id}");
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(wait);
    let mut found = false;
    loop {
        if let Ok(data) = client::raw_get(cfg, &status_path, &[]).await {
            if data["data"].as_array().and_then(|a| a.first()).is_some() {
                found = true;
                break;
            }
        }
        if tokio::time::Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
    if !found {
        if wait > 0 {
            anyhow::bail!("probe {id} not found after {wait}s (--wait {wait})");
        } else {
            anyhow::bail!(
                "probe {id} not found; if it was just created, retry with --wait <seconds>"
            );
        }
    }

    let from_ms_init = if let Some(f) = from {
        crate::util::parse_time_to_unix_millis(f)?
    } else {
        chrono::Utc::now().timestamp_millis()
    };

    // Set up logs API
    let dd_cfg = client::make_dd_config(cfg);
    let api = match client::make_bearer_client(cfg) {
        Some(c) => LogsAPI::with_client_and_config(dd_cfg, c),
        None => LogsAPI::with_config(dd_cfg),
    };

    let query = format!("@debugger.snapshot.probe.id:{id}");
    let mut from_ms = from_ms_init;
    let mut event_count: u32 = 0;
    let mut consecutive_errors: u32 = 0;

    let mut status_interval = tokio::time::interval(std::time::Duration::from_secs(1));
    let mut logs_interval = tokio::time::interval(std::time::Duration::from_secs(1));
    let timeout_sleep = tokio::time::sleep(std::time::Duration::from_secs(timeout));
    tokio::pin!(timeout_sleep);

    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);

    loop {
        tokio::select! {
            _ = &mut timeout_sleep => {
                if event_count == 0 {
                    anyhow::bail!("timed out after {timeout}s with no events");
                }
                return Ok(());
            }
            _ = &mut ctrl_c => {
                return Ok(());
            }
            _ = status_interval.tick() => {
                match client::raw_get(cfg, &status_path, &[]).await {
                    Ok(data) => {
                        consecutive_errors = 0;
                        if let Some(entry) = data["data"].as_array().and_then(|a| a.first()) {
                            if let Some(diagnostics) = entry["attributes"]["diagnostics"].as_array() {
                                for diag in diagnostics {
                                    if diag["status"].as_str() == Some("ERROR") {
                                        eprintln!("probe error: {}", diag);
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        consecutive_errors += 1;
                        if consecutive_errors > 10 {
                            anyhow::bail!("too many consecutive errors, last: {e:?}");
                        }
                        eprintln!("failed to fetch probe status: {e:?}");
                    }
                }
            }
            _ = logs_interval.tick() => {
                let now_ms = chrono::Utc::now().timestamp_millis();
                let filter = LogsQueryFilter::new()
                    .query(query.clone())
                    .from(from_ms.to_string())
                    .to(now_ms.to_string());

                let body = LogsListRequest::new()
                    .filter(filter)
                    .page(LogsListRequestPage::new().limit(100))
                    .sort(LogsSort::TIMESTAMP_ASCENDING);

                let params = ListLogsOptionalParams::default().body(body);

                match api.list_logs(params).await {
                    Ok(resp) => {
                        consecutive_errors = 0;
                        if let Some(logs) = resp.data {
                            for log in logs {
                                formatter::output(cfg, &log)?;
                                event_count += 1;

                                // Advance from past this event's timestamp to
                                // avoid re-fetching it on the next poll.
                                if let Some(ts) = log
                                    .attributes
                                    .as_ref()
                                    .and_then(|a| a.timestamp.as_ref())
                                    .map(|t| t.timestamp_millis())
                                {
                                    if ts >= from_ms {
                                        from_ms = ts + 1;
                                    }
                                }

                                if limit.is_some_and(|l| event_count >= l) {
                                    return Ok(());
                                }
                            }
                        }
                    }
                    Err(e) => {
                        consecutive_errors += 1;
                        if consecutive_errors > 10 {
                            anyhow::bail!("too many consecutive errors, last: {e:?}");
                        }
                        eprintln!("failed to search logs: {e:?}");
                    }
                }
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub async fn probes_watch(
    _cfg: &Config,
    _id: &str,
    _limit: Option<u32>,
    _timeout: u64,
    _from: Option<&str>,
    _wait: u64,
) -> Result<()> {
    anyhow::bail!("watch is not supported in wasm builds")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, OutputFormat};

    fn test_cfg() -> Config {
        Config {
            api_key: Some("test".into()),
            app_key: Some("test".into()),
            access_token: None,
            site: "datadoghq.com".into(),
            org: None,
            output_format: OutputFormat::Json,
            auto_approve: false,
            agent_mode: false,
            read_only: false,
        }
    }

    #[test]
    fn test_delete_output_human() {
        let cfg = test_cfg();
        let result = delete_output(&cfg, "probe-123");
        assert!(result.is_ok());
    }

    #[test]
    fn test_delete_output_agent_mode() {
        let mut cfg = test_cfg();
        cfg.agent_mode = true;
        let result = delete_output(&cfg, "probe-123");
        assert!(result.is_ok());
    }

    fn test_resolved_probe(overrides: impl FnOnce(&mut ResolvedProbe<'_>)) -> serde_json::Value {
        let (template_str, segments) = default_template("com.example.MyClass", "myMethod");
        let mut rp = ResolvedProbe {
            service: "my-service",
            env: "staging",
            type_name: "com.example.MyClass",
            method_name: "myMethod",
            language: "java",
            template_str,
            segments,
            when: None,
            snapshot: true,
            rate: 1,
            budget: 500,
            expires_ms: None,
        };
        overrides(&mut rp);
        build_probe_payload(&rp)
    }

    #[test]
    fn test_default_template() {
        let (tmpl, segs) = default_template("com.example.MyClass", "doStuff");
        assert_eq!(
            tmpl,
            "Executed com.example.MyClass.doStuff, it took {@duration}ms"
        );
        assert_eq!(
            segs[0]["str"],
            "Executed com.example.MyClass.doStuff, it took "
        );
        assert_eq!(segs[1]["dsl"], "@duration");
        assert_eq!(segs[2]["str"], "ms");
    }

    #[test]
    fn test_build_probe_payload_structure() {
        let payload = test_resolved_probe(|_| {});

        assert_eq!(payload["data"]["type"], "di_log_probe");
        assert_eq!(payload["data"]["attributes"]["disabled"], false);

        let meta = &payload["data"]["attributes"]["metadata"];
        assert_eq!(meta["service_name"], "my-service");
        assert_eq!(meta["type"], "LOG_PROBE");
        assert_eq!(meta["budget"]["limit"], 500);
        assert_eq!(meta["budget"]["window"], "total");

        let query = &meta["enablement"]["queries"][0];
        assert_eq!(query["text"], "env:staging");
        assert_eq!(query["tags"][0]["key"], "env");
        assert_eq!(query["tags"][0]["values"][0]["value"], "staging");

        let p = &payload["data"]["attributes"]["probe"];
        assert_eq!(p["language"], "java");
        assert_eq!(p["where"]["type_name"], "com.example.MyClass");
        assert_eq!(p["where"]["method_name"], "myMethod");
        assert_eq!(p["capture_snapshot"], true);
        assert_eq!(p["sampling"]["snapshots_per_second"], 1);
        assert_eq!(p["evaluate_at"], "EXIT");
    }

    #[test]
    fn test_build_probe_payload_expires() {
        let payload = test_resolved_probe(|rp| {
            rp.expires_ms = Some(1_700_000_000_000);
        });
        assert_eq!(
            payload["data"]["attributes"]["expires"],
            1_700_000_000_000_i64
        );
    }

    #[test]
    fn test_build_probe_payload_no_expires() {
        let payload = test_resolved_probe(|_| {});
        assert!(payload["data"]["attributes"].get("expires").is_none());
    }

    #[test]
    fn test_build_probe_payload_when() {
        let when = serde_json::json!({ "dsl": "x > 1", "json": { "gt": [{"ref": "x"}, 1] } });
        let payload = test_resolved_probe(|rp| {
            rp.when = Some(when.clone());
        });
        assert_eq!(
            payload["data"]["attributes"]["probe"]["when"]["dsl"],
            "x > 1"
        );
    }

    #[test]
    fn test_build_probe_payload_no_when() {
        let payload = test_resolved_probe(|_| {});
        assert!(payload["data"]["attributes"]["probe"].get("when").is_none());
    }

    #[test]
    fn test_extract_api_errors_nested_json_detail() {
        // Real API response shape for an invalid template expression.
        let resp = serde_json::json!({
            "errors": [{
                "title": "Generic Error",
                "detail": "{\"message\":\"Valid contextual reference values are: @duration, @exception, @it, and @return.\",\"invalidExpressionPart\":\"@iafsfasf\",\"found\":\"@\",\"position\":0}"
            }]
        });
        let msg = extract_api_errors(&resp).unwrap();
        assert_eq!(
            msg,
            "Valid contextual reference values are: @duration, @exception, @it, and @return."
        );
    }

    #[test]
    fn test_extract_api_errors_plain_detail() {
        let resp = serde_json::json!({
            "errors": [{ "title": "Bad Request", "detail": "something went wrong" }]
        });
        let msg = extract_api_errors(&resp).unwrap();
        assert_eq!(msg, "something went wrong");
    }

    #[test]
    fn test_extract_api_errors_title_fallback() {
        let resp = serde_json::json!({
            "errors": [{ "title": "Not Found" }]
        });
        let msg = extract_api_errors(&resp).unwrap();
        assert_eq!(msg, "Not Found");
    }

    #[test]
    fn test_extract_api_errors_empty_array() {
        let resp = serde_json::json!({ "errors": [] });
        assert!(extract_api_errors(&resp).is_none());
    }

    #[test]
    fn test_extract_api_errors_no_errors_field() {
        let resp = serde_json::json!({ "data": {} });
        assert!(extract_api_errors(&resp).is_none());
    }
}
