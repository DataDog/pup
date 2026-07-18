use std::io::Read;

use anyhow::{Context, Result};
use datadog_api_client::datadogV1::api_events::{
    EventsAPI as EventsV1API, ListEventsOptionalParams,
};
use datadog_api_client::datadogV1::model::{EventAlertType, EventCreateRequest, EventPriority};
use datadog_api_client::datadogV2::api_events::{
    EventsAPI as EventsV2API, SearchEventsOptionalParams,
};
use datadog_api_client::datadogV2::model::{
    EventsListRequest, EventsQueryFilter, EventsRequestPage, EventsSort,
};

use crate::config::Config;
use crate::formatter;
use crate::util;

#[derive(Clone, Debug, clap::ValueEnum)]
pub(crate) enum EventAlertTypeArg {
    Error,
    Warning,
    Info,
    Success,
}

impl From<EventAlertTypeArg> for EventAlertType {
    fn from(value: EventAlertTypeArg) -> Self {
        match value {
            EventAlertTypeArg::Error => Self::ERROR,
            EventAlertTypeArg::Warning => Self::WARNING,
            EventAlertTypeArg::Info => Self::INFO,
            EventAlertTypeArg::Success => Self::SUCCESS,
        }
    }
}

#[derive(Clone, Debug, clap::ValueEnum)]
pub(crate) enum EventPriorityArg {
    Normal,
    Low,
}

impl From<EventPriorityArg> for EventPriority {
    fn from(value: EventPriorityArg) -> Self {
        match value {
            EventPriorityArg::Normal => Self::NORMAL,
            EventPriorityArg::Low => Self::LOW,
        }
    }
}

pub(crate) struct PostOptions {
    pub title: String,
    pub message: Option<String>,
    pub date_happened: Option<i64>,
    pub handle: Option<String>,
    pub priority: EventPriorityArg,
    pub related_event_id: Option<i64>,
    pub tags: Option<String>,
    pub host: Option<String>,
    pub device: Option<String>,
    pub event_type: Option<String>,
    pub aggregation_key: Option<String>,
    pub alert_type: Option<EventAlertTypeArg>,
}

