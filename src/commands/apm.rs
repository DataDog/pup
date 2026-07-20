use anyhow::Result;

use crate::config::Config;
use crate::formatter;
use crate::raw_client;
use crate::util_ext;

pub async fn services_list(cfg: &Config, env: String, from: String, to: String) -> Result<()> {
    let from_ts = util_ext::parse_time_to_unix(&from)?;
    let to_ts = util_ext::parse_time_to_unix(&to)?;
    let path = format!("/api/v2/apm/services?start={from_ts}&end={to_ts}&filter[env]={env}");
    let data = raw_client::raw_get(cfg, &path, &[]).await?;
    formatter::output(cfg, &data)
}

pub async fn services_stats(cfg: &Config, env: String, from: String, to: String) -> Result<()> {
    let from_ts = util_ext::parse_time_to_unix(&from)?;
    let to_ts = util_ext::parse_time_to_unix(&to)?;
    let path = format!("/api/v2/apm/services/stats?start={from_ts}&end={to_ts}&filter[env]={env}");
    let data = raw_client::raw_get(cfg, &path, &[]).await?;
    formatter::output(cfg, &data)
}

pub async fn entities_list(cfg: &Config, from: String, to: String) -> Result<()> {
    let from_ts = util_ext::parse_time_to_unix(&from)?;
    let to_ts = util_ext::parse_time_to_unix(&to)?;
    let path = format!("/api/unstable/apm/entities?start={from_ts}&end={to_ts}");
    let data = raw_client::raw_get(cfg, &path, &[]).await?;
    formatter::output(cfg, &data)
}

pub async fn dependencies_list(cfg: &Config, env: String, from: String, to: String) -> Result<()> {
    let from_ts = util_ext::parse_time_to_unix(&from)?;
    let to_ts = util_ext::parse_time_to_unix(&to)?;
    let path = format!("/api/v1/service_dependencies?start={from_ts}&end={to_ts}&env={env}");
    let data = raw_client::raw_get(cfg, &path, &[]).await?;
    formatter::output(cfg, &data)
}

pub async fn services_operations(
    cfg: &Config,
    service: String,
    env: String,
    from: String,
    to: String,
) -> Result<()> {
    let from_ts = util_ext::parse_time_to_unix(&from)?;
    let to_ts = util_ext::parse_time_to_unix(&to)?;
    let path =
        format!("/api/v1/trace/operation_names/{service}?env={env}&start={from_ts}&end={to_ts}");
    let data = raw_client::raw_get(cfg, &path, &[]).await?;
    formatter::output(cfg, &data)
}

pub async fn services_resources(
    cfg: &Config,
    service: String,
    name: String,
    env: String,
    from: String,
    to: String,
) -> Result<()> {
    let from_ts = util_ext::parse_time_to_unix(&from)?;
    let to_ts = util_ext::parse_time_to_unix(&to)?;
    let path = format!(
        "/api/ui/apm/resources?service={service}&name={name}&env={env}&from={from_ts}&to={to_ts}"
    );
    let data = raw_client::raw_get(cfg, &path, &[]).await?;
    formatter::output(cfg, &data)
}

pub async fn flow_map(
    cfg: &Config,
    query: String,
    limit: i64,
    from: String,
    to: String,
) -> Result<()> {
    let from_ts = util_ext::parse_time_to_unix(&from)?;
    let to_ts = util_ext::parse_time_to_unix(&to)?;
    let path =
        format!("/api/ui/apm/flow-map?query={query}&limit={limit}&start={from_ts}&end={to_ts}");
    let data = raw_client::raw_get(cfg, &path, &[]).await?;
    formatter::output(cfg, &data)
}

pub async fn troubleshooting_list(
    cfg: &Config,
    hostname: Option<String>,
    timeframe: Option<String>,
    result: Option<String>,
) -> Result<()> {
    let path = "/api/unstable/apm/instrumentation-errors";
    let mut pairs: Vec<(String, String)> = Vec::new();
    if let Some(h) = hostname {
        pairs.push(("hostname".to_string(), h));
    }
    if let Some(tf) = timeframe {
        pairs.push(("timeframe".to_string(), tf));
    }
    if let Some(r) = result {
        pairs.push(("result".to_string(), r));
    }
    let query: Vec<(&str, &str)> = pairs
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let data = raw_client::raw_get(cfg, path, &query).await?;
    formatter::output(cfg, &data)
}

