use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use rand::RngExt;
use serde_json::Value;

use crate::config::Config;
use crate::raw_client;

const MAX_RETRY_DELAY: Duration = Duration::from_secs(30);
const INITIAL_PROGRESS_DELAY: Duration = Duration::from_secs(10);
const SECOND_PROGRESS_DELAY: Duration = Duration::from_secs(20);
const INITIAL_PROGRESS_MESSAGE: &str = "DDSQL query is still running after 10s";
const SECOND_PROGRESS_MESSAGE: &str = "DDSQL query is still running after 30s";

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
trait QueryTransport: Sync {
    async fn post(&self, path: &str, body: Value, user_agent: &str) -> Result<Value>;
}

struct RawQueryTransport<'a> {
    config: &'a Config,
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl QueryTransport for RawQueryTransport<'_> {
    async fn post(&self, path: &str, body: Value, user_agent: &str) -> Result<Value> {
        raw_client::raw_post_with_ua(self.config, path, body, user_agent.to_string()).await
    }
}

#[async_trait]
trait Sleeper: Sync {
    async fn sleep(&self, duration: Duration);
}

struct TokioSleeper;

#[async_trait]
impl Sleeper for TokioSleeper {
    async fn sleep(&self, duration: Duration) {
        tokio::time::sleep(duration).await;
    }
}

trait JitterSource: Sync {
    fn sample_millis(&self) -> u64;
}

struct RandomJitter;

impl JitterSource for RandomJitter {
    fn sample_millis(&self) -> u64 {
        rand::rng().random_range(0..1000)
    }
}

trait ProgressOutput: Send + Sync + 'static {
    fn write_stderr(&self, message: &str);
}

struct StderrProgressOutput;

impl ProgressOutput for StderrProgressOutput {
    fn write_stderr(&self, message: &str) {
        eprintln!("{message}");
    }
}

struct ProgressReporter {
    handle: tokio::task::JoinHandle<()>,
}

impl ProgressReporter {
    fn start(output: Arc<dyn ProgressOutput>) -> Self {
        let handle = tokio::spawn(async move {
            tokio::time::sleep(INITIAL_PROGRESS_DELAY).await;
            output.write_stderr(INITIAL_PROGRESS_MESSAGE);
            tokio::time::sleep(SECOND_PROGRESS_DELAY).await;
            output.write_stderr(SECOND_PROGRESS_MESSAGE);
        });
        Self { handle }
    }
}

impl Drop for ProgressReporter {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

struct AdvancedQueryClient<'a> {
    transport: &'a dyn QueryTransport,
    sleeper: &'a dyn Sleeper,
    jitter: &'a dyn JitterSource,
    progress: Arc<dyn ProgressOutput>,
}

impl AdvancedQueryClient<'_> {
    #[allow(clippy::too_many_arguments)]
    async fn execute<F>(
        &self,
        initial_path: &str,
        fetch_path: &str,
        initial_body: Value,
        user_agent: &str,
        extract_status: fn(&Value) -> Result<Option<String>>,
        build_fetch: F,
    ) -> Result<Value>
    where
        F: Fn(&str) -> Value,
    {
        let _progress = ProgressReporter::start(Arc::clone(&self.progress));
        let response = self
            .transport
            .post(initial_path, initial_body, user_agent)
            .await?;
        let mut query_id = match extract_status(&response)? {
            None => return Ok(response),
            Some(query_id) => query_id,
        };

        loop {
            let response = self
                .fetch_with_retries(fetch_path, build_fetch(&query_id), user_agent, &query_id)
                .await?;
            query_id =
                match extract_status(&response).map_err(|error| fetch_error(error, &query_id))? {
                    None => return Ok(response),
                    Some(query_id) => query_id,
                };
        }
    }

    async fn fetch_with_retries(
        &self,
        path: &str,
        body: Value,
        user_agent: &str,
        query_id: &str,
    ) -> Result<Value> {
        let mut attempted_retries = 0;
        let mut exponential_base = None;

        loop {
            match self.transport.post(path, body.clone(), user_agent).await {
                Ok(response) => return Ok(response),
                Err(error) => {
                    let Some(status) = http_status(&error) else {
                        return Err(fetch_error(error, query_id));
                    };
                    let Some(max_retries) = max_retry_count(status) else {
                        return Err(fetch_error(error, query_id));
                    };
                    if attempted_retries >= max_retries {
                        return Err(fetch_error(error, query_id));
                    }

                    let base = *exponential_base.get_or_insert_with(|| initial_retry_base(status));
                    let header_seconds = if status == 429 {
                        rate_limit_reset_seconds(&error)
                    } else {
                        None
                    };
                    if header_seconds.is_some_and(|seconds| seconds > MAX_RETRY_DELAY.as_secs_f64())
                    {
                        return Err(fetch_error_with_detail(
                            error,
                            query_id,
                            "rate-limit reset delay exceeds the 30s retry cap",
                        ));
                    }

                    let header_delay = header_seconds.map(Duration::from_secs_f64);
                    let jitter = Duration::from_millis(self.jitter.sample_millis().min(999));
                    let delay = retry_delay(base, header_delay, jitter);
                    self.sleeper.sleep(delay).await;
                    attempted_retries += 1;
                    exponential_base = Some(base.saturating_mul(2).min(MAX_RETRY_DELAY));
                }
            }
        }
    }
}

