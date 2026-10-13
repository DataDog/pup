use anyhow::Result;
use datadog_api_client::datadogV2::api_downtimes::DowntimesAPI;
#[cfg(not(generated_op = "downtime.get"))]
use datadog_api_client::datadogV2::api_downtimes::GetDowntimeOptionalParams;
#[cfg(not(generated_op = "downtime.list"))]
use datadog_api_client::datadogV2::api_downtimes::ListDowntimesOptionalParams;

use crate::config::Config;
use crate::formatter;

#[cfg(not(generated_op = "downtime.list"))]
pub async fn list(cfg: &Config) -> Result<()> {
    let api = crate::make_api!(DowntimesAPI, cfg);
    let resp = api
        .list_downtimes(ListDowntimesOptionalParams::default())
        .await
        .map_err(|e| anyhow::anyhow!("failed to list downtimes: {e:?}"))?;
    formatter::output(cfg, &resp)
}

#[cfg(not(generated_op = "downtime.get"))]
pub async fn get(cfg: &Config, id: &str) -> Result<()> {
    let api = crate::make_api!(DowntimesAPI, cfg);
    let resp = api
        .get_downtime(id.to_string(), GetDowntimeOptionalParams::default())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get downtime: {e:?}"))?;
    formatter::output(cfg, &resp)
}

#[cfg(not(generated_op = "downtime.create"))]
pub async fn create(cfg: &Config, file: &str) -> Result<()> {
    let body: datadog_api_client::datadogV2::model::DowntimeCreateRequest =
        crate::util::read_json_file(file)?;
    let api = crate::make_api!(DowntimesAPI, cfg);
    let resp = api
        .create_downtime(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create downtime: {e:?}"))?;
    formatter::output(cfg, &resp)
}

#[cfg(not(generated_op = "downtime.cancel"))]
pub async fn cancel(cfg: &Config, id: &str) -> Result<()> {
    let api = crate::make_api!(DowntimesAPI, cfg);
    api.cancel_downtime(id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to cancel downtime: {e:?}"))?;
    println!("Downtime {id} cancelled.");
    Ok(())
}

#[cfg(test)]
mod tests {

    use crate::test_support::*;

    #[cfg(not(generated_op = "downtime.list"))]
    #[tokio::test]
    async fn test_downtime_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::list(&cfg).await;
        cleanup_env();
    }

    #[cfg(not(generated_op = "downtime.get"))]
    #[tokio::test]
    async fn test_downtime_get() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": {}}"#).await;
        let _ = super::get(&cfg, "d1").await;
        cleanup_env();
    }
}