pub async fn service_remapping_list(cfg: &Config) -> Result<()> {
    let data = raw_client::raw_get(cfg, "/api/v2/service-naming-rules", &[]).await?;
    formatter::output(cfg, &data)
}

pub async fn service_remapping_create(
    cfg: &Config,
    name: String,
    filter: String,
    rule_type: i64,
    value: String,
) -> Result<()> {
    let body = serde_json::json!({
        "data": {
            "type": "rule",
            "attributes": {
                "name": name,
                "filter": filter,
                "rule_type": rule_type,
                "rewrite_tag_rules": [{"destination_tag_name": "service", "value": value}]
            }
        }
    });
    let data = raw_client::raw_post(cfg, "/api/v2/service-naming-rules", body).await?;
    formatter::output(cfg, &data)
}

pub async fn service_remapping_get(cfg: &Config, id: String) -> Result<()> {
    let data = raw_client::raw_get(cfg, &format!("/api/v2/service-naming-rules/{id}"), &[]).await?;
    formatter::output(cfg, &data)
}

pub async fn service_remapping_update(
    cfg: &Config,
    id: String,
    name: String,
    filter: String,
    rule_type: i64,
    value: String,
    version: i64,
) -> Result<()> {
    let body = serde_json::json!({
        "data": {
            "type": "rule",
            "attributes": {
                "name": name,
                "filter": filter,
                "rule_type": rule_type,
                "rewrite_tag_rules": [{"destination_tag_name": "service", "value": value}],
                "version": version
            }
        }
    });
    let data =
        raw_client::raw_put(cfg, &format!("/api/v2/service-naming-rules/{id}"), body).await?;
    formatter::output(cfg, &data)
}

pub async fn service_remapping_delete(cfg: &Config, id: String, version: i64) -> Result<()> {
    raw_client::raw_delete(cfg, &format!("/api/v2/service-naming-rules/{id}/{version}")).await
}

// =============================================================================
// APM sampling rules — customer per-(service, env) resource sampling rules.
// Backed by RC product APM_TRACING (provenance:customer). These rules surface
// on traces with `_dd.p.dm:-11` and `ingestion_reason:remote_rule`.
// =============================================================================

const SAMPLING_RULES_BASE: &str = "/api/unstable/remote_config/products/apm_tracing/configs";

pub async fn sampling_rules_list(
    cfg: &Config,
    service: Option<String>,
    env: Option<String>,
) -> Result<()> {
    // If service + env are both given, prefer the narrowed by_target endpoint.
    if let (Some(svc), Some(e)) = (service.as_deref(), env.as_deref()) {
        let path = format!("{SAMPLING_RULES_BASE}/by_target");
        let data = raw_client::raw_get(cfg, &path, &[("service", svc), ("env", e)]).await?;
        return formatter::output(cfg, &data);
    }
    let data = raw_client::raw_get(cfg, SAMPLING_RULES_BASE, &[]).await?;
    formatter::output(cfg, &data)
}

pub async fn sampling_rules_get(cfg: &Config, id: String) -> Result<()> {
    let data = raw_client::raw_get(cfg, &format!("{SAMPLING_RULES_BASE}/{id}"), &[]).await?;
    formatter::output(cfg, &data)
}

pub async fn sampling_rules_create(
    cfg: &Config,
    service: String,
    env: String,
    resource: String,
    sample_rate: f64,
) -> Result<()> {
    let body = serde_json::json!({
        "data": {
            "type": "apm_tracing_config",
            "attributes": {
                "action": "enable",
                "lib_config": {
                    "library_language": "all",
                    "library_version": "latest",
                    "service_name": service,
                    "env": env,
                    "tracing_sampling_rules": [{
                        "service": service,
                        "provenance": "customer",
                        "resource": resource,
                        "sample_rate": sample_rate,
                    }],
                },
                "service_target": {
                    "service": service,
                    "env": env,
                },
            }
        }
    });
    let data = raw_client::raw_post(cfg, SAMPLING_RULES_BASE, body).await?;
    formatter::output(cfg, &data)
}