fn http_status(error: &anyhow::Error) -> Option<u16> {
    error
        .downcast_ref::<raw_client::HttpError>()
        .map(|error| error.status)
}

fn max_retry_count(status: u16) -> Option<usize> {
    match status {
        429 => Some(5),
        500 | 502 | 503 | 504 => Some(2),
        _ => None,
    }
}

fn initial_retry_base(status: u16) -> Duration {
    match status {
        429 => Duration::from_secs(2),
        500 | 502 | 503 | 504 => Duration::from_secs(1),
        _ => unreachable!("initial retry base requires a retryable status"),
    }
}

fn retry_delay(base: Duration, header: Option<Duration>, jitter: Duration) -> Duration {
    header
        .unwrap_or(base)
        .saturating_add(jitter)
        .min(MAX_RETRY_DELAY)
}

fn rate_limit_reset_seconds(error: &anyhow::Error) -> Option<f64> {
    let info = error
        .downcast_ref::<raw_client::HttpError>()?
        .rate_limit
        .as_ref()?;
    paired_reset_seconds(info.remaining.as_deref()?, info.reset.as_deref()?)
}

fn paired_reset_seconds(remaining: &str, reset: &str) -> Option<f64> {
    remaining
        .split(',')
        .zip(reset.split(','))
        .filter_map(|(remaining, reset)| {
            let remaining = remaining.trim().parse::<f64>().ok()?;
            let reset = reset.trim().parse::<f64>().ok()?;
            if !remaining.is_finite() || !reset.is_finite() || remaining > 0.0 || reset < 0.0 {
                return None;
            }
            Some(reset)
        })
        .reduce(f64::max)
}

fn fetch_error(error: anyhow::Error, query_id: &str) -> anyhow::Error {
    let message = format!("query fetch failed (query_id: {query_id}): {error}");
    error.context(message)
}

