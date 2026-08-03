use anyhow::Result;
use datadog_api_client::datadogV2::api_data_deletion::{
    DataDeletionAPI, GetDataDeletionRequestsOptionalParams,
};

use crate::config::Config;
use crate::formatter;
use crate::util;

fn make_api(cfg: &Config) -> DataDeletionAPI {
    crate::make_api!(DataDeletionAPI, cfg)
}

pub async fn requests_list(
    cfg: &Config,
    product: Option<String>,
    query: Option<String>,
    status: Option<String>,
) -> Result<()> {
    let api = make_api(cfg);
    let mut params = GetDataDeletionRequestsOptionalParams::default();
    if let Some(p) = product {
        params = params.product(p);
    }
    if let Some(q) = query {
        params = params.query(q);
    }
    if let Some(s) = status {
        params = params.status(s);
    }
    let resp = api
        .get_data_deletion_requests(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list data deletion requests: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn requests_create(cfg: &Config, product: &str, file: &str) -> Result<()> {
    let body: datadog_api_client::datadogV2::model::CreateDataDeletionRequestBody =
        util::read_json_file(file)?;
    let api = make_api(cfg);
    let resp = api
        .create_data_deletion_request(product.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create data deletion request: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn requests_cancel(cfg: &Config, request_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let resp = api
        .cancel_data_deletion_request(request_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to cancel data deletion request: {e:?}"))?;
    formatter::output(cfg, &resp)
}

#[cfg(test)]
mod tests {

    use crate::test_support::*;

    #[tokio::test]
    async fn test_data_deletion_requests_list() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "GET", r#"{"data":[]}"#).await;
        let result = super::requests_list(&cfg, None, None, None).await;
        assert!(
            result.is_ok(),
            "data deletion requests list failed: {:?}",
            result.err()
        );
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_data_deletion_requests_list_with_filters() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let mock = server
            .mock("GET", mockito::Matcher::Any)
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("product".into(), "logs".into()),
                mockito::Matcher::UrlEncoded("status".into(), "pending".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data":[]}"#)
            .create_async()
            .await;
        let result =
            super::requests_list(&cfg, Some("logs".into()), None, Some("pending".into())).await;
        assert!(
            result.is_ok(),
            "data deletion requests list with filters failed: {:?}",
            result.err()
        );
        mock.assert_async().await;
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }
}
