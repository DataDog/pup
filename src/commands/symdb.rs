use std::collections::HashSet;

use anyhow::Result;

use crate::config::Config;
use crate::formatter;
use crate::raw_client;

#[derive(Clone, clap::ValueEnum)]
pub enum SymdbView {
    Full,
    Names,
    #[value(name = "probe-locations")]
    ProbeLocations,
}

impl std::fmt::Display for SymdbView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SymdbView::Full => write!(f, "full"),
            SymdbView::Names => write!(f, "names"),
            SymdbView::ProbeLocations => write!(f, "probe-locations"),
        }
    }
}

const MAX_RETRIES: u32 = 3;
const RETRY_BASE_MS: u64 = 1000;

/// Fetch with retries for transient server errors (502, 503, 504).
async fn fetch(cfg: &Config, path: &str, query: &[(&str, &str)]) -> Result<serde_json::Value> {
    for attempt in 0..=MAX_RETRIES {
        match raw_client::raw_get(cfg, path, query).await {
            Ok(v) => return Ok(v),
            Err(e) => {
                let retryable = e
                    .downcast_ref::<raw_client::HttpError>()
                    .is_some_and(|h| matches!(h.status, 502..=504));
                if !retryable || attempt == MAX_RETRIES {
                    return Err(e);
                }
                let delay = RETRY_BASE_MS * 2u64.pow(attempt);
                eprintln!("retrying in {delay}ms ({e:#})…");
                tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
            }
        }
    }
    unreachable!()
}

const POLL_INTERVAL_MS: u64 = 1000;
const MAX_POLL_SECS: u64 = 60;

pub async fn search(
    cfg: &Config,
    service: &str,
    query: &str,
    version: Option<&str>,
    view: &SymdbView,
    allow_partial: bool,
) -> Result<()> {
    let mut params: Vec<(&str, &str)> = vec![("service", service), ("query", query)];
    if allow_partial {
        params.push(("allow_partial", "true"));
    }
    if let Some(v) = version {
        params.push(("version", v));
    }
    let data = fetch(cfg, "/api/unstable/symdb-api/v2/scopes/search", &params).await?;

    match view {
        SymdbView::Full => formatter::output(cfg, &data),
        SymdbView::Names => {
            let names = collect_names(&data);
            output_lines(cfg, &names)
        }
        SymdbView::ProbeLocations => {
            let mut collector = ProbeCollector::new();
            let mut all = collector.collect(cfg, &data).await?;

            if !cfg.agent_mode {
                print_lines(&all);
            }

            // Re-poll while any service-version is still indexing.
            if !all_indexing_complete(&data) {
                let deadline =
                    std::time::Instant::now() + std::time::Duration::from_secs(MAX_POLL_SECS);
                loop {
                    tokio::time::sleep(std::time::Duration::from_millis(POLL_INTERVAL_MS)).await;
                    let data =
                        fetch(cfg, "/api/unstable/symdb-api/v2/scopes/search", &params).await?;
                    let new = collector.collect(cfg, &data).await?;
                    if !new.is_empty() {
                        if !cfg.agent_mode {
                            print_lines(&new);
                        }
                        all.extend(new);
                    }
                    if all_indexing_complete(&data) || std::time::Instant::now() >= deadline {
                        break;
                    }
                }
            }

            if cfg.agent_mode {
                output_lines(cfg, &all)
            } else {
                Ok(())
            }
        }
    }
}

fn print_lines(lines: &[String]) {
    for line in lines {
        println!("{line}");
    }
}

/// Returns true when every service-version in the response has finished indexing.
fn all_indexing_complete(data: &serde_json::Value) -> bool {
    let Some(items) = data["data"].as_array() else {
        return true;
    };
    items.iter().all(|item| {
        matches!(
            item["attributes"]["indexing_status"].as_str(),
            Some("COMPLETED" | "SOME_FAILED" | "ALL_FAILED" | "NO_ATTACHMENTS") | None
        )
    })
}