pub async fn sampling_rules_update(
    cfg: &Config,
    id: String,
    service: String,
    env: String,
    resource: String,
    sample_rate: f64,
) -> Result<()> {
    let body = serde_json::json!({
        "data": {
            "id": id,
            "type": "apm_tracing_config",
            "attributes": {
                "action": "enable",
                "lib_config": {
                    "library_language": "all",
                    "library_version": "latest",
                    "service_name": service,
                    "env": env,
                    "tracing_sampling_rules": [{
                        "service": service,
                        "provenance": "customer",
                        "resource": resource,
                        "sample_rate": sample_rate,
                    }],
                },
                "service_target": {
                    "service": service,
                    "env": env,
                },
            }
        }
    });
    let data = raw_client::raw_put(cfg, &format!("{SAMPLING_RULES_BASE}/{id}"), body).await?;
    formatter::output(cfg, &data)
}

pub async fn sampling_rules_delete(cfg: &Config, id: String) -> Result<()> {
    raw_client::raw_delete(cfg, &format!("{SAMPLING_RULES_BASE}/{id}")).await
}

// =============================================================================
// APM adaptive sampling — let Datadog auto-tune per-resource sampling rates to
// fit a monthly byte/percent allotment. Generated rules surface on traces with
// `_dd.p.dm:-12` and `ingestion_reason:adaptive_rule`.
//
// Strategy values:
//   - "fixed_target"  — set a hard byte target (use with --bytes)
//   - "percent_total" — set a percent of allotment cap (use with --percent)
// =============================================================================

const ADAPTIVE_SAMPLING_BASE: &str = "/api/ui/adaptive_sampling";

fn allotment_attributes(bytes: Option<i64>, percent: Option<f64>) -> serde_json::Value {
    let strategy = if bytes.is_some() {
        "fixed_target"
    } else {
        "percent_total"
    };
    let mut attrs = serde_json::json!({ "strategy": strategy });
    if let Some(b) = bytes {
        attrs["allotment_bytes"] = serde_json::json!(b);
    }
    if let Some(p) = percent {
        attrs["allotment_percent"] = serde_json::json!(p);
    }
    attrs
}

pub async fn adaptive_sampling_onboarding_status(
    cfg: &Config,
    service: Option<String>,
    env: Option<String>,
) -> Result<()> {
    let path = format!("{ADAPTIVE_SAMPLING_BASE}/onboarding_status");
    let mut params: Vec<(&str, &str)> = Vec::new();
    if let Some(s) = service.as_deref() {
        params.push(("service", s));
    }
    if let Some(e) = env.as_deref() {
        params.push(("env", e));
    }
    let data = raw_client::raw_get(cfg, &path, &params).await?;
    formatter::output(cfg, &data)
}

async fn post_onboarding(
    cfg: &Config,
    service: String,
    env: String,
    onboarded: bool,
) -> Result<()> {
    let body = serde_json::json!({
        "data": {
            "id": "1",
            "type": "apm_adaptive_sampling_onboarding_status",
            "attributes": {
                "service": service,
                "env": env,
                "onboarded": onboarded,
            }
        }
    });
    let data = raw_client::raw_post(
        cfg,
        &format!("{ADAPTIVE_SAMPLING_BASE}/onboarding_status"),
        body,
    )
    .await?;
    formatter::output(cfg, &data)
}

pub async fn adaptive_sampling_onboard(cfg: &Config, service: String, env: String) -> Result<()> {
    post_onboarding(cfg, service, env, true).await
}

pub async fn adaptive_sampling_offboard(cfg: &Config, service: String, env: String) -> Result<()> {
    post_onboarding(cfg, service, env, false).await
}

pub async fn adaptive_sampling_get_allotment(cfg: &Config) -> Result<()> {
    let path = format!("{ADAPTIVE_SAMPLING_BASE}/allotment_config");
    let data = raw_client::raw_get(cfg, &path, &[]).await?;
    formatter::output(cfg, &data)
}

