use anyhow::Result;

use crate::client;
use crate::config::Config;
use crate::formatter;
use crate::util;

pub async fn services_list(cfg: &Config, env: String, from: String, to: String) -> Result<()> {
    let from_ts = util::parse_time_to_unix(&from)?;
    let to_ts = util::parse_time_to_unix(&to)?;
    let path = format!("/api/v2/apm/services?start={from_ts}&end={to_ts}&filter[env]={env}");
    let data = client::raw_get(cfg, &path, &[]).await?;
    formatter::output(cfg, &data)
}

pub async fn services_stats(cfg: &Config, env: String, from: String, to: String) -> Result<()> {
    let from_ts = util::parse_time_to_unix(&from)?;
    let to_ts = util::parse_time_to_unix(&to)?;
    let path = format!("/api/v2/apm/services/stats?start={from_ts}&end={to_ts}&filter[env]={env}");
    let data = client::raw_get(cfg, &path, &[]).await?;
    formatter::output(cfg, &data)
}

pub async fn entities_list(cfg: &Config, from: String, to: String) -> Result<()> {
    let from_ts = util::parse_time_to_unix(&from)?;
    let to_ts = util::parse_time_to_unix(&to)?;
    let path = format!("/api/unstable/apm/entities?start={from_ts}&end={to_ts}");
    let data = client::raw_get(cfg, &path, &[]).await?;
    formatter::output(cfg, &data)
}

pub async fn dependencies_list(cfg: &Config, env: String, from: String, to: String) -> Result<()> {
    let from_ts = util::parse_time_to_unix(&from)?;
    let to_ts = util::parse_time_to_unix(&to)?;
    let path = format!("/api/v1/service_dependencies?start={from_ts}&end={to_ts}&env={env}");
    let data = client::raw_get(cfg, &path, &[]).await?;
    formatter::output(cfg, &data)
}

pub async fn services_operations(
    cfg: &Config,
    service: String,
    env: String,
    from: String,
    to: String,
) -> Result<()> {
    let from_ts = util::parse_time_to_unix(&from)?;
    let to_ts = util::parse_time_to_unix(&to)?;
    let path =
        format!("/api/v1/trace/operation_names/{service}?env={env}&start={from_ts}&end={to_ts}");
    let data = client::raw_get(cfg, &path, &[]).await?;
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
    let from_ts = util::parse_time_to_unix(&from)?;
    let to_ts = util::parse_time_to_unix(&to)?;
    let path = format!(
        "/api/ui/apm/resources?service={service}&name={name}&env={env}&from={from_ts}&to={to_ts}"
    );
    let data = client::raw_get(cfg, &path, &[]).await?;
    formatter::output(cfg, &data)
}

pub async fn flow_map(
    cfg: &Config,
    query: String,
    limit: i64,
    from: String,
    to: String,
) -> Result<()> {
    let from_ts = util::parse_time_to_unix(&from)?;
    let to_ts = util::parse_time_to_unix(&to)?;
    let path =
        format!("/api/ui/apm/flow-map?query={query}&limit={limit}&start={from_ts}&end={to_ts}");
    let data = client::raw_get(cfg, &path, &[]).await?;
    formatter::output(cfg, &data)
}

pub async fn troubleshooting_list(
    cfg: &Config,
    hostname: String,
    timeframe: Option<String>,
) -> Result<()> {
    let path = "/api/unstable/apm/instrumentation-errors";
    let mut query = vec![("hostname", hostname.as_str())];
    let tf_owned;
    if let Some(tf) = &timeframe {
        tf_owned = tf.clone();
        query.push(("timeframe", tf_owned.as_str()));
    }
    let data = client::raw_get(cfg, path, &query).await?;
    formatter::output(cfg, &data)
}

pub async fn service_remapping_list(cfg: &Config) -> Result<()> {
    let data = client::raw_get(cfg, "/api/v2/service-naming-rules", &[]).await?;
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
    let data = client::raw_post(cfg, "/api/v2/service-naming-rules", body).await?;
    formatter::output(cfg, &data)
}

pub async fn service_remapping_get(cfg: &Config, id: String) -> Result<()> {
    let data = client::raw_get(cfg, &format!("/api/v2/service-naming-rules/{id}"), &[]).await?;
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
    let data = client::raw_put(cfg, &format!("/api/v2/service-naming-rules/{id}"), body).await?;
    formatter::output(cfg, &data)
}

pub async fn service_remapping_delete(cfg: &Config, id: String, version: i64) -> Result<()> {
    client::raw_delete(cfg, &format!("/api/v2/service-naming-rules/{id}/{version}")).await
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
    let data = client::raw_get(cfg, "/api/unstable/apm/service-config", &query).await?;
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
    let data = client::raw_get(cfg, "/api/unstable/apm/service-library-config", &query).await?;
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

        let result = super::troubleshooting_list(&cfg, "my-host".into(), None).await;
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

        let result = super::troubleshooting_list(&cfg, "my-host".into(), Some("4h".into())).await;
        assert!(
            result.is_ok(),
            "troubleshooting list with timeframe failed: {:?}",
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
}
