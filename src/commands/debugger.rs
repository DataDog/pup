use anyhow::Result;
#[cfg(not(target_arch = "wasm32"))]
use datadog_api_client::datadogV2::api_logs::{ListLogsOptionalParams, LogsAPI};
#[cfg(not(target_arch = "wasm32"))]
use datadog_api_client::datadogV2::model::{
    LogsListRequest, LogsListRequestPage, LogsQueryFilter, LogsSort,
};

use crate::raw_client;
use crate::{config::Config, formatter};

async fn post(cfg: &Config, path: &str, body: serde_json::Value) -> Result<serde_json::Value> {
    raw_client::raw_post(cfg, path, body).await
}

async fn post_lenient(
    cfg: &Config,
    path: &str,
    body: serde_json::Value,
) -> Result<serde_json::Value> {
    raw_client::raw_post_lenient(cfg, path, body).await
}

async fn delete(cfg: &Config, path: &str) -> Result<()> {
    raw_client::raw_delete(cfg, path).await
}

async fn get(cfg: &Config, path: &str, query: &[(&str, &str)]) -> Result<serde_json::Value> {
    raw_client::raw_get(cfg, path, query).await
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
    pub capture_expressions: Vec<&'a str>,
    pub rate: u32,
    pub budget: u32,
    pub ttl: Option<&'a str>,
    pub depth: u32,
    pub fields: Option<&'a str>,
}

struct ResolvedProbe<'a> {
    service: &'a str,
    env: &'a str,
    type_name: &'a str,
    method_name: &'a str,
    signature: Option<&'a str>,
    language: &'a str,
    template_str: String,
    segments: serde_json::Value,
    when: Option<serde_json::Value>,
    snapshot: bool,
    capture_expressions: Vec<serde_json::Value>,
    rate: u32,
    budget: u32,
    expires_ms: Option<i64>,
    depth: u32,
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

fn capture_expr_name(dsl: &str) -> String {
    dsl.replace('.', "_")
}

#[cfg(feature = "native")]
async fn parse_capture_expressions(
    cfg: &Config,
    expressions: &[&str],
    depth: u32,
) -> Result<Vec<serde_json::Value>> {
    let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(10));
    let futs: Vec<_> = expressions
        .iter()
        .map(|expr| {
            let expr = expr.to_string();
            let sem = sem.clone();
            async move {
                let _permit = sem.acquire().await;
                let body = serde_json::json!({
                    "data": {
                        "type": "parse-template",
                        "attributes": {
                            "template": format!("{{{expr}}}")
                        }
                    }
                });
                let resp = post_lenient(cfg, "/api/ui/debugger/parse-template", body).await?;
                if let Some(msg) = extract_api_errors(&resp) {
                    anyhow::bail!("invalid capture expression '{expr}': {msg}");
                }
                let seg = &resp["data"]["attributes"]["segments"][0];
                let dsl = seg["dsl"].clone();
                let json_val = seg["json"].clone();
                if dsl.is_null() || json_val.is_null() {
                    anyhow::bail!(
                        "failed to parse capture expression '{expr}': no dsl/json in response"
                    );
                }
                Ok(serde_json::json!({
                    "name": capture_expr_name(&expr),
                    "expr": { "dsl": dsl, "json": json_val },
                    "capture": { "max_reference_depth": depth }
                }))
            }
        })
        .collect();
    futures::future::join_all(futs).await.into_iter().collect()
}

#[cfg(not(feature = "native"))]
async fn parse_capture_expressions(
    cfg: &Config,
    expressions: &[&str],
    depth: u32,
) -> Result<Vec<serde_json::Value>> {
    let mut results = Vec::with_capacity(expressions.len());
    for expr in expressions {
        let body = serde_json::json!({
            "data": {
                "type": "parse-template",
                "attributes": {
                    "template": format!("{{{expr}}}")
                }
            }
        });
        let resp = post_lenient(cfg, "/api/ui/debugger/parse-template", body).await?;
        if let Some(msg) = extract_api_errors(&resp) {
            anyhow::bail!("invalid capture expression '{expr}': {msg}");
        }
        let seg = &resp["data"]["attributes"]["segments"][0];
        let dsl = seg["dsl"].clone();
        let json_val = seg["json"].clone();
        if dsl.is_null() || json_val.is_null() {
            anyhow::bail!("failed to parse capture expression '{expr}': no dsl/json in response");
        }
        results.push(serde_json::json!({
            "name": capture_expr_name(expr),
            "expr": { "dsl": dsl, "json": json_val },
            "capture": { "max_reference_depth": depth }
        }));
    }
    Ok(results)
}

