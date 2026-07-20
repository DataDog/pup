use anyhow::Result;
use datadog_api_client::datadogV1::api_usage_metering::{
    GetHourlyUsageAttributionOptionalParams, GetUsageSummaryOptionalParams, UsageMeteringAPI,
};
use datadog_api_client::datadogV1::model::HourlyUsageAttributionUsageType;

use crate::config::Config;
use crate::formatter;
use crate::util_ext;

pub async fn summary(cfg: &Config, start: String, end: Option<String>) -> Result<()> {
    let api = crate::make_api!(UsageMeteringAPI, cfg);

    let start_dt = util_ext::parse_time_to_datetime(&start)?;

    let mut params = GetUsageSummaryOptionalParams::default();
    if let Some(e) = end {
        let end_dt = util_ext::parse_time_to_datetime(&e)?;
        params = params.end_month(end_dt);
    }

    let resp = api
        .get_usage_summary(start_dt, params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get usage summary: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn hourly(cfg: &Config, start: String, end: Option<String>) -> Result<()> {
    let api = crate::make_api!(UsageMeteringAPI, cfg);

    let start_dt = util_ext::parse_time_to_datetime(&start)?;

    let mut params = GetHourlyUsageAttributionOptionalParams::default();
    if let Some(e) = end {
        let end_dt = util_ext::parse_time_to_datetime(&e)?;
        params = params.end_hr(end_dt);
    }

    let resp = api
        .get_hourly_usage_attribution(start_dt, HourlyUsageAttributionUsageType::API_USAGE, params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get hourly usage: {e:?}"))?;
    formatter::output(cfg, &resp)
}

#[cfg(test)]
mod tests {

    use crate::test_support::*;

    #[tokio::test]
    async fn test_usage_summary() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"usage": []}"#).await;
        let _ = super::summary(&cfg, "2024-01".into(), None).await;
        cleanup_env();
    }
}
