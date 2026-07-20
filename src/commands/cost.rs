use anyhow::Result;
use datadog_api_client::datadogV2::api_cloud_cost_management::{
    CloudCostManagementAPI, ListCostAnomaliesOptionalParams,
};
use datadog_api_client::datadogV2::api_usage_metering::{
    GetCostByOrgOptionalParams, GetMonthlyCostAttributionOptionalParams,
    GetProjectedCostOptionalParams, UsageMeteringAPI as UsageMeteringV2API,
};

use crate::config::Config;
use crate::formatter;
use crate::util;
use crate::util_ext;

fn make_usage_api(cfg: &Config) -> UsageMeteringV2API {
    crate::make_api!(UsageMeteringV2API, cfg)
}

pub async fn projected(cfg: &Config) -> Result<()> {
    let api = make_usage_api(cfg);
    let resp = api
        .get_projected_cost(GetProjectedCostOptionalParams::default())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get projected cost: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn by_org(cfg: &Config, start_month: String, end_month: Option<String>) -> Result<()> {
    let api = make_usage_api(cfg);

    let start_dt = util_ext::parse_time_to_datetime(&start_month)?;

    let mut params = GetCostByOrgOptionalParams::default();
    if let Some(e) = end_month {
        let end_dt = util_ext::parse_time_to_datetime(&e)?;
        params = params.end_month(end_dt);
    }

    let resp = api
        .get_cost_by_org(start_dt, params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get cost by org: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn attribution(cfg: &Config, start: String, fields: Option<String>) -> Result<()> {
    let api = make_usage_api(cfg);

    let start_dt = util_ext::parse_time_to_datetime(&start)?;

    let fields_str = fields.unwrap_or_else(|| "*".to_string());
    let params = GetMonthlyCostAttributionOptionalParams::default();

    let resp = api
        .get_monthly_cost_attribution(start_dt, fields_str, params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get cost attribution: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---- Cloud Cost Management — AWS CUR Config ----

fn make_ccm_api(cfg: &Config) -> CloudCostManagementAPI {
    crate::make_api!(CloudCostManagementAPI, cfg)
}

pub async fn aws_config_list(cfg: &Config) -> Result<()> {
    let api = make_ccm_api(cfg);
    let resp = api
        .list_cost_awscur_configs()
        .await
        .map_err(|e| anyhow::anyhow!("failed to list AWS CUR configs: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn aws_config_get(cfg: &Config, id: i64) -> Result<()> {
    let api = make_ccm_api(cfg);
    let resp = api
        .get_cost_awscur_config(id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get AWS CUR config: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn aws_config_create(cfg: &Config, file: &str) -> Result<()> {
    let body: datadog_api_client::datadogV2::model::AwsCURConfigPostRequest =
        util::read_json_file(file)?;
    let api = make_ccm_api(cfg);
    let resp = api
        .create_cost_awscur_config(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create AWS CUR config: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn aws_config_delete(cfg: &Config, id: i64) -> Result<()> {
    let api = make_ccm_api(cfg);
    api.delete_cost_awscur_config(id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete AWS CUR config: {e:?}"))?;
    eprintln!("AWS CUR config {id} deleted.");
    Ok(())
}

// ---- Cloud Cost Management — Azure UC Config ----

pub async fn azure_config_list(cfg: &Config) -> Result<()> {
    let api = make_ccm_api(cfg);
    let resp = api
        .list_cost_azure_uc_configs()
        .await
        .map_err(|e| anyhow::anyhow!("failed to list Azure UC configs: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn azure_config_get(cfg: &Config, id: i64) -> Result<()> {
    let api = make_ccm_api(cfg);
    let resp = api
        .get_cost_azure_uc_config(id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get Azure UC config: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn azure_config_create(cfg: &Config, file: &str) -> Result<()> {
    let body: datadog_api_client::datadogV2::model::AzureUCConfigPostRequest =
        util::read_json_file(file)?;
    let api = make_ccm_api(cfg);
    let resp = api
        .create_cost_azure_uc_configs(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create Azure UC config: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn azure_config_delete(cfg: &Config, id: i64) -> Result<()> {
    let api = make_ccm_api(cfg);
    api.delete_cost_azure_uc_config(id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete Azure UC config: {e:?}"))?;
    eprintln!("Azure UC config {id} deleted.");
    Ok(())
}

// ---- Cloud Cost Management — GCP Usage Cost Config ----

pub async fn gcp_config_list(cfg: &Config) -> Result<()> {
    let api = make_ccm_api(cfg);
    let resp = api
        .list_cost_gcp_usage_cost_configs()
        .await
        .map_err(|e| anyhow::anyhow!("failed to list GCP usage cost configs: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn gcp_config_get(cfg: &Config, id: i64) -> Result<()> {
    let api = make_ccm_api(cfg);
    let resp = api
        .get_cost_gcp_usage_cost_config(id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get GCP usage cost config: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn gcp_config_create(cfg: &Config, file: &str) -> Result<()> {
    let body: datadog_api_client::datadogV2::model::GCPUsageCostConfigPostRequest =
        util::read_json_file(file)?;
    let api = make_ccm_api(cfg);
    let resp = api
        .create_cost_gcp_usage_cost_config(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create GCP usage cost config: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn gcp_config_delete(cfg: &Config, id: i64) -> Result<()> {
    let api = make_ccm_api(cfg);
    api.delete_cost_gcp_usage_cost_config(id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete GCP usage cost config: {e:?}"))?;
    eprintln!("GCP usage cost config {id} deleted.");
    Ok(())
}

// ---- Cloud Cost Management — OCI Configs ----

pub async fn oci_configs_list(cfg: &Config) -> Result<()> {
    let api = make_ccm_api(cfg);
    let resp = api
        .list_cost_oci_configs()
        .await
        .map_err(|e| anyhow::anyhow!("failed to list OCI configs: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---- Cloud Cost Management — Anomalies ----

pub async fn anomalies_list(cfg: &Config) -> Result<()> {
    let api = make_ccm_api(cfg);
    let resp = api
        .list_cost_anomalies(ListCostAnomaliesOptionalParams::default())
        .await
        .map_err(|e| anyhow::anyhow!("failed to list cost anomalies: {e:?}"))?;
    formatter::output(cfg, &resp)
}

#[cfg(test)]
mod tests {

    use crate::test_support::*;

    #[tokio::test]
    async fn test_cost_projected() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::projected(&cfg).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_oci_configs_list() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "GET", r#"{"data":[]}"#).await;
        let result = super::oci_configs_list(&cfg).await;
        assert!(
            result.is_ok(),
            "oci_configs_list failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_oci_configs_list_error() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(403)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors":["Forbidden"]}"#)
            .create_async()
            .await;
        let result = super::oci_configs_list(&cfg).await;
        assert!(result.is_err(), "expected error for 403 response");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_anomalies_list() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "GET", r#"{"data":null}"#).await;
        let result = super::anomalies_list(&cfg).await;
        assert!(result.is_ok(), "anomalies_list failed: {:?}", result.err());
        cleanup_env();
    }
}