pub async fn adaptive_sampling_set_allotment(
    cfg: &Config,
    bytes: Option<i64>,
    percent: Option<f64>,
) -> Result<()> {
    let attrs = allotment_attributes(bytes, percent);
    let body = serde_json::json!({
        "data": {
            "id": "1",
            "type": "apm_adaptive_sampling_allotment_config",
            "attributes": attrs,
        }
    });
    let data = raw_client::raw_post(
        cfg,
        &format!("{ADAPTIVE_SAMPLING_BASE}/allotment_config"),
        body,
    )
    .await?;
    formatter::output(cfg, &data)
}

pub async fn adaptive_sampling_check(cfg: &Config) -> Result<()> {
    let path = format!("{ADAPTIVE_SAMPLING_BASE}/allotment_check");
    let data = raw_client::raw_get(cfg, &path, &[]).await?;
    formatter::output(cfg, &data)
}

pub async fn adaptive_sampling_preview(
    cfg: &Config,
    bytes: Option<i64>,
    percent: Option<f64>,
) -> Result<()> {
    let attrs = allotment_attributes(bytes, percent);
    let body = serde_json::json!({
        "data": {
            "id": "1",
            "type": "apm_adaptive_sampling_allotment_preview",
            "attributes": attrs,
        }
    });
    let data =
        raw_client::raw_post(cfg, &format!("{ADAPTIVE_SAMPLING_BASE}/preview"), body).await?;
    formatter::output(cfg, &data)
}

pub async fn service_config_get(
    cfg: &Config,
    service_name: String,
    env: Option<String>,
    service_instance_ids: Option<String>,
) -> Result<()> {
    let mut query = vec![("service_name", service_name.as_str())];
    let env_owned;
    if let Some(e) = &env {
        env_owned = e.clone();
        query.push(("env", env_owned.as_str()));
    }
    let ids_owned;
    if let Some(ids) = &service_instance_ids {
        ids_owned = ids.clone();
        query.push(("service_instance_ids", ids_owned.as_str()));
    }
    let data = raw_client::raw_get(cfg, "/api/unstable/apm/service-config", &query).await?;
    formatter::output(cfg, &data)
}

pub async fn service_library_config_get(
    cfg: &Config,
    service_name: String,
    env: Option<String>,
    language: Option<String>,
    mixed: bool,
) -> Result<()> {
    let mut query = vec![("service_name", service_name.as_str())];
    let env_owned;
    if let Some(e) = &env {
        env_owned = e.clone();
        query.push(("env", env_owned.as_str()));
    }
    let lang_owned;
    if let Some(l) = &language {
        lang_owned = l.clone();
        query.push(("language_name", lang_owned.as_str()));
    }
    if mixed {
        query.push(("is_mixed", "true"));
    }
    let data = raw_client::raw_get(cfg, "/api/unstable/apm/service-library-config", &query).await?;
    formatter::output(cfg, &data)
}

#[cfg(test)]
mod tests {

    use crate::test_support::*;

