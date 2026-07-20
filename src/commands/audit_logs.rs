use anyhow::Result;
use datadog_api_client::datadogV2::api_audit::{
    AuditAPI, ListAuditLogsOptionalParams, SearchAuditLogsOptionalParams,
};
use datadog_api_client::datadogV2::model::{
    AuditLogsQueryFilter, AuditLogsQueryPageOptions, AuditLogsSearchEventsRequest, AuditLogsSort,
};

use crate::config::Config;
use crate::formatter;
use crate::util_ext;

pub async fn list(cfg: &Config, from: String, to: String, limit: i32) -> Result<()> {
    let api = crate::make_api!(AuditAPI, cfg);

    let from_dt = util_ext::parse_time_to_datetime(&from)?;
    let to_dt = util_ext::parse_time_to_datetime(&to)?;

    let params = ListAuditLogsOptionalParams::default()
        .filter_from(from_dt)
        .filter_to(to_dt)
        .page_limit(limit);

    let resp = api
        .list_audit_logs(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list audit logs: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn search(
    cfg: &Config,
    query: String,
    from: String,
    to: String,
    limit: i32,
) -> Result<()> {
    let api = crate::make_api!(AuditAPI, cfg);

    let from_ms = util_ext::parse_time_to_unix_millis(&from)?;
    let to_ms = util_ext::parse_time_to_unix_millis(&to)?;

    let from_str = chrono::DateTime::from_timestamp_millis(from_ms)
        .unwrap()
        .to_rfc3339();
    let to_str = chrono::DateTime::from_timestamp_millis(to_ms)
        .unwrap()
        .to_rfc3339();

    let body = AuditLogsSearchEventsRequest::new()
        .filter(
            AuditLogsQueryFilter::new()
                .query(query)
                .from(from_str)
                .to(to_str),
        )
        .page(AuditLogsQueryPageOptions::new().limit(limit))
        .sort(AuditLogsSort::TIMESTAMP_DESCENDING);

    let params = SearchAuditLogsOptionalParams::default().body(body);
    let resp = api
        .search_audit_logs(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to search audit logs: {e:?}"))?;
    formatter::output(cfg, &resp)
}

#[cfg(test)]
mod tests {

    use crate::test_support::*;

    #[tokio::test]
    async fn test_audit_logs_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::list(&cfg, "1h".into(), "now".into(), 10).await;
        cleanup_env();
    }
}