fn fetch_error_with_detail(error: anyhow::Error, query_id: &str, detail: &str) -> anyhow::Error {
    let message = format!("query fetch failed (query_id: {query_id}): {detail}: {error}");
    error.context(message)
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn execute<F>(
    config: &Config,
    initial_path: &str,
    fetch_path: &str,
    initial_body: Value,
    user_agent: &str,
    extract_status: fn(&Value) -> Result<Option<String>>,
    build_fetch: F,
) -> Result<Value>
where
    F: Fn(&str) -> Value,
{
    let transport = RawQueryTransport { config };
    let sleeper = TokioSleeper;
    let jitter = RandomJitter;
    let client = AdvancedQueryClient {
        transport: &transport,
        sleeper: &sleeper,
        jitter: &jitter,
        progress: Arc::new(StderrProgressOutput),
    };
    client
        .execute(
            initial_path,
            fetch_path,
            initial_body,
            user_agent,
            extract_status,
            build_fetch,
        )
        .await
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    use anyhow::anyhow;
    use serde_json::json;
    use tokio::sync::Notify;

    use super::*;
    use crate::rate_limit::RateLimitInfo;

    const INITIAL_PATH: &str = "/query";
    const FETCH_PATH: &str = "/query/fetch";

    #[derive(Debug)]
    struct Request {
        path: String,
        body: Value,
        user_agent: String,
    }

    struct ScriptedTransport {
        responses: Mutex<VecDeque<Result<Value>>>,
        requests: Mutex<Vec<Request>>,
    }

    impl ScriptedTransport {
        fn new(responses: Vec<Result<Value>>) -> Self {
            Self {
                responses: Mutex::new(responses.into()),
                requests: Mutex::new(Vec::new()),
            }
        }

        fn requests(&self) -> std::sync::MutexGuard<'_, Vec<Request>> {
            self.requests.lock().unwrap()
        }
    }

    #[async_trait]
    impl QueryTransport for ScriptedTransport {
        async fn post(&self, path: &str, body: Value, user_agent: &str) -> Result<Value> {
            self.requests.lock().unwrap().push(Request {
                path: path.to_string(),
                body,
                user_agent: user_agent.to_string(),
            });
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .ok_or_else(|| anyhow!("scripted response queue exhausted"))?
        }
    }

    #[derive(Default)]
    struct RecordingSleeper {
        durations: Mutex<Vec<Duration>>,
    }

    #[async_trait]
    impl Sleeper for RecordingSleeper {
        async fn sleep(&self, duration: Duration) {
            self.durations.lock().unwrap().push(duration);
        }
    }

    struct SequenceJitter {
        millis: Mutex<VecDeque<u64>>,
    }

    impl SequenceJitter {
        fn new(millis: impl IntoIterator<Item = u64>) -> Self {
            Self {
                millis: Mutex::new(millis.into_iter().collect()),
            }
        }
    }

    impl JitterSource for SequenceJitter {
        fn sample_millis(&self) -> u64 {
            self.millis.lock().unwrap().pop_front().unwrap_or(0)
        }
    }

    #[derive(Default)]
    struct RecordingProgressOutput {
        stderr: Mutex<Vec<String>>,
    }

    impl ProgressOutput for RecordingProgressOutput {
        fn write_stderr(&self, message: &str) {
            self.stderr.lock().unwrap().push(message.to_string());
        }
    }

    fn running() -> Value {
        running_with_id("query-123")
    }

    fn running_with_id(query_id: &str) -> Value {
        json!({"state": "running", "query_id": query_id})
    }

    fn completed() -> Value {
        json!({"state": "completed"})
    }

    fn extract_status(value: &Value) -> Result<Option<String>> {
        match value["state"].as_str() {
            Some("running") => Ok(Some(
                value["query_id"]
                    .as_str()
                    .ok_or_else(|| anyhow!("missing query id"))?
                    .to_string(),
            )),
            Some("completed") => Ok(None),
            state => Err(anyhow!("unexpected state: {state:?}")),
        }
    }

    fn fetch_body(query_id: &str) -> Value {
        json!({"query_id": query_id})
    }

    fn http_error(status: u16, remaining: Option<&str>, reset: Option<&str>) -> anyhow::Error {
        raw_client::HttpError {
            status,
            method: "POST".to_string(),
            url: "https://example.test/query/fetch".to_string(),
            body: "temporary failure".to_string(),
            rate_limit: if remaining.is_some() || reset.is_some() {
                Some(RateLimitInfo {
                    remaining: remaining.map(str::to_string),
                    reset: reset.map(str::to_string),
                    ..Default::default()
                })
            } else {
                None
            },
        }
        .into()
    }

    fn test_client<'a>(
        transport: &'a dyn QueryTransport,
        sleeper: &'a dyn Sleeper,
        jitter: &'a dyn JitterSource,
        progress: Arc<dyn ProgressOutput>,
    ) -> AdvancedQueryClient<'a> {
        AdvancedQueryClient {
            transport,
            sleeper,
            jitter,
            progress,
        }
    }

    #[tokio::test]
    async fn successful_running_response_fetches_immediately() {
        let transport = ScriptedTransport::new(vec![Ok(running()), Ok(completed())]);
        let sleeper = RecordingSleeper::default();
        let jitter = SequenceJitter::new([]);
        let client = test_client(
            &transport,
            &sleeper,
            &jitter,
            Arc::new(RecordingProgressOutput::default()),
        );

        client
            .execute(
                INITIAL_PATH,
                FETCH_PATH,
                json!({"query": "SELECT 1"}),
                "pup/test",
                extract_status,
                fetch_body,
            )
            .await
            .unwrap();

        assert!(sleeper.durations.lock().unwrap().is_empty());
        let requests = transport.requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].path, INITIAL_PATH);
        assert_eq!(requests[1].path, FETCH_PATH);
        assert_eq!(requests[1].body, json!({"query_id": "query-123"}));
        assert_eq!(requests[1].user_agent, "pup/test");
    }

    #[tokio::test]
    async fn many_successful_polls_change_query_ids_without_sleep_or_attempt_cap() {
        let mut responses = vec![Ok(running_with_id("query-0"))];
        responses.extend((1..=128).map(|index| Ok(running_with_id(&format!("query-{index}")))));
        responses.push(Ok(completed()));
        let transport = ScriptedTransport::new(responses);
        let sleeper = RecordingSleeper::default();
        let jitter = SequenceJitter::new([]);
        let client = test_client(
            &transport,
            &sleeper,
            &jitter,
            Arc::new(RecordingProgressOutput::default()),
        );

        client
            .execute(
                INITIAL_PATH,
                FETCH_PATH,
                json!({}),
                "pup/test",
                extract_status,
                fetch_body,
            )
            .await
            .unwrap();

        assert!(sleeper.durations.lock().unwrap().is_empty());
        let requests = transport.requests();
        assert_eq!(requests.len(), 130);
        assert_eq!(requests[1].body, json!({"query_id": "query-0"}));
        assert_eq!(requests[129].body, json!({"query_id": "query-128"}));
    }

    #[tokio::test]
    async fn submission_failure_is_not_retried() {
        let transport =
            ScriptedTransport::new(vec![Err(http_error(429, None, None)), Ok(completed())]);
        let sleeper = RecordingSleeper::default();
        let jitter = SequenceJitter::new([999]);
        let client = test_client(
            &transport,
            &sleeper,
            &jitter,
            Arc::new(RecordingProgressOutput::default()),
        );

        let error = client
            .execute(
                INITIAL_PATH,
                FETCH_PATH,
                json!({}),
                "pup/test",
                extract_status,
                fetch_body,
            )
            .await
            .unwrap_err();

        assert_eq!(http_status(&error), Some(429));
        assert_eq!(transport.requests().len(), 1);
        assert!(sleeper.durations.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn retryable_fetch_statuses_use_exact_attempt_limits_and_bases() {
        for (status, expected_delays) in [
            (429, vec![2, 4, 8, 16, 30]),
            (500, vec![1, 2]),
            (502, vec![1, 2]),
            (503, vec![1, 2]),
            (504, vec![1, 2]),
        ] {
            let mut responses = vec![Ok(running())];
            responses
                .extend((0..=expected_delays.len()).map(|_| Err(http_error(status, None, None))));
            let transport = ScriptedTransport::new(responses);
            let sleeper = RecordingSleeper::default();
            let jitter = SequenceJitter::new(std::iter::repeat_n(0, expected_delays.len()));
            let client = test_client(
                &transport,
                &sleeper,
                &jitter,
                Arc::new(RecordingProgressOutput::default()),
            );

            let error = client
                .execute(
                    INITIAL_PATH,
                    FETCH_PATH,
                    json!({}),
                    "pup/test",
                    extract_status,
                    fetch_body,
                )
                .await
                .unwrap_err();

            assert!(error.to_string().contains("query_id: query-123"));
            assert_eq!(
                sleeper.durations.lock().unwrap().as_slice(),
                expected_delays
                    .iter()
                    .copied()
                    .map(Duration::from_secs)
                    .collect::<Vec<_>>()
                    .as_slice(),
                "status {status}"
            );
            assert_eq!(
                transport
                    .requests()
                    .iter()
                    .filter(|request| request.path == FETCH_PATH)
                    .count(),
                expected_delays.len() + 1,
                "status {status}"
            );
        }
    }

    #[tokio::test]
    async fn mixed_statuses_share_attempt_count_and_carried_exponential_base() {
        // The browser carries one attempted count and one exponential base. The
        // first 5xx chooses 1s and the following 429 continues at 2s. Supported
        // 5xx statuses share a two-retry gate, so the later 502 stops immediately.
        let transport = ScriptedTransport::new(vec![
            Ok(running()),
            Err(http_error(500, None, None)),
            Err(http_error(429, None, None)),
            Err(http_error(502, None, None)),
            Ok(completed()),
        ]);
        let sleeper = RecordingSleeper::default();
        let jitter = SequenceJitter::new([0, 0, 0]);
        let client = test_client(
            &transport,
            &sleeper,
            &jitter,
            Arc::new(RecordingProgressOutput::default()),
        );

        let error = client
            .execute(
                INITIAL_PATH,
                FETCH_PATH,
                json!({}),
                "pup/test",
                extract_status,
                fetch_body,
            )
            .await
            .unwrap_err();

        assert!(error.to_string().contains("HTTP 502"));
        assert_eq!(
            sleeper.durations.lock().unwrap().as_slice(),
            [Duration::from_secs(1), Duration::from_secs(2)]
        );
        assert_eq!(transport.requests().len(), 4);
    }

    #[tokio::test]
    async fn mixed_statuses_use_first_status_base_and_current_status_retry_gate() {
        let transport = ScriptedTransport::new(vec![
            Ok(running()),
            Err(http_error(429, None, None)),
            Err(http_error(500, None, None)),
            Err(http_error(429, None, None)),
            Err(http_error(429, None, None)),
            Err(http_error(429, None, None)),
            Ok(completed()),
        ]);
        let sleeper = RecordingSleeper::default();
        let jitter = SequenceJitter::new([0, 0, 0, 0, 0]);
        let client = test_client(
            &transport,
            &sleeper,
            &jitter,
            Arc::new(RecordingProgressOutput::default()),
        );

        let response = client
            .execute(
                INITIAL_PATH,
                FETCH_PATH,
                json!({}),
                "pup/test",
                extract_status,
                fetch_body,
            )
            .await
            .unwrap();

        assert_eq!(response, completed());
        assert_eq!(
            sleeper.durations.lock().unwrap().as_slice(),
            [2, 4, 8, 16, 30].map(Duration::from_secs)
        );
    }

    #[tokio::test]
    async fn retryable_fetch_can_recover() {
        let transport = ScriptedTransport::new(vec![
            Ok(running()),
            Err(http_error(503, None, None)),
            Ok(completed()),
        ]);
        let sleeper = RecordingSleeper::default();
        let jitter = SequenceJitter::new([250]);
        let client = test_client(
            &transport,
            &sleeper,
            &jitter,
            Arc::new(RecordingProgressOutput::default()),
        );

        let response = client
            .execute(
                INITIAL_PATH,
                FETCH_PATH,
                json!({}),
                "pup/test",
                extract_status,
                fetch_body,
            )
            .await
            .unwrap();

        assert_eq!(response, completed());
        assert_eq!(
            sleeper.durations.lock().unwrap().as_slice(),
            [Duration::from_millis(1_250)]
        );
    }

    #[tokio::test]
    async fn consecutive_jitter_samples_do_not_change_next_exponential_base() {
        let transport = ScriptedTransport::new(vec![
            Ok(running()),
            Err(http_error(429, None, None)),
            Err(http_error(429, None, None)),
            Ok(completed()),
        ]);
        let sleeper = RecordingSleeper::default();
        let jitter = SequenceJitter::new([750, 0]);
        let client = test_client(
            &transport,
            &sleeper,
            &jitter,
            Arc::new(RecordingProgressOutput::default()),
        );

        client
            .execute(
                INITIAL_PATH,
                FETCH_PATH,
                json!({}),
                "pup/test",
                extract_status,
                fetch_body,
            )
            .await
            .unwrap();

        assert_eq!(
            sleeper.durations.lock().unwrap().as_slice(),
            [Duration::from_millis(2_750), Duration::from_secs(4)]
        );
    }

    #[tokio::test]
    async fn non_retryable_fetch_error_surfaces_immediately_with_query_id() {
        let transport = ScriptedTransport::new(vec![
            Ok(running()),
            Err(http_error(400, None, None)),
            Ok(completed()),
        ]);
        let sleeper = RecordingSleeper::default();
        let jitter = SequenceJitter::new([999]);
        let client = test_client(
            &transport,
            &sleeper,
            &jitter,
            Arc::new(RecordingProgressOutput::default()),
        );

        let error = client
            .execute(
                INITIAL_PATH,
                FETCH_PATH,
                json!({}),
                "pup/test",
                extract_status,
                fetch_body,
            )
            .await
            .unwrap_err();

        assert!(error.to_string().contains("HTTP 400"));
        assert!(error.to_string().contains("query_id: query-123"));
        assert_eq!(
            error
                .downcast_ref::<raw_client::HttpError>()
                .map(|source| source.status),
            Some(400),
            "query context must preserve the structured HTTP source"
        );
        assert!(error
            .chain()
            .any(|source| source.to_string().contains("HTTP 400")));
        assert_eq!(transport.requests().len(), 2);
        assert!(sleeper.durations.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn non_http_fetch_failure_surfaces_immediately_with_query_id() {
        let transport = ScriptedTransport::new(vec![
            Ok(running()),
            Err(anyhow!("connection reset by peer")),
            Ok(completed()),
        ]);
        let sleeper = RecordingSleeper::default();
        let jitter = SequenceJitter::new([999]);
        let client = test_client(
            &transport,
            &sleeper,
            &jitter,
            Arc::new(RecordingProgressOutput::default()),
        );

        let error = client
            .execute(
                INITIAL_PATH,
                FETCH_PATH,
                json!({}),
                "pup/test",
                extract_status,
                fetch_body,
            )
            .await
            .unwrap_err();

        assert!(error.to_string().contains("query_id: query-123"));
        assert!(error.to_string().contains("connection reset by peer"));
        assert_eq!(transport.requests().len(), 2);
        assert!(sleeper.durations.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn malformed_successful_fetch_includes_current_query_id() {
        let transport = ScriptedTransport::new(vec![
            Ok(running_with_id("current-query")),
            Ok(json!({"state": "unknown"})),
        ]);
        let sleeper = RecordingSleeper::default();
        let jitter = SequenceJitter::new([]);
        let client = test_client(
            &transport,
            &sleeper,
            &jitter,
            Arc::new(RecordingProgressOutput::default()),
        );

        let error = client
            .execute(
                INITIAL_PATH,
                FETCH_PATH,
                json!({}),
                "pup/test",
                extract_status,
                fetch_body,
            )
            .await
            .unwrap_err();

        assert!(error.to_string().contains("query_id: current-query"));
        assert!(error.to_string().contains("unexpected state"));
        assert!(sleeper.durations.lock().unwrap().is_empty());
    }

    #[test]
    fn retry_jitter_is_additive_independent_and_capped() {
        assert_eq!(
            retry_delay(Duration::from_secs(2), None, Duration::from_millis(999)),
            Duration::from_millis(2_999)
        );
        assert_eq!(
            retry_delay(
                Duration::from_secs(4),
                Some(Duration::from_secs(3)),
                Duration::from_millis(250)
            ),
            Duration::from_millis(3_250),
            "a selected header replaces the exponential base"
        );
        assert_eq!(
            retry_delay(Duration::from_secs(30), None, Duration::from_millis(999)),
            Duration::from_secs(30)
        );
    }

    #[test]
    fn random_jitter_stays_within_browser_bounds() {
        let jitter = RandomJitter;
        for _ in 0..10_000 {
            assert!(jitter.sample_millis() <= 999);
        }
    }

    #[test]
    fn rate_limit_headers_pair_values_and_choose_greatest_exhausted_reset() {
        assert_eq!(paired_reset_seconds("0, 1, -2", "3, 20, 8.5"), Some(8.5));
        assert_eq!(paired_reset_seconds("0, 0", "5, 12"), Some(12.0));
    }

    #[test]
    fn rate_limit_headers_ignore_malformed_and_incomplete_pairs() {
        assert_eq!(paired_reset_seconds("bad, 0, NaN, 0", "4, nope, 7"), None);
        assert_eq!(
            paired_reset_seconds("bad, 0", "4, 7"),
            Some(7.0),
            "a malformed pair does not hide a later valid exhausted limit"
        );
        assert_eq!(
            paired_reset_seconds("1, 0", "20"),
            None,
            "the exhausted remaining value has no reset partner"
        );
        assert_eq!(
            paired_reset_seconds("0", "2, 50"),
            Some(2.0),
            "an extra reset value is ignored"
        );
        assert_eq!(paired_reset_seconds("1, 2", "5, 9"), None);
        assert_eq!(paired_reset_seconds("0", "-1"), None);
        assert_eq!(paired_reset_seconds("0", "inf"), None);
    }

    #[test]
    fn rate_limit_header_preserves_finite_values_larger_than_duration() {
        assert_eq!(paired_reset_seconds("0", "1e300"), Some(1e300));
    }

    #[tokio::test]
    async fn rate_limit_header_delay_over_cap_stops_without_retrying_early() {
        let transport = ScriptedTransport::new(vec![
            Ok(running()),
            Err(http_error(429, Some("0"), Some("1e300"))),
            Ok(completed()),
        ]);
        let sleeper = RecordingSleeper::default();
        let jitter = SequenceJitter::new([0]);
        let client = test_client(
            &transport,
            &sleeper,
            &jitter,
            Arc::new(RecordingProgressOutput::default()),
        );

        let error = client
            .execute(
                INITIAL_PATH,
                FETCH_PATH,
                json!({}),
                "pup/test",
                extract_status,
                fetch_body,
            )
            .await
            .unwrap_err();

        assert!(error.to_string().contains("exceeds the 30s retry cap"));
        assert!(error.to_string().contains("query_id: query-123"));
        assert_eq!(
            error
                .downcast_ref::<raw_client::HttpError>()
                .map(|source| source.status),
            Some(429),
            "the header-stop context must preserve the structured HTTP source"
        );
        assert!(sleeper.durations.lock().unwrap().is_empty());
        assert_eq!(transport.requests().len(), 2);
    }

    #[tokio::test]
    async fn smaller_rate_limit_header_replaces_base_without_changing_next_base() {
        let transport = ScriptedTransport::new(vec![
            Ok(running()),
            Err(http_error(429, Some("0"), Some("1"))),
            Err(http_error(429, None, None)),
            Ok(completed()),
        ]);
        let sleeper = RecordingSleeper::default();
        let jitter = SequenceJitter::new([0, 0]);
        let client = test_client(
            &transport,
            &sleeper,
            &jitter,
            Arc::new(RecordingProgressOutput::default()),
        );

        client
            .execute(
                INITIAL_PATH,
                FETCH_PATH,
                json!({}),
                "pup/test",
                extract_status,
                fetch_body,
            )
            .await
            .unwrap();

        assert_eq!(
            sleeper.durations.lock().unwrap().as_slice(),
            [Duration::from_secs(1), Duration::from_secs(4)]
        );
    }

    #[tokio::test]
    async fn rate_limit_header_delay_combines_with_jitter_and_complete_cap() {
        let transport = ScriptedTransport::new(vec![
            Ok(running()),
            Err(http_error(429, Some("0, 0"), Some("1, 29.5"))),
            Ok(completed()),
        ]);
        let sleeper = RecordingSleeper::default();
        let jitter = SequenceJitter::new([999]);
        let client = test_client(
            &transport,
            &sleeper,
            &jitter,
            Arc::new(RecordingProgressOutput::default()),
        );

        client
            .execute(
                INITIAL_PATH,
                FETCH_PATH,
                json!({}),
                "pup/test",
                extract_status,
                fetch_body,
            )
            .await
            .unwrap();

        assert_eq!(
            sleeper.durations.lock().unwrap().as_slice(),
            [Duration::from_secs(30)]
        );
    }

    #[tokio::test(start_paused = true)]
    async fn progress_is_emitted_at_ten_and_thirty_seconds_to_stderr_only() {
        let output = Arc::new(RecordingProgressOutput::default());
        let reporter = ProgressReporter::start(output.clone());
        tokio::task::yield_now().await;

        tokio::time::advance(Duration::from_secs(9)).await;
        tokio::task::yield_now().await;
        assert!(output.stderr.lock().unwrap().is_empty());

        tokio::time::advance(Duration::from_secs(1)).await;
        tokio::task::yield_now().await;
        assert_eq!(
            output.stderr.lock().unwrap().as_slice(),
            [INITIAL_PROGRESS_MESSAGE]
        );

        tokio::time::advance(Duration::from_secs(20)).await;
        tokio::task::yield_now().await;
        assert_eq!(
            output.stderr.lock().unwrap().as_slice(),
            [INITIAL_PROGRESS_MESSAGE, SECOND_PROGRESS_MESSAGE]
        );
        drop(reporter);
    }

    #[tokio::test(start_paused = true)]
    async fn dropped_reporter_emits_no_progress() {
        let output = Arc::new(RecordingProgressOutput::default());
        let reporter = ProgressReporter::start(output.clone());
        tokio::task::yield_now().await;
        drop(reporter);

        tokio::time::advance(Duration::from_secs(30)).await;
        tokio::task::yield_now().await;
        assert!(output.stderr.lock().unwrap().is_empty());
    }

    struct BlockingFetchTransport {
        requests: AtomicUsize,
        fetch_started: Notify,
        release_fetch: Notify,
    }

    impl BlockingFetchTransport {
        fn new() -> Self {
            Self {
                requests: AtomicUsize::new(0),
                fetch_started: Notify::new(),
                release_fetch: Notify::new(),
            }
        }
    }

    #[async_trait]
    impl QueryTransport for BlockingFetchTransport {
        async fn post(&self, _path: &str, _body: Value, _user_agent: &str) -> Result<Value> {
            if self.requests.fetch_add(1, Ordering::SeqCst) == 0 {
                return Ok(running());
            }
            self.fetch_started.notify_one();
            self.release_fetch.notified().await;
            Ok(completed())
        }
    }

    #[tokio::test(start_paused = true)]
    async fn query_lifecycle_emits_progress_at_ten_and_thirty_seconds() {
        let transport = Arc::new(BlockingFetchTransport::new());
        let sleeper = Arc::new(RecordingSleeper::default());
        let jitter = Arc::new(SequenceJitter::new([]));
        let progress = Arc::new(RecordingProgressOutput::default());
        let task_transport = Arc::clone(&transport);
        let task_sleeper = Arc::clone(&sleeper);
        let task_jitter = Arc::clone(&jitter);
        let task_progress = Arc::clone(&progress);
        let task = tokio::spawn(async move {
            let client = test_client(
                task_transport.as_ref(),
                task_sleeper.as_ref(),
                task_jitter.as_ref(),
                task_progress,
            );
            client
                .execute(
                    INITIAL_PATH,
                    FETCH_PATH,
                    json!({}),
                    "pup/test",
                    extract_status,
                    fetch_body,
                )
                .await
        });

        transport.fetch_started.notified().await;
        tokio::time::advance(Duration::from_secs(9)).await;
        tokio::task::yield_now().await;
        assert!(progress.stderr.lock().unwrap().is_empty());
        tokio::time::advance(Duration::from_secs(1)).await;
        tokio::task::yield_now().await;
        assert_eq!(
            progress.stderr.lock().unwrap().as_slice(),
            [INITIAL_PROGRESS_MESSAGE]
        );
        tokio::time::advance(Duration::from_secs(20)).await;
        tokio::task::yield_now().await;
        assert_eq!(
            progress.stderr.lock().unwrap().as_slice(),
            [INITIAL_PROGRESS_MESSAGE, SECOND_PROGRESS_MESSAGE]
        );

        transport.release_fetch.notify_one();
        assert_eq!(task.await.unwrap().unwrap(), completed());
    }

    #[tokio::test]
    async fn caller_can_cancel_query_while_fetch_is_pending() {
        let transport = Arc::new(BlockingFetchTransport::new());
        let sleeper = Arc::new(RecordingSleeper::default());
        let jitter = Arc::new(SequenceJitter::new([]));
        let progress = Arc::new(RecordingProgressOutput::default());
        let task_transport = Arc::clone(&transport);
        let task_sleeper = Arc::clone(&sleeper);
        let task_jitter = Arc::clone(&jitter);
        let task = tokio::spawn(async move {
            let client = test_client(
                task_transport.as_ref(),
                task_sleeper.as_ref(),
                task_jitter.as_ref(),
                progress,
            );
            client
                .execute(
                    INITIAL_PATH,
                    FETCH_PATH,
                    json!({}),
                    "pup/test",
                    extract_status,
                    fetch_body,
                )
                .await
        });

        transport.fetch_started.notified().await;
        task.abort();
        let error = task.await.unwrap_err();
        assert!(error.is_cancelled());
        assert_eq!(transport.requests.load(Ordering::SeqCst), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn client_has_no_overall_elapsed_time_deadline() {
        let transport = Arc::new(BlockingFetchTransport::new());
        let sleeper = Arc::new(RecordingSleeper::default());
        let jitter = Arc::new(SequenceJitter::new([]));
        let progress = Arc::new(RecordingProgressOutput::default());
        let task_transport = Arc::clone(&transport);
        let task_sleeper = Arc::clone(&sleeper);
        let task_jitter = Arc::clone(&jitter);
        let task = tokio::spawn(async move {
            let client = test_client(
                task_transport.as_ref(),
                task_sleeper.as_ref(),
                task_jitter.as_ref(),
                progress,
            );
            client
                .execute(
                    INITIAL_PATH,
                    FETCH_PATH,
                    json!({}),
                    "pup/test",
                    extract_status,
                    fetch_body,
                )
                .await
        });

        transport.fetch_started.notified().await;
        tokio::time::advance(Duration::from_secs(24 * 60 * 60)).await;
        tokio::task::yield_now().await;
        assert!(!task.is_finished());

        transport.release_fetch.notify_one();
        assert_eq!(task.await.unwrap().unwrap(), completed());
    }
}
