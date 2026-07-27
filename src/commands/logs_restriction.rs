use anyhow::Result;
use datadog_api_client::datadogV2::api_logs_restriction_queries::{
    ListRestrictionQueriesOptionalParams, ListRestrictionQueryRolesOptionalParams,
    LogsRestrictionQueriesAPI,
};

use crate::config::Config;
use crate::formatter;
use crate::util;

pub async fn list(cfg: &Config) -> Result<()> {
    let api = crate::make_api!(LogsRestrictionQueriesAPI, cfg);
    let resp = api
        .list_restriction_queries(ListRestrictionQueriesOptionalParams::default())
        .await
        .map_err(|e| anyhow::anyhow!("failed to list restriction queries: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn get(cfg: &Config, query_id: &str) -> Result<()> {
    let api = crate::make_api!(LogsRestrictionQueriesAPI, cfg);
    let resp = api
        .get_restriction_query(query_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get restriction query: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn create(cfg: &Config, file: &str) -> Result<()> {
    let api = crate::make_api!(LogsRestrictionQueriesAPI, cfg);
    let body = util::read_json_file(file)?;
    let resp = api
        .create_restriction_query(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create restriction query: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn update(cfg: &Config, query_id: &str, file: &str) -> Result<()> {
    let api = crate::make_api!(LogsRestrictionQueriesAPI, cfg);
    let body = util::read_json_file(file)?;
    let resp = api
        .update_restriction_query(query_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update restriction query: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn delete(cfg: &Config, query_id: &str) -> Result<()> {
    let api = crate::make_api!(LogsRestrictionQueriesAPI, cfg);
    api.delete_restriction_query(query_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete restriction query: {e:?}"))?;
    println!("Restriction query '{query_id}' deleted successfully.");
    Ok(())
}

pub async fn roles_list(cfg: &Config, query_id: &str) -> Result<()> {
    let api = crate::make_api!(LogsRestrictionQueriesAPI, cfg);
    let resp = api
        .list_restriction_query_roles(
            query_id.to_string(),
            ListRestrictionQueryRolesOptionalParams::default(),
        )
        .await
        .map_err(|e| anyhow::anyhow!("failed to list restriction query roles: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn roles_add(cfg: &Config, query_id: &str, file: &str) -> Result<()> {
    let api = crate::make_api!(LogsRestrictionQueriesAPI, cfg);
    let body = util::read_json_file(file)?;
    api.add_role_to_restriction_query(query_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to add role to restriction query: {e:?}"))?;
    println!("Role added to restriction query '{query_id}'.");
    Ok(())
}

#[cfg(test)]
mod tests {

    use crate::test_support::*;

    #[tokio::test]
    async fn test_logs_restriction_list() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(
            &mut server,
            "GET",
            r#"{"data":[],"meta":{"page":{"total_count":0}}}"#,
        )
        .await;
        let result = super::list(&cfg).await;
        assert!(
            result.is_ok(),
            "logs_restriction list failed: {:?}",
            result.err()
        );
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_logs_restriction_delete_missing_id() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = server
            .mock("DELETE", mockito::Matcher::Any)
            .with_status(404)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors":["Not Found"]}"#)
            .create_async()
            .await;
        let result = super::delete(&cfg, "nonexistent-id").await;
        assert!(
            result.is_err(),
            "expected error for missing restriction query"
        );
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }
}