    #[tokio::test]
    async fn test_apm_services_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::services_list(&cfg, "prod".into(), "1h".into(), "now".into()).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_apm_services_resources_uses_from_to() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let mock = server
            .mock("GET", "/api/ui/apm/resources")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("service".into(), "web".into()),
                mockito::Matcher::UrlEncoded("name".into(), "http.request".into()),
                mockito::Matcher::UrlEncoded("env".into(), "prod".into()),
                mockito::Matcher::Regex("from=".into()),
                mockito::Matcher::Regex("to=".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": []}"#)
            .create_async()
            .await;

        let result = super::services_resources(
            &cfg,
            "web".into(),
            "http.request".into(),
            "prod".into(),
            "1h".into(),
            "now".into(),
        )
        .await;
        assert!(
            result.is_ok(),
            "services_resources failed: {:?}",
            result.err()
        );
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_apm_troubleshooting_list() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let mock = server
            .mock("GET", "/api/unstable/apm/instrumentation-errors")
            .match_query(mockito::Matcher::UrlEncoded(
                "hostname".into(),
                "my-host".into(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": []}"#)
            .create_async()
            .await;

        let result = super::troubleshooting_list(&cfg, Some("my-host".into()), None, None).await;
        assert!(
            result.is_ok(),
            "troubleshooting list failed: {:?}",
            result.err()
        );
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_apm_troubleshooting_list_with_timeframe() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let mock = server
            .mock("GET", "/api/unstable/apm/instrumentation-errors")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("hostname".into(), "my-host".into()),
                mockito::Matcher::UrlEncoded("timeframe".into(), "4h".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": []}"#)
            .create_async()
            .await;

        let result =
            super::troubleshooting_list(&cfg, Some("my-host".into()), Some("4h".into()), None)
                .await;
        assert!(
            result.is_ok(),
            "troubleshooting list with timeframe failed: {:?}",
            result.err()
        );
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_apm_troubleshooting_list_org_wide() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let mock = server
            .mock("GET", "/api/unstable/apm/instrumentation-errors")
            .match_query(mockito::Matcher::Missing)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": []}"#)
            .create_async()
            .await;

        let result = super::troubleshooting_list(&cfg, None, None, None).await;
        assert!(
            result.is_ok(),
            "troubleshooting list org-wide failed: {:?}",
            result.err()
        );
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_apm_troubleshooting_list_with_result_filter() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let mock = server
            .mock("GET", "/api/unstable/apm/instrumentation-errors")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("hostname".into(), "my-host".into()),
                mockito::Matcher::UrlEncoded("result".into(), "error,abort".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": []}"#)
            .create_async()
            .await;

        let result = super::troubleshooting_list(
            &cfg,
            Some("my-host".into()),
            None,
            Some("error,abort".into()),
        )
        .await;
        assert!(
            result.is_ok(),
            "troubleshooting list with result filter failed: {:?}",
            result.err()
        );
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_apm_service_config_get() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let mock = server
            .mock("GET", "/api/unstable/apm/service-config")
            .match_query(mockito::Matcher::UrlEncoded(
                "service_name".into(),
                "my-service".into(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"service_name":"my-service","service_configs":[]}"#)
            .create_async()
            .await;

        let result = super::service_config_get(&cfg, "my-service".into(), None, None).await;
        assert!(
            result.is_ok(),
            "service_config_get failed: {:?}",
            result.err()
        );
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_apm_service_config_get_with_filters() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let mock = server
            .mock("GET", "/api/unstable/apm/service-config")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("service_name".into(), "my-service".into()),
                mockito::Matcher::UrlEncoded("env".into(), "prod".into()),
                mockito::Matcher::UrlEncoded("service_instance_ids".into(), "id-1,id-2".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"service_name":"my-service","service_configs":[]}"#)
            .create_async()
            .await;

        let result = super::service_config_get(
            &cfg,
            "my-service".into(),
            Some("prod".into()),
            Some("id-1,id-2".into()),
        )
        .await;
        assert!(
            result.is_ok(),
            "service_config_get with filters failed: {:?}",
            result.err()
        );
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_apm_service_library_config_get() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let mock = server
            .mock("GET", "/api/unstable/apm/service-library-config")
            .match_query(mockito::Matcher::UrlEncoded(
                "service_name".into(),
                "my-service".into(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"service_name":"my-service","configs":[]}"#)
            .create_async()
            .await;

        let result =
            super::service_library_config_get(&cfg, "my-service".into(), None, None, false).await;
        assert!(
            result.is_ok(),
            "service_library_config_get failed: {:?}",
            result.err()
        );
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_apm_service_library_config_get_with_filters() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let mock = server
            .mock("GET", "/api/unstable/apm/service-library-config")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("service_name".into(), "my-service".into()),
                mockito::Matcher::UrlEncoded("env".into(), "prod".into()),
                mockito::Matcher::UrlEncoded("language_name".into(), "python".into()),
                mockito::Matcher::UrlEncoded("is_mixed".into(), "true".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"service_name":"my-service","is_mixed":true,"configs":[]}"#)
            .create_async()
            .await;

        let result = super::service_library_config_get(
            &cfg,
            "my-service".into(),
            Some("prod".into()),
            Some("python".into()),
            true,
        )
        .await;
        assert!(
            result.is_ok(),
            "service_library_config_get with filters failed: {:?}",
            result.err()
        );
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_service_remapping_list() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let mock = server
            .mock("GET", "/api/v2/service-naming-rules")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": []}"#)
            .create_async()
            .await;

        let result = super::service_remapping_list(&cfg).await;
        assert!(
            result.is_ok(),
            "service_remapping_list failed: {:?}",
            result.err()
        );
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_service_remapping_list_api_error() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        server
            .mock("GET", "/api/v2/service-naming-rules")
            .with_status(403)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors": ["Forbidden"]}"#)
            .create_async()
            .await;

        let result = super::service_remapping_list(&cfg).await;
        assert!(result.is_err(), "expected error on 403");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_service_remapping_create() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let mock = server
            .mock("POST", "/api/v2/service-naming-rules")
            .with_status(201)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": {"id": "abc123"}}"#)
            .create_async()
            .await;

        let result = super::service_remapping_create(
            &cfg,
            "my-rule".into(),
            "service:my-svc".into(),
            0,
            "new-name".into(),
        )
        .await;
        assert!(
            result.is_ok(),
            "service_remapping_create failed: {:?}",
            result.err()
        );
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_service_remapping_create_api_error() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        server
            .mock("POST", "/api/v2/service-naming-rules")
            .with_status(422)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors": ["Invalid rule_type"]}"#)
            .create_async()
            .await;

        let result = super::service_remapping_create(
            &cfg,
            "my-rule".into(),
            "service:my-svc".into(),
            99,
            "new-name".into(),
        )
        .await;
        assert!(result.is_err(), "expected error on 422");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_service_remapping_get() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let mock = server
            .mock("GET", "/api/v2/service-naming-rules/abc123")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": {"id": "abc123"}}"#)
            .create_async()
            .await;

        let result = super::service_remapping_get(&cfg, "abc123".into()).await;
        assert!(
            result.is_ok(),
            "service_remapping_get failed: {:?}",
            result.err()
        );
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_service_remapping_get_not_found() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        server
            .mock("GET", "/api/v2/service-naming-rules/missing")
            .with_status(404)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors": ["Not found"]}"#)
            .create_async()
            .await;

        let result = super::service_remapping_get(&cfg, "missing".into()).await;
        assert!(result.is_err(), "expected error on 404");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_service_remapping_update() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let mock = server
            .mock("PUT", "/api/v2/service-naming-rules/abc123")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": {"id": "abc123"}}"#)
            .create_async()
            .await;

        let result = super::service_remapping_update(
            &cfg,
            "abc123".into(),
            "updated-rule".into(),
            "service:my-svc".into(),
            0,
            "new-name".into(),
            2,
        )
        .await;
        assert!(
            result.is_ok(),
            "service_remapping_update failed: {:?}",
            result.err()
        );
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_service_remapping_update_conflict() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        server
            .mock("PUT", "/api/v2/service-naming-rules/abc123")
            .with_status(409)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors": ["Conflict: stale version"]}"#)
            .create_async()
            .await;

        let result = super::service_remapping_update(
            &cfg,
            "abc123".into(),
            "updated-rule".into(),
            "service:my-svc".into(),
            0,
            "new-name".into(),
            1,
        )
        .await;
        assert!(result.is_err(), "expected error on 409");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_service_remapping_delete() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let mock = server
            .mock("DELETE", "/api/v2/service-naming-rules/abc123/2")
            .with_status(204)
            .create_async()
            .await;

        let result = super::service_remapping_delete(&cfg, "abc123".into(), 2).await;
        assert!(
            result.is_ok(),
            "service_remapping_delete failed: {:?}",
            result.err()
        );
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_service_remapping_delete_not_found() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        server
            .mock("DELETE", "/api/v2/service-naming-rules/missing/1")
            .with_status(404)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors": ["Not found"]}"#)
            .create_async()
            .await;

        let result = super::service_remapping_delete(&cfg, "missing".into(), 1).await;
        assert!(result.is_err(), "expected error on 404");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_service_remapping_update_204_no_content() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let mock = server
            .mock("PUT", "/api/v2/service-naming-rules/abc123")
            .with_status(204)
            .create_async()
            .await;

        let result = super::service_remapping_update(
            &cfg,
            "abc123".into(),
            "updated-rule".into(),
            "service:my-svc".into(),
            0,
            "new-name".into(),
            2,
        )
        .await;
        assert!(
            result.is_ok(),
            "204 No Content should not be an error: {:?}",
            result.err()
        );
        mock.assert_async().await;
        cleanup_env();
    }

    // ===== sampling rules =====

    #[tokio::test]
    async fn test_sampling_rules_list() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let mock = server
            .mock(
                "GET",
                "/api/unstable/remote_config/products/apm_tracing/configs",
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": []}"#)
            .create_async()
            .await;

        let result = super::sampling_rules_list(&cfg, None, None).await;
        assert!(
            result.is_ok(),
            "sampling_rules_list failed: {:?}",
            result.err()
        );
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_sampling_rules_list_by_target() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let mock = server
            .mock(
                "GET",
                "/api/unstable/remote_config/products/apm_tracing/configs/by_target?service=api&env=prod",
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": []}"#)
            .create_async()
            .await;

        let result =
            super::sampling_rules_list(&cfg, Some("api".into()), Some("prod".into())).await;
        assert!(
            result.is_ok(),
            "sampling_rules_list by_target failed: {:?}",
            result.err()
        );
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_sampling_rules_get() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let mock = server
            .mock(
                "GET",
                "/api/unstable/remote_config/products/apm_tracing/configs/abc123",
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": {"id": "abc123"}}"#)
            .create_async()
            .await;

        let result = super::sampling_rules_get(&cfg, "abc123".into()).await;
        assert!(
            result.is_ok(),
            "sampling_rules_get failed: {:?}",
            result.err()
        );
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_sampling_rules_get_not_found() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        server
            .mock(
                "GET",
                "/api/unstable/remote_config/products/apm_tracing/configs/missing",
            )
            .with_status(404)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors": ["Not Found"]}"#)
            .create_async()
            .await;

        let result = super::sampling_rules_get(&cfg, "missing".into()).await;
        assert!(result.is_err(), "expected error on 404");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_sampling_rules_create() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let mock = server
            .mock(
                "POST",
                "/api/unstable/remote_config/products/apm_tracing/configs",
            )
            .with_status(201)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": {"id": "new-config-id"}}"#)
            .create_async()
            .await;

        let result =
            super::sampling_rules_create(&cfg, "api".into(), "prod".into(), "*".into(), 0.1).await;
        assert!(
            result.is_ok(),
            "sampling_rules_create failed: {:?}",
            result.err()
        );
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_sampling_rules_create_api_error() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        server
            .mock(
                "POST",
                "/api/unstable/remote_config/products/apm_tracing/configs",
            )
            .with_status(422)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors": ["Invalid sample_rate"]}"#)
            .create_async()
            .await;

        let result =
            super::sampling_rules_create(&cfg, "api".into(), "prod".into(), "*".into(), -1.0).await;
        assert!(result.is_err(), "expected error on 422");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_sampling_rules_update() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let mock = server
            .mock(
                "PUT",
                "/api/unstable/remote_config/products/apm_tracing/configs/abc123",
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": {"id": "abc123"}}"#)
            .create_async()
            .await;

        let result = super::sampling_rules_update(
            &cfg,
            "abc123".into(),
            "api".into(),
            "prod".into(),
            "*".into(),
            0.5,
        )
        .await;
        assert!(
            result.is_ok(),
            "sampling_rules_update failed: {:?}",
            result.err()
        );
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_sampling_rules_delete() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let mock = server
            .mock(
                "DELETE",
                "/api/unstable/remote_config/products/apm_tracing/configs/abc123",
            )
            .with_status(204)
            .create_async()
            .await;

        let result = super::sampling_rules_delete(&cfg, "abc123".into()).await;
        assert!(
            result.is_ok(),
            "sampling_rules_delete failed: {:?}",
            result.err()
        );
        mock.assert_async().await;
        cleanup_env();
    }

    // ===== adaptive sampling =====

    #[tokio::test]
    async fn test_adaptive_sampling_onboarding_status_no_filter() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let mock = server
            .mock("GET", "/api/ui/adaptive_sampling/onboarding_status")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": []}"#)
            .create_async()
            .await;

        let result = super::adaptive_sampling_onboarding_status(&cfg, None, None).await;
        assert!(
            result.is_ok(),
            "adaptive_sampling_onboarding_status failed: {:?}",
            result.err()
        );
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_adaptive_sampling_onboarding_status_with_filter() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let mock = server
            .mock(
                "GET",
                "/api/ui/adaptive_sampling/onboarding_status?service=api&env=prod",
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": {"onboarded": true}}"#)
            .create_async()
            .await;

        let result = super::adaptive_sampling_onboarding_status(
            &cfg,
            Some("api".into()),
            Some("prod".into()),
        )
        .await;
        assert!(
            result.is_ok(),
            "adaptive_sampling_onboarding_status with filter failed: {:?}",
            result.err()
        );
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_adaptive_sampling_onboard() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let mock = server
            .mock("POST", "/api/ui/adaptive_sampling/onboarding_status")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": {"onboarded": true}}"#)
            .create_async()
            .await;

        let result = super::adaptive_sampling_onboard(&cfg, "api".into(), "prod".into()).await;
        assert!(
            result.is_ok(),
            "adaptive_sampling_onboard failed: {:?}",
            result.err()
        );
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_adaptive_sampling_offboard() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let mock = server
            .mock("POST", "/api/ui/adaptive_sampling/onboarding_status")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": {"onboarded": false}}"#)
            .create_async()
            .await;

        let result = super::adaptive_sampling_offboard(&cfg, "api".into(), "prod".into()).await;
        assert!(
            result.is_ok(),
            "adaptive_sampling_offboard failed: {:?}",
            result.err()
        );
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_adaptive_sampling_get_allotment() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let mock = server
            .mock("GET", "/api/ui/adaptive_sampling/allotment_config")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": {"strategy": "fixed_target", "allotment_bytes": 100000}}"#)
            .create_async()
            .await;

        let result = super::adaptive_sampling_get_allotment(&cfg).await;
        assert!(
            result.is_ok(),
            "adaptive_sampling_get_allotment failed: {:?}",
            result.err()
        );
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_adaptive_sampling_set_allotment_bytes() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let mock = server
            .mock("POST", "/api/ui/adaptive_sampling/allotment_config")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": {}}"#)
            .create_async()
            .await;

        let result = super::adaptive_sampling_set_allotment(&cfg, Some(100_000), None).await;
        assert!(
            result.is_ok(),
            "adaptive_sampling_set_allotment with bytes failed: {:?}",
            result.err()
        );
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_adaptive_sampling_set_allotment_percent() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let mock = server
            .mock("POST", "/api/ui/adaptive_sampling/allotment_config")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": {}}"#)
            .create_async()
            .await;

        let result = super::adaptive_sampling_set_allotment(&cfg, None, Some(50.0)).await;
        assert!(
            result.is_ok(),
            "adaptive_sampling_set_allotment with percent failed: {:?}",
            result.err()
        );
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_adaptive_sampling_check() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let mock = server
            .mock("GET", "/api/ui/adaptive_sampling/allotment_check")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"data": {"allotment_bytes": 100000, "ingested_bytes": 50000, "projected_monthly_ingested_bytes": 150000}}"#,
            )
            .create_async()
            .await;

        let result = super::adaptive_sampling_check(&cfg).await;
        assert!(
            result.is_ok(),
            "adaptive_sampling_check failed: {:?}",
            result.err()
        );
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_adaptive_sampling_preview() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let mock = server
            .mock("POST", "/api/ui/adaptive_sampling/preview")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": {"monthly_quota": 100000, "monthly_target": 50000}}"#)
            .create_async()
            .await;

        let result = super::adaptive_sampling_preview(&cfg, Some(50_000), None).await;
        assert!(
            result.is_ok(),
            "adaptive_sampling_preview failed: {:?}",
            result.err()
        );
        mock.assert_async().await;
        cleanup_env();
    }
}