/// In agent mode, emit a structured envelope; otherwise print one line per item.
fn output_lines(cfg: &Config, lines: &[String]) -> Result<()> {
    if cfg.agent_mode {
        return formatter::output(cfg, &lines.to_vec());
    }
    print_lines(lines);
    Ok(())
}

fn collect_names(data: &serde_json::Value) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut names = Vec::new();
    if let Some(items) = data["data"].as_array() {
        for item in items {
            if let Some(scopes) = item["attributes"]["scopes"].as_array() {
                for s in scopes {
                    if let Some(name) = s["scope"]["name"].as_str() {
                        if seen.insert(name.to_string()) {
                            names.push(name.to_string());
                        }
                    }
                }
            }
        }
    }
    names
}

#[cfg(feature = "native")]
async fn fetch_children_bulk(cfg: &Config, scope_ids: &[&str]) -> Vec<Result<serde_json::Value>> {
    let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(20));
    let futs: Vec<_> = scope_ids
        .iter()
        .map(|id| {
            let id = id.to_string();
            let sem = sem.clone();
            async move {
                let _permit = sem.acquire().await;
                fetch(
                    cfg,
                    &format!("/api/unstable/symdb-api/v2/scopes/{id}/children"),
                    &[],
                )
                .await
            }
        })
        .collect();
    futures::future::join_all(futs).await
}

#[cfg(not(feature = "native"))]
async fn fetch_children_bulk(cfg: &Config, scope_ids: &[&str]) -> Vec<Result<serde_json::Value>> {
    let mut results = Vec::with_capacity(scope_ids.len());
    for id in scope_ids {
        results.push(
            fetch(
                cfg,
                &format!("/api/unstable/symdb-api/v2/scopes/{id}/children"),
                &[],
            )
            .await,
        );
    }
    results
}

/// Build a probe location string from a scope entry's probe_location and symbols.
/// Extracts ARG-type symbols to produce `type:method(argType1,argType2,...)`.
/// Falls back to `type:method` when no ARG symbols are present.
fn format_probe_location(scope: &serde_json::Value, type_name: &str, method_name: &str) -> String {
    if let Some(symbols) = scope["scope"]["symbols"].as_array() {
        let arg_types: Vec<&str> = symbols
            .iter()
            .filter(|s| s["symbol_type"].as_str() == Some("ARG"))
            .filter_map(|s| s["type"].as_str())
            .collect();
        if !arg_types.is_empty() {
            return format!("{type_name}:{method_name}({})", arg_types.join(", "));
        }
    }
    format!("{type_name}:{method_name}")
}

/// Tracks state across polls so we don't re-fetch children for classes we've already expanded.
struct ProbeCollector {
    seen_locations: HashSet<String>,
    seen_class_ids: HashSet<String>,
}

impl ProbeCollector {
    fn new() -> Self {
        Self {
            seen_locations: HashSet::new(),
            seen_class_ids: HashSet::new(),
        }
    }

    /// Extract probe locations from a search response. Returns only new locations.
    async fn collect(&mut self, cfg: &Config, data: &serde_json::Value) -> Result<Vec<String>> {
        let Some(items) = data["data"].as_array() else {
            return Ok(Vec::new());
        };

        let mut lines: Vec<String> = Vec::new();
        let mut new_class_ids: Vec<&str> = Vec::new();

        for item in items {
            let Some(scopes) = item["attributes"]["scopes"].as_array() else {
                continue;
            };
            for s in scopes {
                let scope_type = s["scope"]["scope_type"].as_str().unwrap_or("");
                let pl = &s["probe_location"];

                if !pl.is_null() {
                    if let (Some(t), Some(m)) =
                        (pl["type_name"].as_str(), pl["method_name"].as_str())
                    {
                        let loc = format_probe_location(s, t, m);
                        if self.seen_locations.insert(loc.clone()) {
                            lines.push(loc);
                        }
                    }
                } else if scope_type == "CLASS" {
                    if let Some(id) = s["scope"]["id"].as_str() {
                        if self.seen_class_ids.insert(id.to_string()) {
                            new_class_ids.push(id);
                        }
                    }
                }
            }
        }

        // Fetch children only for newly discovered classes.
        if !new_class_ids.is_empty() {
            let results = fetch_children_bulk(cfg, &new_class_ids).await;

            for result in results {
                let Ok(children) = result else { continue };
                let Some(child_items) = children["data"].as_array() else {
                    continue;
                };
                for child_item in child_items {
                    let Some(child_scopes) = child_item["attributes"]["scopes"].as_array() else {
                        continue;
                    };
                    for cs in child_scopes {
                        let cpl = &cs["probe_location"];
                        if cpl.is_null() {
                            continue;
                        }
                        if let (Some(t), Some(m)) =
                            (cpl["type_name"].as_str(), cpl["method_name"].as_str())
                        {
                            let loc = format_probe_location(cs, t, m);
                            if self.seen_locations.insert(loc.clone()) {
                                lines.push(loc);
                            }
                        }
                    }
                }
            }
        }

        Ok(lines)
    }
}

