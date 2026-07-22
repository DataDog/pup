use std::io::{IsTerminal, Read};

#[cfg(not(target_arch = "wasm32"))]
use anyhow::Context;
use anyhow::Result;
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
use crate::util_ext;

const MAX_AGGREGATION_KEY_CHARS: usize = 100;
const MAX_EVENT_TEXT_CHARS: usize = 4000;
const MAX_EVENT_AGE_SECONDS: i64 = 18 * 60 * 60;

#[derive(Clone, Debug, clap::ValueEnum)]
pub(crate) enum EventAlertTypeArg {
    Error,
    Warning,
    Info,
    Success,
    #[value(name = "user_update")]
    UserUpdate,
    Recommendation,
    Snapshot,
}

impl From<EventAlertTypeArg> for EventAlertType {
    fn from(value: EventAlertTypeArg) -> Self {
        match value {
            EventAlertTypeArg::Error => Self::ERROR,
            EventAlertTypeArg::Warning => Self::WARNING,
            EventAlertTypeArg::Info => Self::INFO,
            EventAlertTypeArg::Success => Self::SUCCESS,
            EventAlertTypeArg::UserUpdate => Self::USER_UPDATE,
            EventAlertTypeArg::Recommendation => Self::RECOMMENDATION,
            EventAlertTypeArg::Snapshot => Self::SNAPSHOT,
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
    // The V1 events intake endpoint (POST /api/v1/events) authenticates with the
    // API key alone and does not accept OAuth2 bearer tokens.
    cfg.validate_api_key_only()?;
    if options.title.trim().is_empty() {
        anyhow::bail!("event title is empty");
    }
    let api = crate::make_api_no_auth!(EventsV1API, cfg);
    let message = resolve_message(
        options.message.take(),
        std::io::stdin().is_terminal(),
        std::io::stdin().lock(),
    )?;
    validate_post_request(&options, &message, chrono::Utc::now().timestamp())?;
    let body = build_post_request(options, message);
    let resp = api
        .create_event(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to post event: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub(crate) fn resolve_host(host: String, no_host: bool) -> Option<String> {
    resolve_host_with(host, no_host, local_hostname)
}

// `gethostname` is a native-only dependency (the `events` module still compiles
// for wasm32), so keep the wasm path a graceful failure that falls back to no host.
#[cfg(not(target_arch = "wasm32"))]
fn local_hostname() -> Result<String> {
    let hostname = gethostname::gethostname();
    let hostname = hostname
        .to_str()
        .context("local hostname is not valid UTF-8")?
        .trim();
    if hostname.is_empty() {
        anyhow::bail!("local hostname is empty");
    }
    Ok(hostname.to_owned())
}

#[cfg(target_arch = "wasm32")]
fn local_hostname() -> Result<String> {
    anyhow::bail!("local hostname lookup is not supported on this platform")
}

fn resolve_host_with(
    host: String,
    no_host: bool,
    local_hostname: impl FnOnce() -> Result<String>,
) -> Option<String> {
    if no_host {
        None
    } else if host.is_empty() {
        // Default to the local hostname, but a lookup failure (e.g. a non-UTF-8
        // hostname) must not abort the whole command — post without a host instead.
        match local_hostname() {
            Ok(hostname) => Some(hostname),
            Err(e) => {
                eprintln!(
                    "warning: could not determine local hostname ({e}); posting event \
                     without a host (use --host to set one or --no_host to silence this)"
                );
                None
            }
        }
    } else {
        Some(host)
    }
}

fn resolve_message(
    message: Option<String>,
    stdin_is_tty: bool,
    reader: impl Read,
) -> Result<String> {
    let message = match message {
        Some(message) => message,
        None => {
            // Only read stdin when it is piped; reading an interactive terminal
            // would block forever waiting for EOF.
            if stdin_is_tty {
                anyhow::bail!(
                    "no event message provided: pass it as an argument or pipe it via stdin"
                );
            }
            let buf = util_ext::read_to_string(reader, "failed to read event message from stdin")?;
            // Match argument input by dropping trailing whitespace added by shell pipelines.
            buf.trim_end().to_owned()
        }
    };

    if message.trim().is_empty() {
        anyhow::bail!("event message is empty");
    }
    Ok(message)
}

fn validate_post_request(options: &PostOptions, message: &str, now: i64) -> Result<()> {
    if let Some(aggregation_key) = &options.aggregation_key {
        if aggregation_key.chars().count() > MAX_AGGREGATION_KEY_CHARS {
            anyhow::bail!(
                "event aggregation key must be at most {MAX_AGGREGATION_KEY_CHARS} characters"
            );
        }
    }
    if message.chars().count() > MAX_EVENT_TEXT_CHARS {
        anyhow::bail!("event text must be at most {MAX_EVENT_TEXT_CHARS} characters");
    }
    if let Some(date_happened) = options.date_happened {
        if date_happened < now - MAX_EVENT_AGE_SECONDS {
            anyhow::bail!("event date_happened cannot be more than 18 hours in the past");
        }
    }
    Ok(())
}

fn build_post_request(options: PostOptions, message: String) -> EventCreateRequest {
    let mut body =
        EventCreateRequest::new(message, options.title).priority(Some(options.priority.into()));

    if let Some(tags) = options.tags {
        body = body.tags(
            tags.split(',')
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(str::to_owned)
                .collect(),
        );
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

    let from_ms = util_ext::parse_time_to_unix_millis(&from)?;
    let to_ms = util_ext::parse_time_to_unix_millis(&to)?;

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
            date_happened: None,
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

    fn auth_config(
        api_key: Option<&str>,
        app_key: Option<&str>,
        access_token: Option<&str>,
    ) -> Config {
        Config {
            api_key: api_key.map(String::from),
            app_key: app_key.map(String::from),
            access_token: access_token.map(String::from),
            site: "datadoghq.com".into(),
            site_explicit: false,
            org: None,
            output_format: OutputFormat::Json,
            auto_approve: false,
            agent_mode: false,
            read_only: false,
            jq: None,
        }
    }

    #[tokio::test]
    async fn test_events_post_sends_only_api_key_and_post_fields() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        std::env::set_var("PUP_MOCK_SERVER", server.url());
        let cfg = auth_config(Some("test-api-key"), None, None);
        let date_happened = chrono::Utc::now().timestamp();
        let _mock = server
            .mock("POST", "/api/v1/events")
            .match_header("DD-API-KEY", "test-api-key")
            .match_header("DD-APPLICATION-KEY", mockito::Matcher::Missing)
            .match_header("Authorization", mockito::Matcher::Missing)
            .match_body(mockito::Matcher::Json(serde_json::json!({
                "aggregation_key": "test-group",
                "alert_type": "warning",
                "date_happened": date_happened,
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

        let mut options = post_options();
        options.date_happened = Some(date_happened);
        let result = super::post(&cfg, options).await;
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

    #[tokio::test]
    async fn test_events_post_includes_host_in_payload() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = server
            .mock("POST", "/api/v1/events")
            .match_body(mockito::Matcher::PartialJson(serde_json::json!({
                "host": "resolved-host",
                "text": "Test message",
                "title": "Test title"
            })))
            .with_status(202)
            .with_header("content-type", "application/json")
            .with_body(r#"{"status":"ok","event":{"id":12345}}"#)
            .create_async()
            .await;

        let mut options = post_options();
        options.host = Some("resolved-host".into());
        let result = super::post(&cfg, options).await;
        assert!(
            result.is_ok(),
            "events post with host failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_events_post_rejects_bearer_only() {
        // The V1 events intake endpoint rejects OAuth2 bearer tokens, so posting
        // must fail fast when only an access token is configured.
        let cfg = auth_config(None, None, Some("token"));

        let result = super::post(&cfg, post_options()).await;
        let err = result.expect_err("events post should reject bearer-only auth");
        assert!(err.to_string().contains("DD_API_KEY"));
    }

    #[tokio::test]
    async fn test_events_post_rejects_missing_api_key() {
        let cfg = auth_config(None, Some("test-app-key"), None);

        let result = super::post(&cfg, post_options()).await;
        let err = result.expect_err("events post should require an API key");
        assert!(err.to_string().contains("DD_API_KEY"));
    }

    #[tokio::test]
    async fn test_events_post_rejects_empty_title() {
        let cfg = auth_config(Some("test-api-key"), None, None);
        let mut options = post_options();
        options.title = "   ".into();

        let result = super::post(&cfg, options).await;
        assert!(result.is_err(), "empty title should be rejected");
    }

    #[test]
    fn test_events_post_accepts_legacy_flag_aliases() {
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

        assert!(result.is_ok(), "legacy flag aliases should parse");
    }

    #[test]
    fn test_events_post_accepts_v1_field_flag_names() {
        for (device_flag, source_type_flag) in [
            ("--device-name", "--source-type-name"),
            ("--device_name", "--source_type_name"),
        ] {
            let result = crate::Cli::try_parse_from([
                "pup",
                "events",
                "post",
                device_flag,
                "test-device",
                source_type_flag,
                "test-source",
                "Test title",
                "Test message",
            ]);

            assert!(
                result.is_ok(),
                "V1 field flags {device_flag} and {source_type_flag} should parse"
            );
        }
    }

    #[test]
    fn test_events_post_accepts_openapi_alert_types() {
        for alert_type in [
            "error",
            "warning",
            "info",
            "success",
            "user_update",
            "recommendation",
            "snapshot",
        ] {
            let result = crate::Cli::try_parse_from([
                "pup",
                "events",
                "post",
                "--alert_type",
                alert_type,
                "Test title",
                "Test message",
            ]);

            assert!(
                result.is_ok(),
                "alert type {alert_type:?} should be accepted"
            );
        }
    }

    #[test]
    fn test_events_post_rejects_non_openapi_alert_type() {
        let result = crate::Cli::try_parse_from([
            "pup",
            "events",
            "post",
            "--alert_type",
            "custom_alert",
            "Test title",
            "Test message",
        ]);

        assert!(result.is_err(), "non-OpenAPI alert type should be rejected");
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
            super::resolve_message(None, false, std::io::Cursor::new("Message from stdin"))
                .unwrap();
        assert_eq!(message, "Message from stdin");
    }

    #[test]
    fn test_events_post_errors_on_tty_without_message() {
        // An interactive terminal with no message must error instead of blocking.
        let result = super::resolve_message(None, true, std::io::empty());
        assert!(result.is_err(), "TTY stdin with no message should error");
    }

    #[test]
    fn test_events_post_rejects_empty_message() {
        let from_stdin = super::resolve_message(None, false, std::io::Cursor::new("   \n"));
        assert!(
            from_stdin.is_err(),
            "blank stdin message should be rejected"
        );
        let from_arg = super::resolve_message(Some(String::new()), false, std::io::empty());
        assert!(
            from_arg.is_err(),
            "empty message argument should be rejected"
        );
    }

    #[test]
    fn test_events_post_trims_trailing_newline_from_stdin() {
        let message =
            super::resolve_message(None, false, std::io::Cursor::new("Message from stdin\n"))
                .unwrap();
        assert_eq!(message, "Message from stdin");
    }

    #[test]
    fn test_events_post_filters_empty_tags() {
        let mut options = post_options();
        options.tags = Some("a,, b ,".into());
        let body = super::build_post_request(options, "msg".into());
        assert_eq!(body.tags, Some(vec!["a".to_string(), "b".to_string()]));
    }

    #[test]
    fn test_events_post_serializes_openapi_alert_types() {
        for (alert_type, expected) in [
            (super::EventAlertTypeArg::Error, "error"),
            (super::EventAlertTypeArg::Warning, "warning"),
            (super::EventAlertTypeArg::Info, "info"),
            (super::EventAlertTypeArg::Success, "success"),
            (super::EventAlertTypeArg::UserUpdate, "user_update"),
            (super::EventAlertTypeArg::Recommendation, "recommendation"),
            (super::EventAlertTypeArg::Snapshot, "snapshot"),
        ] {
            let mut options = post_options();
            options.alert_type = Some(alert_type);
            let body = super::build_post_request(options, "msg".into());
            let value = serde_json::to_value(body).unwrap();
            assert_eq!(value["alert_type"], expected);
        }
    }

    #[test]
    fn test_events_post_accepts_openapi_request_limits() {
        let now = chrono::Utc::now().timestamp();
        let mut options = post_options();
        options.aggregation_key = Some("a".repeat(super::MAX_AGGREGATION_KEY_CHARS));
        options.date_happened = Some(now - super::MAX_EVENT_AGE_SECONDS);
        let message = "m".repeat(super::MAX_EVENT_TEXT_CHARS);

        assert!(super::validate_post_request(&options, &message, now).is_ok());
    }

    #[test]
    fn test_events_post_rejects_aggregation_key_over_openapi_limit() {
        let mut options = post_options();
        options.aggregation_key = Some("a".repeat(super::MAX_AGGREGATION_KEY_CHARS + 1));

        let err = super::validate_post_request(&options, "message", 0).unwrap_err();
        assert!(err.to_string().contains("aggregation key"));
    }

    #[test]
    fn test_events_post_rejects_text_over_openapi_limit() {
        let options = post_options();
        let message = "m".repeat(super::MAX_EVENT_TEXT_CHARS + 1);

        let err = super::validate_post_request(&options, &message, 0).unwrap_err();
        assert!(err.to_string().contains("event text"));
    }

    #[test]
    fn test_events_post_rejects_date_older_than_openapi_limit() {
        let now = chrono::Utc::now().timestamp();
        let mut options = post_options();
        options.date_happened = Some(now - super::MAX_EVENT_AGE_SECONDS - 1);

        let err = super::validate_post_request(&options, "message", now).unwrap_err();
        assert!(err.to_string().contains("18 hours"));
    }

    #[test]
    fn test_events_post_no_host_overrides_host() {
        assert_eq!(
            super::resolve_host_with("test-host".into(), true, || {
                panic!("hostname lookup must not run")
            }),
            None
        );
        assert_eq!(
            super::resolve_host_with("test-host".into(), false, || {
                panic!("hostname lookup must not run")
            }),
            Some("test-host".into())
        );
    }

    #[test]
    fn test_events_post_defaults_to_local_host() {
        assert_eq!(
            super::resolve_host_with(String::new(), false, || Ok("local-host".into())),
            Some("local-host".into())
        );
    }

    #[test]
    fn test_events_post_falls_back_to_no_host_when_lookup_fails() {
        // A hostname lookup failure must not abort the post; fall back to no host.
        assert_eq!(
            super::resolve_host_with(String::new(), false, || {
                anyhow::bail!("hostname unavailable")
            }),
            None
        );
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