pub async fn post(cfg: &Config, mut options: PostOptions) -> Result<()> {
    let api = crate::make_api!(EventsV1API, cfg);
    let message = resolve_message(options.message.take(), std::io::stdin().lock())?;
    let body = build_post_request(options, message);
    let resp = api
        .create_event(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to post event: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub(crate) fn resolve_host(host: String, no_host: bool) -> Result<Option<String>> {
    resolve_host_with(host, no_host, || {
        let output = std::process::Command::new("hostname")
            .output()
            .context("failed to determine local hostname")?;
        if !output.status.success() {
            anyhow::bail!("failed to determine local hostname");
        }

        let hostname =
            String::from_utf8(output.stdout).context("local hostname is not valid UTF-8")?;
        let hostname = hostname.trim();
        if hostname.is_empty() {
            anyhow::bail!("local hostname is empty");
        }
        Ok(hostname.to_owned())
    })
}

fn resolve_host_with(
    host: String,
    no_host: bool,
    local_hostname: impl FnOnce() -> Result<String>,
) -> Result<Option<String>> {
    if no_host {
        Ok(None)
    } else if host.is_empty() {
        local_hostname().map(Some)
    } else {
        Ok(Some(host))
    }
}

fn resolve_message(message: Option<String>, mut reader: impl Read) -> Result<String> {
    if let Some(message) = message {
        return Ok(message);
    }

    let mut message = String::new();
    reader
        .read_to_string(&mut message)
        .context("failed to read event message from stdin")?;
    Ok(message)
}

fn build_post_request(options: PostOptions, message: String) -> EventCreateRequest {
    let mut body =
        EventCreateRequest::new(message, options.title).priority(Some(options.priority.into()));

    if let Some(tags) = options.tags {
        body = body.tags(tags.split(',').map(str::trim).map(str::to_owned).collect());
    }
    if let Some(host) = options.host {
        body = body.host(host);
    }
    if let Some(date_happened) = options.date_happened {
        body = body.date_happened(date_happened);
    }
    if let Some(handle) = options.handle {
        body.additional_properties
            .insert("handle".into(), serde_json::Value::String(handle));
    }
    if let Some(related_event_id) = options.related_event_id {
        body = body.related_event_id(related_event_id);
    }
    if let Some(device) = options.device {
        body = body.device_name(device);
    }
    if let Some(event_type) = options.event_type {
        body = body.source_type_name(event_type);
    }
    if let Some(aggregation_key) = options.aggregation_key {
        body = body.aggregation_key(aggregation_key);
    }
    if let Some(alert_type) = options.alert_type {
        body = body.alert_type(alert_type.into());
    }

    body
}

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
    use clap::Parser;

    fn post_options() -> super::PostOptions {
        super::PostOptions {
            title: "Test title".into(),
            message: Some("Test message".into()),
            date_happened: Some(1_700_000_000),
            handle: Some("test-user".into()),
            priority: super::EventPriorityArg::Low,
            related_event_id: Some(42),
            tags: Some("test:first, test:second".into()),
            host: None,
            device: Some("test-device".into()),
            event_type: Some("test-source".into()),
            aggregation_key: Some("test-group".into()),
            alert_type: Some(super::EventAlertTypeArg::Warning),
        }
    }

    #[tokio::test]
    async fn test_events_post_sends_dogshell_fields_without_host() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = server
            .mock("POST", "/api/v1/events")
            .match_body(mockito::Matcher::Json(serde_json::json!({
                "aggregation_key": "test-group",
                "alert_type": "warning",
                "date_happened": 1_700_000_000,
                "device_name": "test-device",
                "handle": "test-user",
                "priority": "low",
                "related_event_id": 42,
                "source_type_name": "test-source",
                "tags": ["test:first", "test:second"],
                "text": "Test message",
                "title": "Test title"
            })))
            .with_status(202)
            .with_header("content-type", "application/json")
            .with_body(r#"{"status":"ok","event":{"id":12345}}"#)
            .create_async()
            .await;

        let result = super::post(&cfg, post_options()).await;
        assert!(result.is_ok(), "events post failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_events_post_reports_api_error() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = server
            .mock("POST", "/api/v1/events")
            .with_status(400)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors":["Bad Request"]}"#)
            .create_async()
            .await;

        let result = super::post(&cfg, post_options()).await;
        assert!(result.is_err(), "events post should report API errors");
        cleanup_env();
    }

    #[test]
    fn test_events_post_accepts_dogshell_flag_names() {
        let result = crate::Cli::try_parse_from([
            "pup",
            "events",
            "post",
            "--date_happened",
            "1700000000",
            "--handle",
            "test-user",
            "--priority",
            "low",
            "--related_event_id",
            "42",
            "--tags",
            "test:first,test:second",
            "--host",
            "ignored-host",
            "--no_host",
            "--device",
            "test-device",
            "--type",
            "test-source",
            "--aggregation_key",
            "test-group",
            "--alert_type",
            "warning",
            "Test title",
        ]);

        assert!(result.is_ok(), "dogshell flags should parse");
    }

    #[test]
    fn test_events_post_rejects_invalid_alert_type() {
        let result = crate::Cli::try_parse_from([
            "pup",
            "events",
            "post",
            "--alert_type",
            "critical",
            "Test title",
            "Test message",
        ]);

        assert!(result.is_err(), "invalid alert type should be rejected");
    }

    #[test]
    fn test_events_post_rejects_invalid_priority() {
        let result = crate::Cli::try_parse_from([
            "pup",
            "events",
            "post",
            "--priority",
            "urgent",
            "Test title",
            "Test message",
        ]);

        assert!(result.is_err(), "invalid priority should be rejected");
    }

    #[test]
    fn test_events_post_reads_message_from_stdin() {
        let message =
            super::resolve_message(None, std::io::Cursor::new("Message from stdin")).unwrap();
        assert_eq!(message, "Message from stdin");
    }

    #[test]
    fn test_events_post_no_host_overrides_host() {
        assert_eq!(
            super::resolve_host_with("test-host".into(), true, || {
                panic!("hostname lookup must not run")
            })
            .unwrap(),
            None
        );
        assert_eq!(
            super::resolve_host_with("test-host".into(), false, || {
                panic!("hostname lookup must not run")
            })
            .unwrap(),
            Some("test-host".into())
        );
    }

    #[test]
    fn test_events_post_defaults_to_local_host() {
        assert_eq!(
            super::resolve_host_with(String::new(), false, || Ok("local-host".into())).unwrap(),
            Some("local-host".into())
        );
    }

    #[test]
    fn test_events_post_reports_local_host_error() {
        let result = super::resolve_host_with(String::new(), false, || {
            anyhow::bail!("hostname unavailable")
        });
        assert!(result.is_err(), "hostname errors should be reported");
    }

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
            site: "datadoghq.com".into(),
            site_explicit: false,
            org: None,
            output_format: OutputFormat::Json,
            auto_approve: false,
            agent_mode: false,
            read_only: false,
            jq: None,
        };

        let result =
            super::search(&cfg, "source:nginx".into(), "1h".into(), "now".into(), 10).await;
        assert!(result.is_err(), "events search should require API keys");
        cleanup_env();
    }
}