/// Fetch the language for a service from symdb metadata.
/// Returns the language string (e.g. "java", "python") or an error if not found.
pub async fn service_language(cfg: &Config, service: &str) -> Result<String> {
    let params = [("service", service)];
    let data = fetch(cfg, "/api/unstable/symdb-api/v2/services/metadata", &params).await?;
    data["data"]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|entry| entry["attributes"]["language"].as_str())
        .map(String::from)
        .ok_or_else(|| anyhow::anyhow!("could not detect language for service {service}"))
}

#[cfg(test)]
mod tests {
    use crate::test_support::*;

    use super::*;

    #[test]
    fn test_collect_names_empty() {
        let data = serde_json::json!({"data": []});
        assert!(collect_names(&data).is_empty());
    }

    #[test]
    fn test_collect_names_extracts_scope_names() {
        let data = serde_json::json!({
            "data": [{
                "attributes": {
                    "scopes": [
                        {"scope": {"name": "com.example.Foo"}},
                        {"scope": {"name": "com.example.Bar"}}
                    ]
                }
            }]
        });
        let names = collect_names(&data);
        assert_eq!(names, vec!["com.example.Foo", "com.example.Bar"]);
    }

    #[test]
    fn test_collect_names_deduplicates_across_versions() {
        let data = serde_json::json!({
            "data": [
                {
                    "attributes": {
                        "scopes": [
                            {"scope": {"name": "com.example.Foo"}},
                            {"scope": {"name": "com.example.Bar"}}
                        ]
                    }
                },
                {
                    "attributes": {
                        "scopes": [
                            {"scope": {"name": "com.example.Bar"}},
                            {"scope": {"name": "com.example.Baz"}}
                        ]
                    }
                }
            ]
        });
        let names = collect_names(&data);
        assert_eq!(
            names,
            vec!["com.example.Foo", "com.example.Bar", "com.example.Baz"]
        );
    }

    #[test]
    fn test_collect_names_missing_data_key() {
        let data = serde_json::json!({"other": "stuff"});
        assert!(collect_names(&data).is_empty());
    }

    #[test]
    fn test_format_probe_location_with_args() {
        let scope = serde_json::json!({
            "scope": {
                "symbols": [
                    { "symbol_type": "ARG", "name": "a", "type": "int" },
                    { "symbol_type": "ARG", "name": "b", "type": "java.lang.String" },
                    { "symbol_type": "LOCAL", "name": "tmp", "type": "int" }
                ]
            }
        });
        assert_eq!(
            format_probe_location(&scope, "com.example.Foo", "bar"),
            "com.example.Foo:bar(int, java.lang.String)"
        );
    }

    #[test]
    fn test_format_probe_location_no_args() {
        let scope = serde_json::json!({
            "scope": {
                "symbols": [
                    { "symbol_type": "LOCAL", "name": "tmp", "type": "int" }
                ]
            }
        });
        assert_eq!(
            format_probe_location(&scope, "com.example.Foo", "bar"),
            "com.example.Foo:bar"
        );
    }

