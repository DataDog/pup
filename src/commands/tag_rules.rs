use anyhow::Result;
use datadog_api_client::datadogV2::api_tag_rules::{
    DeleteTagRuleOptionalParams, GetTagRuleOptionalParams, GetTagRuleScoreOptionalParams,
    ListTagRulesOptionalParams, TagRulesAPI,
};
use datadog_api_client::datadogV2::model::{TagRuleCreateRequest, TagRuleUpdateRequest};

use crate::config::Config;
use crate::formatter;
use crate::util;

fn make_api(cfg: &Config) -> TagRulesAPI {
    crate::make_api!(TagRulesAPI, cfg)
}

pub async fn list(
    cfg: &Config,
    include_disabled: bool,
    include_deleted: bool,
    include_score: bool,
    filter_source: Option<String>,
) -> Result<()> {
    let api = make_api(cfg);
    let mut params = ListTagRulesOptionalParams::default();
    if include_disabled {
        params = params.include_disabled(true);
    }
    if include_deleted {
        params = params.include_deleted(true);
    }
    if include_score {
        params = params.include(datadog_api_client::datadogV2::model::TagRuleInclude::SCORE);
    }
    if let Some(src) = filter_source {
        let source = match src.as_str() {
            "logs" => datadog_api_client::datadogV2::model::TagRuleSource::LOGS,
            "spans" => datadog_api_client::datadogV2::model::TagRuleSource::SPANS,
            "metrics" => datadog_api_client::datadogV2::model::TagRuleSource::METRICS,
            "rum" => datadog_api_client::datadogV2::model::TagRuleSource::RUM,
            "feed" => datadog_api_client::datadogV2::model::TagRuleSource::FEED,
            other => anyhow::bail!(
                "unknown source '{other}'; valid values: logs, spans, metrics, rum, feed"
            ),
        };
        params = params.filter_source(source);
    }
    let resp = api
        .list_tag_rules(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list tag rules: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn get(cfg: &Config, rule_id: &str, include_score: bool) -> Result<()> {
    let api = make_api(cfg);
    let mut params = GetTagRuleOptionalParams::default();
    if include_score {
        params = params.include(datadog_api_client::datadogV2::model::TagRuleInclude::SCORE);
    }
    let resp = api
        .get_tag_rule(rule_id.to_string(), params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get tag rule: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn create(cfg: &Config, file: &str) -> Result<()> {
    let api = make_api(cfg);
    let body: TagRuleCreateRequest = util::read_json_file(file)?;
    let resp = api
        .create_tag_rule(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create tag rule: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn update(cfg: &Config, rule_id: &str, file: &str) -> Result<()> {
    let api = make_api(cfg);
    let body: TagRuleUpdateRequest = util::read_json_file(file)?;
    let resp = api
        .update_tag_rule(rule_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update tag rule: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn delete(cfg: &Config, rule_id: &str, hard_delete: bool) -> Result<()> {
    let api = make_api(cfg);
    let mut params = DeleteTagRuleOptionalParams::default();
    if hard_delete {
        params = params.hard_delete(true);
    }
    api.delete_tag_rule(rule_id.to_string(), params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete tag rule: {e:?}"))?;
    println!("Tag rule {rule_id} deleted.");
    Ok(())
}

pub async fn score(
    cfg: &Config,
    rule_id: &str,
    ts_start: Option<i64>,
    ts_end: Option<i64>,
) -> Result<()> {
    let api = make_api(cfg);
    let mut params = GetTagRuleScoreOptionalParams::default();
    if let Some(s) = ts_start {
        params = params.ts_start(s);
    }
    if let Some(e) = ts_end {
        params = params.ts_end(e);
    }
    let resp = api
        .get_tag_rule_score(rule_id.to_string(), params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get tag rule score: {e:?}"))?;
    formatter::output(cfg, &resp)
}

#[cfg(test)]
mod tests {
    use crate::test_support::*;

    const RULE_BODY: &str = r#"{"data":{"id":"rule-1","type":"tag_rule","attributes":{"created_at":"2024-01-01T00:00:00Z","created_by":"u","enabled":true,"modified_at":"2024-01-01T00:00:00Z","modified_by":"u","name":"p","negated":false,"required":true,"rule_type":"blocking","scope":"org","source":"logs","tag_key":"env","tag_value_patterns":[],"version":1}}}"#;

    #[tokio::test]
    async fn test_tag_rules_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data":[]}"#).await;
        let result = super::list(&cfg, false, false, false, None).await;
        assert!(result.is_ok(), "tag rules list failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_tag_rules_list_error() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        s.mock("GET", mockito::Matcher::Any)
            .with_status(403)
            .with_body(r#"{"errors":["Forbidden"]}"#)
            .create_async()
            .await;
        let result = super::list(&cfg, false, false, false, None).await;
        assert!(result.is_err(), "tag rules list should fail on 403");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_tag_rules_list_valid_source() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data":[]}"#).await;
        for src in ["logs", "spans", "metrics", "rum", "feed"] {
            let result = super::list(&cfg, true, true, true, Some(src.to_string())).await;
            assert!(result.is_ok(), "source '{src}' failed: {:?}", result.err());
        }
        cleanup_env();
    }

    #[tokio::test]
    async fn test_tag_rules_list_invalid_source() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data":[]}"#).await;
        let result = super::list(&cfg, false, false, false, Some("invalid".to_string())).await;
        assert!(result.is_err(), "unknown source should fail");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_tag_rules_get() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, RULE_BODY).await;
        let result = super::get(&cfg, "rule-1", false).await;
        assert!(result.is_ok(), "tag rules get failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_tag_rules_get_error() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        s.mock("GET", mockito::Matcher::Any)
            .with_status(404)
            .with_body(r#"{"errors":["not found"]}"#)
            .create_async()
            .await;
        let result = super::get(&cfg, "missing", false).await;
        assert!(result.is_err(), "get should fail for missing rule");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_tag_rules_create() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, RULE_BODY).await;
        let tmp = write_temp_json(
            "tag_rule_create.json",
            r#"{"data":{"type":"tag_rule","attributes":{"name":"p","rule_type":"surfacing","scope":"org","source":"logs","tag_key":"env","tag_value_patterns":[]}}}"#,
        );
        let result = super::create(&cfg, tmp.to_str().unwrap()).await;
        assert!(
            result.is_ok(),
            "tag rules create failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_tag_rules_create_bad_file() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, "{}").await;
        let result = super::create(&cfg, "/nonexistent/file.json").await;
        assert!(result.is_err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_tag_rules_update() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, RULE_BODY).await;
        let tmp = write_temp_json(
            "tag_rule_update.json",
            r#"{"data":{"id":"rule-1","type":"tag_rule","attributes":{"tag_key":"env","rule_type":"blocking"}}}"#,
        );
        let result = super::update(&cfg, "rule-1", tmp.to_str().unwrap()).await;
        assert!(
            result.is_ok(),
            "tag rules update failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_tag_rules_delete() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, "").await;
        let result = super::delete(&cfg, "rule-1", false).await;
        assert!(
            result.is_ok(),
            "tag rules delete failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_tag_rules_delete_error() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        s.mock("DELETE", mockito::Matcher::Any)
            .with_status(404)
            .with_body(r#"{"errors":["not found"]}"#)
            .create_async()
            .await;
        let result = super::delete(&cfg, "missing", false).await;
        assert!(result.is_err(), "delete should fail for missing rule");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_tag_rules_score() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data":{"id":"rule-1","type":"tag_rule_score","attributes":{"score":null,"ts_start":0,"ts_end":0,"version":1}}}"#).await;
        let result = super::score(&cfg, "rule-1", Some(0), Some(1)).await;
        assert!(result.is_ok(), "tag rules score failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_tag_rules_score_error() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        s.mock("GET", mockito::Matcher::Any)
            .with_status(403)
            .with_body(r#"{"errors":["Forbidden"]}"#)
            .create_async()
            .await;
        let result = super::score(&cfg, "rule-1", None, None).await;
        assert!(result.is_err(), "score should fail on 403");
        cleanup_env();
    }
}
