use anyhow::Result;
use datadog_api_client::datadogV2::api_tag_policies::{
    DeleteTagPolicyOptionalParams, GetTagPolicyOptionalParams, GetTagPolicyScoreOptionalParams,
    ListTagPoliciesOptionalParams, TagPoliciesAPI,
};
use datadog_api_client::datadogV2::model::{TagPolicyCreateRequest, TagPolicyUpdateRequest};

use crate::config::Config;
use crate::formatter;
use crate::util;

fn make_api(cfg: &Config) -> TagPoliciesAPI {
    crate::make_api!(TagPoliciesAPI, cfg)
}

pub async fn list(
    cfg: &Config,
    include_disabled: bool,
    include_deleted: bool,
    include_score: bool,
    filter_source: Option<String>,
) -> Result<()> {
    let api = make_api(cfg);
    let mut params = ListTagPoliciesOptionalParams::default();
    if include_disabled {
        params = params.include_disabled(true);
    }
    if include_deleted {
        params = params.include_deleted(true);
    }
    if include_score {
        params = params.include(datadog_api_client::datadogV2::model::TagPolicyInclude::SCORE);
    }
    if let Some(src) = filter_source {
        let source = match src.as_str() {
            "logs" => datadog_api_client::datadogV2::model::TagPolicySource::LOGS,
            "spans" => datadog_api_client::datadogV2::model::TagPolicySource::SPANS,
            "metrics" => datadog_api_client::datadogV2::model::TagPolicySource::METRICS,
            "rum" => datadog_api_client::datadogV2::model::TagPolicySource::RUM,
            "feed" => datadog_api_client::datadogV2::model::TagPolicySource::FEED,
            other => anyhow::bail!(
                "unknown source '{other}'; valid values: logs, spans, metrics, rum, feed"
            ),
        };
        params = params.filter_source(source);
    }
    let resp = api
        .list_tag_policies(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list tag policies: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn get(cfg: &Config, policy_id: &str, include_score: bool) -> Result<()> {
    let api = make_api(cfg);
    let mut params = GetTagPolicyOptionalParams::default();
    if include_score {
        params = params.include(datadog_api_client::datadogV2::model::TagPolicyInclude::SCORE);
    }
    let resp = api
        .get_tag_policy(policy_id.to_string(), params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get tag policy: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn create(cfg: &Config, file: &str) -> Result<()> {
    let api = make_api(cfg);
    let body: TagPolicyCreateRequest = util::read_json_file(file)?;
    let resp = api
        .create_tag_policy(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create tag policy: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn update(cfg: &Config, policy_id: &str, file: &str) -> Result<()> {
    let api = make_api(cfg);
    let body: TagPolicyUpdateRequest = util::read_json_file(file)?;
    let resp = api
        .update_tag_policy(policy_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update tag policy: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn delete(cfg: &Config, policy_id: &str, hard_delete: bool) -> Result<()> {
    let api = make_api(cfg);
    let mut params = DeleteTagPolicyOptionalParams::default();
    if hard_delete {
        params = params.hard_delete(true);
    }
    api.delete_tag_policy(policy_id.to_string(), params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete tag policy: {e:?}"))?;
    println!("Tag policy {policy_id} deleted.");
    Ok(())
}

pub async fn score(
    cfg: &Config,
    policy_id: &str,
    ts_start: Option<i64>,
    ts_end: Option<i64>,
) -> Result<()> {
    let api = make_api(cfg);
    let mut params = GetTagPolicyScoreOptionalParams::default();
    if let Some(s) = ts_start {
        params = params.ts_start(s);
    }
    if let Some(e) = ts_end {
        params = params.ts_end(e);
    }
    let resp = api
        .get_tag_policy_score(policy_id.to_string(), params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get tag policy score: {e:?}"))?;
    formatter::output(cfg, &resp)
}

#[cfg(test)]
mod tests {
    use crate::test_support::*;

    #[tokio::test]
    async fn test_tag_policies_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data":[]}"#).await;
        let result = super::list(&cfg, false, false, false, None).await;
        assert!(
            result.is_ok(),
            "tag policies list failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_tag_policies_list_error() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        s.mock("GET", mockito::Matcher::Any)
            .with_status(403)
            .with_body(r#"{"errors":["Forbidden"]}"#)
            .create_async()
            .await;
        let result = super::list(&cfg, false, false, false, None).await;
        assert!(result.is_err(), "tag policies list should fail on 403");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_tag_policies_list_invalid_source() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data":[]}"#).await;
        let result = super::list(&cfg, false, false, false, Some("invalid".to_string())).await;
        assert!(result.is_err(), "unknown source should fail");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_tag_policies_get() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(
            &mut s,
            r#"{"data":{"id":"pol-1","type":"tag_policies","attributes":{"created_at":"2024-01-01T00:00:00Z","created_by":"u","enabled":true,"modified_at":"2024-01-01T00:00:00Z","modified_by":"u","negated":false,"policy_name":"p","policy_type":"blocking","required":true,"scope":"org","source":"logs","tag_key":"env","tag_value_patterns":[],"version":1}}}"#,
        )
        .await;
        let result = super::get(&cfg, "pol-1", false).await;
        assert!(
            result.is_ok(),
            "tag policies get failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_tag_policies_get_error() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        s.mock("GET", mockito::Matcher::Any)
            .with_status(404)
            .with_body(r#"{"errors":["not found"]}"#)
            .create_async()
            .await;
        let result = super::get(&cfg, "missing", false).await;
        assert!(result.is_err(), "get should fail for missing policy");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_tag_policies_create() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(
            &mut s,
            r#"{"data":{"id":"pol-1","type":"tag_policies","attributes":{"created_at":"2024-01-01T00:00:00Z","created_by":"u","enabled":true,"modified_at":"2024-01-01T00:00:00Z","modified_by":"u","negated":false,"policy_name":"p","policy_type":"blocking","required":true,"scope":"org","source":"logs","tag_key":"env","tag_value_patterns":[],"version":1}}}"#,
        )
        .await;
        let tmp = write_temp_json(
            "tag_policy_create.json",
            r#"{"data":{"type":"tag_policies","attributes":{"policy_name":"p","policy_type":"surfacing","scope":"org","source":"logs","tag_key":"env","tag_value_patterns":[]}}}"#,
        );
        let result = super::create(&cfg, tmp.to_str().unwrap()).await;
        assert!(
            result.is_ok(),
            "tag policies create failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_tag_policies_create_bad_file() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, "{}").await;
        let result = super::create(&cfg, "/nonexistent/file.json").await;
        assert!(result.is_err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_tag_policies_update() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(
            &mut s,
            r#"{"data":{"id":"pol-1","type":"tag_policies","attributes":{"created_at":"2024-01-01T00:00:00Z","created_by":"u","enabled":true,"modified_at":"2024-01-01T00:00:00Z","modified_by":"u","negated":false,"policy_name":"p","policy_type":"blocking","required":true,"scope":"org","source":"logs","tag_key":"env","tag_value_patterns":[],"version":1}}}"#,
        )
        .await;
        let tmp = write_temp_json(
            "tag_policy_update.json",
            r#"{"data":{"id":"pol-1","type":"tag_policies","attributes":{"tag_key":"env","policy_type":"require"}}}"#,
        );
        let result = super::update(&cfg, "pol-1", tmp.to_str().unwrap()).await;
        assert!(
            result.is_ok(),
            "tag policies update failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_tag_policies_delete() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, "").await;
        let result = super::delete(&cfg, "pol-1", false).await;
        assert!(
            result.is_ok(),
            "tag policies delete failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_tag_policies_delete_error() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        s.mock("DELETE", mockito::Matcher::Any)
            .with_status(404)
            .with_body(r#"{"errors":["not found"]}"#)
            .create_async()
            .await;
        let result = super::delete(&cfg, "missing", false).await;
        assert!(result.is_err(), "delete should fail for missing policy");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_tag_policies_score() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data":{"id":"pol-1","type":"tag_policy_score","attributes":{"score":null,"ts_start":0,"ts_end":0,"version":1}}}"#).await;
        let result = super::score(&cfg, "pol-1", None, None).await;
        assert!(
            result.is_ok(),
            "tag policies score failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_tag_policies_score_error() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        s.mock("GET", mockito::Matcher::Any)
            .with_status(403)
            .with_body(r#"{"errors":["Forbidden"]}"#)
            .create_async()
            .await;
        let result = super::score(&cfg, "pol-1", None, None).await;
        assert!(result.is_err(), "score should fail on 403");
        cleanup_env();
    }
}
