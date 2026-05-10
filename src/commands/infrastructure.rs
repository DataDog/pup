use anyhow::Result;
use datadog_api_client::datadogV1::api_hosts::{HostsAPI, ListHostsOptionalParams};

use crate::config::Config;
use crate::formatter;

pub async fn hosts_list(
    cfg: &Config,
    filter: Option<String>,
    sort: String,
    count: i64,
    start: Option<i64>,
    include_muted_hosts_data: bool,
    include_hosts_metadata: bool,
) -> Result<()> {
    let api = crate::make_api!(HostsAPI, cfg);
    let mut params = ListHostsOptionalParams::default()
        .count(count)
        .sort_field(sort)
        .include_muted_hosts_data(include_muted_hosts_data)
        .include_hosts_metadata(include_hosts_metadata);
    if let Some(f) = filter {
        params = params.filter(f);
    }
    if let Some(s) = start {
        params = params.start(s);
    }
    let resp = api
        .list_hosts(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list hosts: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn hosts_get(cfg: &Config, hostname: &str) -> Result<()> {
    // The V1 HostsAPI does not have a direct get-host method.
    // Use list_hosts with a filter to find the specific host.
    let api = crate::make_api!(HostsAPI, cfg);
    let params = ListHostsOptionalParams::default()
        .filter(hostname.to_string())
        .count(1);
    let resp = api
        .list_hosts(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get host {hostname}: {e:?}"))?;
    formatter::output(cfg, &resp)
}

#[cfg(test)]
mod tests {

    use crate::test_support::*;

    #[tokio::test]
    async fn test_infrastructure_hosts_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"host_list": [], "total_returned": 0}"#).await;
        let _ = super::hosts_list(&cfg, None, "name".into(), 10, None, false, false).await;
        cleanup_env();
    }
}