    #[test]
    fn test_format_probe_location_no_symbols() {
        let scope = serde_json::json!({});
        assert_eq!(
            format_probe_location(&scope, "com.example.Foo", "bar"),
            "com.example.Foo:bar"
        );
    }

    #[test]
    fn test_format_probe_location_empty_symbols() {
        let scope = serde_json::json!({ "scope": { "symbols": [] } });
        assert_eq!(
            format_probe_location(&scope, "com.example.Foo", "bar"),
            "com.example.Foo:bar"
        );
    }

    #[test]
    fn test_all_indexing_complete() {
        let completed =
            serde_json::json!({"data": [{"attributes": {"indexing_status": "COMPLETED"}}]});
        assert!(all_indexing_complete(&completed));

        let some_failed =
            serde_json::json!({"data": [{"attributes": {"indexing_status": "SOME_FAILED"}}]});
        assert!(all_indexing_complete(&some_failed));

        let all_failed =
            serde_json::json!({"data": [{"attributes": {"indexing_status": "ALL_FAILED"}}]});
        assert!(all_indexing_complete(&all_failed));

        let no_attach =
            serde_json::json!({"data": [{"attributes": {"indexing_status": "NO_ATTACHMENTS"}}]});
        assert!(all_indexing_complete(&no_attach));

        let processing =
            serde_json::json!({"data": [{"attributes": {"indexing_status": "PROCESSING"}}]});
        assert!(!all_indexing_complete(&processing));

        let requested =
            serde_json::json!({"data": [{"attributes": {"indexing_status": "REQUESTED"}}]});
        assert!(!all_indexing_complete(&requested));

        let pending = serde_json::json!({"data": [{"attributes": {"indexing_status": "PENDING"}}]});
        assert!(!all_indexing_complete(&pending));

        // Mixed: one completed + one processing → not complete.
        let mixed = serde_json::json!({"data": [
            {"attributes": {"indexing_status": "COMPLETED"}},
            {"attributes": {"indexing_status": "PROCESSING"}}
        ]});
        assert!(!all_indexing_complete(&mixed));

        // Empty data array → complete (nothing to wait for).
        assert!(all_indexing_complete(&serde_json::json!({"data": []})));

        // Missing data key → complete.
        assert!(all_indexing_complete(&serde_json::json!({})));

        // Missing indexing_status field → complete (backward compat).
        let no_field = serde_json::json!({"data": [{"attributes": {}}]});
        assert!(all_indexing_complete(&no_field));
    }

    #[test]
    fn test_symdb_view_display() {
        assert_eq!(SymdbView::Full.to_string(), "full");
        assert_eq!(SymdbView::Names.to_string(), "names");
        assert_eq!(SymdbView::ProbeLocations.to_string(), "probe-locations");
    }

    #[tokio::test]
    async fn test_symdb_search() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(
            &mut s,
            r#"{"data": [{"attributes": {"scopes": [{"scope": {"name": "MyClass"}}]}}]}"#,
        )
        .await;
        let _ = super::search(
            &cfg,
            "my-service",
            "MyClass",
            None,
            &super::SymdbView::Full,
            true,
        )
        .await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_symdb_search_names_view() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(
            &mut s,
            r#"{"data": [{"attributes": {"scopes": [{"scope": {"name": "MyClass"}}]}}]}"#,
        )
        .await;
        let _ = super::search(
            &cfg,
            "my-service",
            "MyClass",
            None,
            &super::SymdbView::Names,
            true,
        )
        .await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_symdb_search_probe_locations_view() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": [{"attributes": {"scopes": [{"scope": {"name": "MyClass", "scope_type": "METHOD"}, "probe_location": {"type_name": "MyClass", "method_name": "doStuff"}}]}}]}"#).await;
        let _ = super::search(
            &cfg,
            "my-service",
            "MyClass",
            None,
            &super::SymdbView::ProbeLocations,
            true,
        )
        .await;
        cleanup_env();
    }
}
