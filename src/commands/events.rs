use anyhow::Result;
use datadog_api_client::datadogV1::api_events::{
    EventsAPI as EventsV1API, ListEventsOptionalParams,
};
use datadog_api_client::datadogV2::api_events::{
    EventsAPI as EventsV2API, SearchEventsOptionalParams,
};
use datadog_api_client::datadogV2::model::{
    EventsListRequest, EventsQueryFilter, EventsRequestPage, EventsSort,
};

use crate::config::Config;
use crate::formatter;
use crate::util;

pub async fn list(cfg: &Config, start: i64, end: i64, tags: Option<String>) -> Result<()> {
    let api = crate::make_api!(EventsV1API, cfg);

    // Default to last hour if not specified
    let now = chrono::Utc::now().timestamp();
    let start = if start == 0 { now - 3600 } else { start };
    let end = if end == 0 { now } else { end };

    let mut params = ListEventsOptionalParams::default();
    if let Some(t) = tags {
        params = params.tags(t);
    }
    let resp = api
        .list_events(start, end, params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list events: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn search(
    cfg: &Config,
    query: String,
    from: String,
    to: String,
    limit: i32,
) -> Result<()> {
    let api = crate::make_api!(EventsV2API, cfg);

    let from_ms = util::parse_time_to_unix_millis(&from)?;
    let to_ms = util::parse_time_to_unix_millis(&to)?;

    let from_str = chrono::DateTime::from_timestamp_millis(from_ms)
        .unwrap()
        .to_rfc3339();
    let to_str = chrono::DateTime::from_timestamp_millis(to_ms)
        .unwrap()
        .to_rfc3339();

    let body = EventsListRequest::new()
        .filter(
            EventsQueryFilter::new()
                .query(query)
                .from(from_str)
                .to(to_str),
        )
        .page(EventsRequestPage::new().limit(limit))
        .sort(EventsSort::TIMESTAMP_DESCENDING);

    let params = SearchEventsOptionalParams::default().body(body);
    let resp = api
        .search_events(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to search events: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn get(cfg: &Config, id: i64) -> Result<()> {
    let api = crate::make_api!(EventsV1API, cfg);
    let resp = api
        .get_event(id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get event: {e:?}"))?;
    formatter::output(cfg, &resp)
}

#[cfg(test)]
mod tests {

    use crate::config::{Config, OutputFormat};
    use crate::test_support::*;

    #[tokio::test]
    async fn test_events_list() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "GET", r#"{"events": []}"#).await;

        let now = chrono::Utc::now().timestamp();
        let result = super::list(&cfg, now - 3600, now, None).await;
        assert!(result.is_ok(), "events list failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_events_get() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(
            &mut server,
            "GET",
            r#"{"event": {"id": 12345, "title": "Test Event", "text": "Something happened"}}"#,
        )
        .await;

        let result = super::get(&cfg, 12345).await;
        assert!(result.is_ok(), "events get failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_events_search() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "POST", r#"{"data": [], "meta": {"page": {}}}"#).await;

        let result =
            super::search(&cfg, "source:nginx".into(), "1h".into(), "now".into(), 10).await;
        assert!(result.is_ok(), "events search failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_events_search_requires_api_keys() {
        let _lock = lock_env().await;
        let server = mockito::Server::new_async().await;
        std::env::set_var("PUP_MOCK_SERVER", server.url());

        let cfg = Config {
            api_key: None,
            app_key: None,
            access_token: Some("token".into()),
            pat: None,
            pat_kind: None,
            site: "datadoghq.com".into(),
            site_explicit: false,
            org: None,
            output_format: OutputFormat::Json,
            auto_approve: false,
            agent_mode: false,
            read_only: false,
        };

        let result =
            super::search(&cfg, "source:nginx".into(), "1h".into(), "now".into(), 10).await;
        assert!(result.is_err(), "events search should require API keys");
        cleanup_env();
    }
}