fn build_probe_payload(p: &ResolvedProbe<'_>) -> serde_json::Value {
    let mut probe = serde_json::json!({
        "capture": { "max_reference_depth": p.depth },
        "capture_snapshot": p.snapshot,
        "template": p.template_str,
        "segments": p.segments,
        "sampling": { "snapshots_per_second": p.rate },
        "language": p.language,
        "where": {
            "type_name": p.type_name,
            "method_name": p.method_name,
        },
        "evaluate_at": "EXIT",
        "tags": [],
        "version": 0
    });

    if let Some(sig) = p.signature {
        probe["where"]["signature"] = serde_json::json!(sig);
    }

    if let Some(w) = &p.when {
        probe["when"] = w.clone();
    }

    if !p.capture_expressions.is_empty() {
        probe["capture_expressions"] = serde_json::Value::Array(p.capture_expressions.clone());
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
        capture_expressions,
        rate,
        budget,
        ttl,
        depth,
        fields,
    } = params;
    let expires_ms = if let Some(ttl_str) = ttl {
        Some(crate::util_ext::now_millis() + crate::util_ext::parse_duration_to_millis(ttl_str)?)
    } else {
        None
    };

    let (type_name, method_part) = probe_location
        .rsplit_once(':')
        .ok_or_else(|| anyhow::anyhow!("probe_location must be in format TYPE:METHOD"))?;

    // Split optional signature: "myMethod(int,int)" → method="myMethod", sig="(int,int)"
    let (method_name, signature) = match method_part.find('(') {
        Some(i) => (&method_part[..i], Some(&method_part[i..])),
        None => (method_part, None),
    };

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

    // Parse capture expressions via parse-template (wrap as "{expr}" template)
    let parsed_captures = parse_capture_expressions(cfg, &capture_expressions, depth).await?;

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
        signature,
        language,
        template_str,
        segments,
        when,
        snapshot,
        capture_expressions: parsed_captures,
        rate,
        budget,
        expires_ms,
        depth,
    });

    let resp = post(cfg, PROBES_PATH, payload).await?;

    match fields {
        Some(field_list) => {
            let output = extract_create_fields(&resp, field_list);
            formatter::output(cfg, &output)
        }
        None => formatter::output(cfg, &resp),
    }
}

fn extract_create_fields(resp: &serde_json::Value, field_list: &str) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    for field in field_list.split(',').map(|f| f.trim()) {
        match field {
            "id" => {
                if let Some(v) = resp.pointer("/data/id") {
                    obj.insert("id".to_string(), v.clone());
                }
            }
            "service" => {
                if let Some(v) = resp.pointer("/data/attributes/metadata/service_name") {
                    obj.insert("service".to_string(), v.clone());
                }
            }
            "env" => {
                if let Some(v) = resp
                    .pointer("/data/attributes/metadata/enablement/queries/0/tags/0/values/0/value")
                {
                    obj.insert("env".to_string(), v.clone());
                }
            }
            "location" => {
                let probe = &resp["data"]["attributes"]["probe"]["where"];
                if let (Some(t), Some(m)) =
                    (probe["type_name"].as_str(), probe["method_name"].as_str())
                {
                    obj.insert(
                        "location".to_string(),
                        serde_json::json!(format!("{t}:{m}")),
                    );
                }
            }
            "template" => {
                if let Some(v) = resp.pointer("/data/attributes/probe/template") {
                    obj.insert("template".to_string(), v.clone());
                }
            }
            "expires" => {
                if let Some(v) = resp.pointer("/data/attributes/expires") {
                    obj.insert("expires".to_string(), v.clone());
                }
            }
            other => {
                eprintln!("unknown field: {other}");
            }
        }
    }
    serde_json::Value::Object(obj)
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
    fields: Option<&str>,
) -> Result<()> {
    // Wait for the probe to appear in probe-statuses.  With --wait 0 (the
    // default) we check once and fail immediately if not found.
    let status_path = format!("/api/ui/debugger/probe-statuses?ids={id}");
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(wait);
    let mut found = false;
    loop {
        if let Ok(data) = raw_client::raw_get(cfg, &status_path, &[]).await {
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
        crate::util_ext::parse_time_to_unix_millis(f)?
    } else {
        chrono::Utc::now().timestamp_millis()
    };

    // Set up logs API
    let api = crate::make_api!(LogsAPI, cfg);

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
                match raw_client::raw_get(cfg, &status_path, &[]).await {
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
                                let log_json = serde_json::to_value(&log).unwrap_or_default();
                                let output_value = if let Some(field_list) = fields {
                                    let mut obj = serde_json::Map::new();
                                    for field in field_list.split(',').map(|f| f.trim()) {
                                        match field {
                                            "message" => {
                                                if let Some(v) = log_json.pointer("/attributes/message") {
                                                    obj.insert("message".to_string(), v.clone());
                                                }
                                            }
                                            "captures" => {
                                                if let Some(v) = log_json.pointer("/attributes/attributes/debugger/snapshot/captures") {
                                                    obj.insert("captures".to_string(), v.clone());
                                                }
                                            }
                                            "timestamp" => {
                                                if let Some(v) = log_json.pointer("/attributes/timestamp") {
                                                    obj.insert("timestamp".to_string(), v.clone());
                                                }
                                            }
                                            other => {
                                                eprintln!("unknown field: {other}");
                                            }
                                        }
                                    }
                                    serde_json::Value::Object(obj)
                                } else {
                                    log_json.pointer("/attributes/attributes/debugger")
                                        .cloned()
                                        .unwrap_or(log_json.clone())
                                };
                                formatter::output(cfg, &output_value)?;
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
    _fields: Option<&str>,
) -> Result<()> {
    anyhow::bail!("watch is not supported in wasm builds")
}

// --- Context subcommand ---

pub async fn context(
    cfg: &Config,
    service: &str,
    env: Option<&str>,
    fields: Option<&str>,
) -> Result<()> {
    let path = "/api/unstable/debugger/live-debugger/service-context";
    let data = get(cfg, path, &[("service", service)]).await?;

    let data = match env {
        Some(env_filter) => filter_context_env(&data, env_filter),
        None => data,
    };

    match fields {
        Some(field_list) => formatter::output(cfg, &extract_context_fields(&data, field_list)),
        None => formatter::output(cfg, &data),
    }
}

fn extract_context_fields(data: &serde_json::Value, field_list: &str) -> serde_json::Value {
    let attrs = &data["data"]["attributes"];
    let mut obj = serde_json::Map::new();
    for field in field_list.split(',').map(|f| f.trim()) {
        match field {
            "service" => {
                if let Some(v) = attrs.get("service") {
                    obj.insert("service".into(), v.clone());
                }
            }
            "language" => {
                if let Some(v) = attrs.get("language") {
                    obj.insert("language".into(), v.clone());
                }
            }
            "envs" => {
                let envs: Vec<&str> = attrs["environments"]
                    .as_array()
                    .map(|arr| arr.iter().filter_map(|e| e["env"].as_str()).collect())
                    .unwrap_or_default();
                obj.insert("envs".into(), serde_json::json!(envs));
            }
            "repo" => {
                if let Some(v) = attrs.get("git_repository_url") {
                    obj.insert("repo".into(), v.clone());
                }
            }
            other => {
                eprintln!("unknown field: {other}");
            }
        }
    }
    serde_json::Value::Object(obj)
}

fn filter_context_env(data: &serde_json::Value, env_filter: &str) -> serde_json::Value {
    let mut filtered = data.clone();
    if let Some(envs) = filtered
        .pointer_mut("/data/attributes/environments")
        .and_then(|v| v.as_array_mut())
    {
        envs.retain(|e| e["env"].as_str() == Some(env_filter));
    }
    filtered
}

#[cfg(test)]
mod tests {
    use crate::test_support::*;

    use super::*;
    use crate::config::{Config, OutputFormat};

    fn test_cfg() -> Config {
        Config {
            api_key: Some("test".into()),
            app_key: Some("test".into()),
            access_token: None,
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
            signature: None,
            language: "java",
            template_str,
            segments,
            when: None,
            snapshot: true,
            capture_expressions: vec![],
            rate: 1,
            budget: 500,
            expires_ms: None,
            depth: 3,
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

    #[test]
    fn test_capture_expr_name_simple_ref() {
        assert_eq!(capture_expr_name("userId"), "userId");
    }

    #[test]
    fn test_capture_expr_name_dotted() {
        assert_eq!(
            capture_expr_name("vets.vets[1].firstName"),
            "vets_vets[1]_firstName"
        );
    }

    #[test]
    fn test_capture_expr_name_nested() {
        assert_eq!(
            capture_expr_name("deepObjs[len(garbage)].next.next"),
            "deepObjs[len(garbage)]_next_next"
        );
    }

    #[test]
    fn test_build_probe_payload_capture_expressions() {
        let captures = vec![serde_json::json!({
            "name": "user_name",
            "expr": {
                "dsl": "user.name",
                "json": {
                    "getmember": [{"ref": "user"}, "name"]
                }
            },
            "capture": {
                "max_reference_depth": 3,
            }
        })];
        let payload = test_resolved_probe(|rp| {
            rp.snapshot = false;
            rp.capture_expressions = captures;
        });
        let probe = &payload["data"]["attributes"]["probe"];
        assert_eq!(probe["capture_snapshot"], false);
        assert_eq!(probe["capture_expressions"][0]["name"], "user_name");
        assert_eq!(probe["capture_expressions"][0]["expr"]["dsl"], "user.name");
        assert_eq!(
            probe["capture_expressions"][0]["capture"]["max_reference_depth"],
            3
        );
    }

    #[test]
    fn test_build_probe_payload_no_capture_expressions() {
        let payload = test_resolved_probe(|rp| {
            rp.snapshot = false;
        });
        let probe = &payload["data"]["attributes"]["probe"];
        assert!(probe.get("capture_expressions").is_none());
    }

    #[test]
    fn test_build_probe_payload_custom_depth() {
        let payload = test_resolved_probe(|rp| {
            rp.depth = 5;
        });
        let probe = &payload["data"]["attributes"]["probe"];
        assert_eq!(probe["capture"]["max_reference_depth"], 5);
    }

    fn sample_create_response() -> serde_json::Value {
        serde_json::json!({
            "data": {
                "id": "082b17fd-c75c-4ab0-8426-3456b98d7600",
                "type": "di_log_probe",
                "attributes": {
                    "expires": 1775146304000_i64,
                    "metadata": {
                        "service_name": "debugger-demo-java",
                        "enablement": {
                            "queries": [{
                                "tags": [{
                                    "key": "env",
                                    "values": [{ "value": "prod", "is_excluded": false }]
                                }]
                            }]
                        }
                    },
                    "probe": {
                        "template": "showVetList called with page={page}",
                        "where": {
                            "type_name": "org.springframework.samples.petclinic.vet.VetController",
                            "method_name": "showVetList"
                        }
                    }
                }
            }
        })
    }

    #[test]
    fn test_extract_create_fields_id() {
        let resp = sample_create_response();
        let out = extract_create_fields(&resp, "id");
        assert_eq!(out["id"], "082b17fd-c75c-4ab0-8426-3456b98d7600");
        assert!(out.get("service").is_none());
    }

    #[test]
    fn test_extract_create_fields_all() {
        let resp = sample_create_response();
        let out = extract_create_fields(&resp, "id,service,env,location,template,expires");
        assert_eq!(out["id"], "082b17fd-c75c-4ab0-8426-3456b98d7600");
        assert_eq!(out["service"], "debugger-demo-java");
        assert_eq!(out["env"], "prod");
        assert_eq!(
            out["location"],
            "org.springframework.samples.petclinic.vet.VetController:showVetList"
        );
        assert_eq!(out["template"], "showVetList called with page={page}");
        assert_eq!(out["expires"], 1775146304000_i64);
    }

    #[test]
    fn test_extract_create_fields_ignores_unknown() {
        let resp = sample_create_response();
        let out = extract_create_fields(&resp, "id,bogus");
        assert_eq!(out["id"], "082b17fd-c75c-4ab0-8426-3456b98d7600");
        assert!(out.get("bogus").is_none());
    }

    #[test]
    fn test_probe_location_parsing_plain() {
        let loc = "com.example.MyClass:myMethod";
        let (_, method_part) = loc.rsplit_once(':').unwrap();
        let (method_name, signature) = match method_part.find('(') {
            Some(i) => (&method_part[..i], Some(&method_part[i..])),
            None => (method_part, None),
        };
        assert_eq!(method_name, "myMethod");
        assert_eq!(signature, None);
    }

    #[test]
    fn test_probe_location_parsing_with_signature() {
        let loc = "com.example.MyClass:myMethod(int,java.lang.String)";
        let (type_name, method_part) = loc.rsplit_once(':').unwrap();
        let (method_name, signature) = match method_part.find('(') {
            Some(i) => (&method_part[..i], Some(&method_part[i..])),
            None => (method_part, None),
        };
        assert_eq!(type_name, "com.example.MyClass");
        assert_eq!(method_name, "myMethod");
        assert_eq!(signature, Some("(int,java.lang.String)"));
    }

    #[test]
    fn test_probe_location_parsing_empty_args() {
        let loc = "com.example.MyClass:myMethod()";
        let (_, method_part) = loc.rsplit_once(':').unwrap();
        let (method_name, signature) = match method_part.find('(') {
            Some(i) => (&method_part[..i], Some(&method_part[i..])),
            None => (method_part, None),
        };
        assert_eq!(method_name, "myMethod");
        assert_eq!(signature, Some("()"));
    }

    #[test]
    fn test_build_probe_payload_no_signature() {
        let payload = test_resolved_probe(|_| {});
        let where_block = &payload["data"]["attributes"]["probe"]["where"];
        assert!(where_block.get("signature").is_none());
    }

    #[test]
    fn test_build_probe_payload_with_signature() {
        let payload = test_resolved_probe(|rp| {
            rp.signature = Some("(int,java.lang.String)");
        });
        let where_block = &payload["data"]["attributes"]["probe"]["where"];
        assert_eq!(where_block["type_name"], "com.example.MyClass");
        assert_eq!(where_block["method_name"], "myMethod");
        assert_eq!(where_block["signature"], "(int,java.lang.String)");
    }

    fn sample_context_response() -> serde_json::Value {
        serde_json::json!({
            "data": {
                "attributes": {
                    "service": "my-service",
                    "language": "java",
                    "git_repository_url": "https://github.com/example/repo",
                    "environments": [
                        { "env": "staging", "instance_groups": [] },
                        { "env": "production", "instance_groups": [] }
                    ]
                }
            }
        })
    }

    #[test]
    fn test_extract_context_fields_envs() {
        let data = sample_context_response();
        let out = extract_context_fields(&data, "envs");
        assert_eq!(out["envs"], serde_json::json!(["staging", "production"]));
        assert!(out.get("service").is_none());
    }

    #[test]
    fn test_extract_context_fields_all() {
        let data = sample_context_response();
        let out = extract_context_fields(&data, "service,language,envs,repo");
        assert_eq!(out["service"], "my-service");
        assert_eq!(out["language"], "java");
        assert_eq!(out["envs"], serde_json::json!(["staging", "production"]));
        assert_eq!(out["repo"], "https://github.com/example/repo");
    }

    #[test]
    fn test_extract_context_fields_ignores_unknown() {
        let data = sample_context_response();
        let out = extract_context_fields(&data, "service,bogus");
        assert_eq!(out["service"], "my-service");
        assert!(out.get("bogus").is_none());
    }

    #[tokio::test]
    async fn test_debugger_probes_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"[{"id": "probe-1", "type": "LOG_PROBE"}]"#).await;
        let _ = super::probes_list(&cfg, None).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_debugger_probes_list_with_service() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"[{"id": "probe-1", "type": "LOG_PROBE"}]"#).await;
        let _ = super::probes_list(&cfg, Some("my-service")).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_debugger_probes_get() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"id": "probe-1", "type": "LOG_PROBE"}"#).await;
        let _ = super::probes_get(&cfg, "probe-1").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_debugger_probes_create() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": {"id": "probe-new"}}"#).await;
        let params = super::ProbeCreateParams {
            service: "my-service",
            env: "staging",
            probe_location: "com.example.MyClass:myMethod",
            language: "java",
            template: None,
            condition: None,
            snapshot: true,
            capture_expressions: vec![],
            rate: 1,
            budget: 1000,
            ttl: Some("1h"),
            depth: 3,
            fields: None,
        };
        let _ = super::probes_create(&cfg, params).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_debugger_probes_delete() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#""#).await;
        let _ = super::probes_delete(&cfg, "probe-1").await;
        cleanup_env();
    }
}
