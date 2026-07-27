use anyhow::Result;
use datadog_api_client::datadogV1::api_service_level_objectives::{
    DeleteSLOOptionalParams, GetSLOHistoryOptionalParams, GetSLOOptionalParams,
    ListSLOsOptionalParams, ServiceLevelObjectivesAPI,
};
use datadog_api_client::datadogV1::model::{
    SLOThreshold, SLOType, ServiceLevelObjective, ServiceLevelObjectiveRequest,
};
use datadog_api_client::datadogV2::model::RawErrorBudgetRemaining;

use crate::config::Config;
use crate::formatter;
use crate::util;

pub async fn list(
    cfg: &Config,
    query: Option<String>,
    tags_query: Option<String>,
    metrics_query: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<()> {
    let api = crate::make_api!(ServiceLevelObjectivesAPI, cfg);
    let mut params = ListSLOsOptionalParams::default();
    if let Some(query) = query {
        params = params.query(query);
    }
    if let Some(tags_query) = tags_query {
        params = params.tags_query(tags_query);
    }
    if let Some(metrics_query) = metrics_query {
        params = params.metrics_query(metrics_query);
    }
    if let Some(limit) = limit {
        params = params.limit(limit);
    }
    if let Some(offset) = offset {
        params = params.offset(offset);
    }
    let resp = api
        .list_slos(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list SLOs: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn get(cfg: &Config, id: &str) -> Result<()> {
    let api = crate::make_api!(ServiceLevelObjectivesAPI, cfg);
    let resp = api
        .get_slo(id.to_string(), GetSLOOptionalParams::default())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get SLO: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn create(cfg: &Config, file: &str) -> Result<()> {
    let body: ServiceLevelObjectiveRequest = util::read_json_file(file)?;
    let api = crate::make_api!(ServiceLevelObjectivesAPI, cfg);
    let resp = api
        .create_slo(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create SLO: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn update(cfg: &Config, id: &str, file: &str) -> Result<()> {
    let body: ServiceLevelObjective = util::read_json_file(file)?;
    let api = crate::make_api!(ServiceLevelObjectivesAPI, cfg);
    let resp = api
        .update_slo(id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update SLO: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn delete(cfg: &Config, id: &str) -> Result<()> {
    let api = crate::make_api!(ServiceLevelObjectivesAPI, cfg);
    let resp = api
        .delete_slo(id.to_string(), DeleteSLOOptionalParams::default())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete SLO: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn status(cfg: &Config, id: &str, from_ts: i64, to_ts: i64) -> Result<()> {
    use datadog_api_client::datadogV2::api_service_level_objectives::{
        GetSloStatusOptionalParams, ServiceLevelObjectivesAPI as SloV2API,
    };

    let api = crate::make_api!(SloV2API, cfg);
    let mut resp = api
        .get_slo_status(
            id.to_string(),
            from_ts,
            to_ts,
            GetSloStatusOptionalParams::default(),
        )
        .await
        .map_err(|e| anyhow::anyhow!("failed to get SLO status: {e:?}"))?;

    // The v2 status endpoint always reports 0 for monitor-type SLOs
    // (https://github.com/DataDog/pup/issues/646). Fall back to the v1
    // history endpoint, which computes it correctly when given a target.
    if let Some(budget) = monitor_error_budget_remaining(cfg, id, from_ts, to_ts).await? {
        resp.data.attributes.error_budget_remaining = budget.remaining_pct;
        resp.data.attributes.raw_error_budget_remaining = RawErrorBudgetRemaining::new(
            "seconds".to_string(),
            raw_remaining_seconds(&budget, to_ts - from_ts),
        );
    }

    formatter::output(cfg, &resp)
}

/// A monitor-type SLO's error budget remaining, as a percentage (0-100) of
/// the threshold's allowed unreliability window, along with the threshold
/// target it was computed against.
struct MonitorErrorBudget {
    remaining_pct: f64,
    target: f64,
}

/// Returns the error budget remaining for a monitor-type SLO by querying the
/// v1 history endpoint with the target threshold closest to the requested
/// window, or `None` if the SLO isn't monitor-type or has no thresholds.
async fn monitor_error_budget_remaining(
    cfg: &Config,
    id: &str,
    from_ts: i64,
    to_ts: i64,
) -> Result<Option<MonitorErrorBudget>> {
    let api = crate::make_api!(ServiceLevelObjectivesAPI, cfg);
    let slo = api
        .get_slo(id.to_string(), GetSLOOptionalParams::default())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get SLO: {e:?}"))?;

    let data = match slo.data {
        Some(data) => data,
        None => return Ok(None),
    };
    if data.type_ != Some(SLOType::MONITOR) {
        return Ok(None);
    }
    let target = match data.thresholds.as_deref() {
        Some(thresholds) => closest_threshold_target(thresholds, to_ts - from_ts),
        None => None,
    };
    let target = match target {
        Some(target) => target,
        None => return Ok(None),
    };

    let history = api
        .get_slo_history(
            id.to_string(),
            from_ts,
            to_ts,
            GetSLOHistoryOptionalParams::default().target(target),
        )
        .await
        .map_err(|e| anyhow::anyhow!("failed to get SLO history: {e:?}"))?;

    Ok(history
        .data
        .and_then(|data| data.overall)
        .and_then(|overall| overall.error_budget_remaining)
        .and_then(|remaining| remaining.get("custom").copied())
        .map(|remaining_pct| MonitorErrorBudget {
            remaining_pct,
            target,
        }))
}

/// Converts a monitor SLO's remaining error budget percentage into seconds.
/// The total error budget for the window is `(1 - target) * window`; the
/// remaining budget is the fraction of that still unspent.
fn raw_remaining_seconds(budget: &MonitorErrorBudget, window_secs: i64) -> f64 {
    let total_budget_secs = (1.0 - budget.target / 100.0) * window_secs as f64;
    budget.remaining_pct / 100.0 * total_budget_secs
}

/// Picks the target of the threshold whose timeframe most closely matches
/// the requested window duration (falls back to the first threshold if none
/// have a recognized timeframe).
fn closest_threshold_target(thresholds: &[SLOThreshold], window_secs: i64) -> Option<f64> {
    thresholds
        .iter()
        .min_by_key(|threshold| match threshold.timeframe.to_string().as_str() {
            "7d" => (7 * 86_400 - window_secs).abs(),
            "30d" => (30 * 86_400 - window_secs).abs(),
            "90d" => (90 * 86_400 - window_secs).abs(),
            _ => i64::MAX,
        })
        .map(|threshold| threshold.target)
}

#[cfg(test)]
mod tests {

    use crate::test_support::*;

    #[tokio::test]
    async fn test_slos_list() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let mock = server
            .mock("GET", "/api/v1/slo")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": [], "errors": []}"#)
            .create_async()
            .await;

        let result = super::list(&cfg, None, None, None, None, None).await;
        assert!(result.is_ok(), "slos list failed: {:?}", result.err());
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_slos_list_with_query() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let mock = server
            .mock("GET", "/api/v1/slo")
            .match_query(mockito::Matcher::UrlEncoded(
                "query".into(),
                "monitor-history-reader".into(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": [], "errors": []}"#)
            .create_async()
            .await;

        let result = super::list(
            &cfg,
            Some("monitor-history-reader".into()),
            None,
            None,
            None,
            None,
        )
        .await;
        assert!(
            result.is_ok(),
            "slos list with query failed: {:?}",
            result.err()
        );
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_slos_list_with_tags_query() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let mock = server
            .mock("GET", "/api/v1/slo")
            .match_query(mockito::Matcher::UrlEncoded(
                "tags_query".into(),
                "team:slo-app".into(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": [], "errors": []}"#)
            .create_async()
            .await;

        let result = super::list(&cfg, None, Some("team:slo-app".into()), None, None, None).await;
        assert!(
            result.is_ok(),
            "slos list with tags_query failed: {:?}",
            result.err()
        );
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_slos_list_with_limit_and_offset() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let mock = server
            .mock("GET", "/api/v1/slo")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("limit".into(), "25".into()),
                mockito::Matcher::UrlEncoded("offset".into(), "50".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": [], "errors": []}"#)
            .create_async()
            .await;

        let result = super::list(&cfg, None, None, None, Some(25), Some(50)).await;
        assert!(
            result.is_ok(),
            "slos list with pagination failed: {:?}",
            result.err()
        );
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_slos_list_with_metrics_query() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let mock = server
            .mock("GET", "/api/v1/slo")
            .match_query(mockito::Matcher::UrlEncoded(
                "metrics_query".into(),
                "sum:requests.error{service:api}".into(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": [], "errors": []}"#)
            .create_async()
            .await;

        let result = super::list(
            &cfg,
            None,
            None,
            Some("sum:requests.error{service:api}".into()),
            None,
            None,
        )
        .await;
        assert!(
            result.is_ok(),
            "slos list with metrics_query failed: {:?}",
            result.err()
        );
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_slos_list_api_error() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let mock = server
            .mock("GET", "/api/v1/slo")
            .match_query(mockito::Matcher::UrlEncoded("query".into(), "team".into()))
            .with_status(500)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors":["boom"]}"#)
            .create_async()
            .await;

        let result = super::list(&cfg, Some("team".into()), None, None, None, None).await;
        assert!(
            result.is_err(),
            "slos list error path unexpectedly succeeded"
        );
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("failed to list SLOs"),
            "slos list error did not contain context"
        );
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_slos_get() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(
            &mut server,
            "GET",
            r#"{"data": {"id": "abc123", "name": "Test SLO", "type": "metric", "thresholds": [{"timeframe": "7d", "target": 99.9}]}, "errors": []}"#,
        )
        .await;

        let result = super::get(&cfg, "abc123").await;
        assert!(result.is_ok(), "slos get failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_slos_delete() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "DELETE", r#"{"data": []}"#).await;

        let result = super::delete(&cfg, "abc123").await;
        assert!(result.is_ok(), "slos delete failed: {:?}", result.err());
        cleanup_env();
    }

    fn slo_threshold(timeframe: &str, target: f64) -> super::SLOThreshold {
        let json = format!(r#"{{"timeframe": "{timeframe}", "target": {target}}}"#);
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn test_raw_remaining_seconds() {
        // 99.5% target over a 7-day window allows (1 - 0.995) * 604800 = 3024s
        // of downtime; 86.111% of that budget remaining is ~2603.997s.
        let budget = super::MonitorErrorBudget {
            remaining_pct: 86.111,
            target: 99.5,
        };
        let remaining = super::raw_remaining_seconds(&budget, 7 * 86_400);
        assert!(
            (remaining - 2603.9966).abs() < 0.01,
            "expected ~2603.9966s, got {remaining}"
        );
    }

    #[test]
    fn test_raw_remaining_seconds_full_budget() {
        let budget = super::MonitorErrorBudget {
            remaining_pct: 100.0,
            target: 99.9,
        };
        let remaining = super::raw_remaining_seconds(&budget, 30 * 86_400);
        assert!((remaining - 2592.0).abs() < 0.01, "got {remaining}");
    }

    #[test]
    fn test_closest_threshold_target_exact_match() {
        let thresholds = vec![
            slo_threshold("7d", 99.5),
            slo_threshold("30d", 99.9),
            slo_threshold("90d", 99.99),
        ];
        let target = super::closest_threshold_target(&thresholds, 30 * 86_400);
        assert_eq!(target, Some(99.9));
    }

    #[test]
    fn test_closest_threshold_target_unknown_timeframe_falls_back() {
        let thresholds = vec![slo_threshold("custom", 42.0)];
        let target = super::closest_threshold_target(&thresholds, 60);
        assert_eq!(target, Some(42.0), "should fall back to the only threshold");
    }

    const MONITOR_SLO_STATUS_BODY: &str = r#"{"data": {"attributes": {"error_budget_remaining": 0.0, "raw_error_budget_remaining": {"unit": "second", "value": 0.0}, "sli": 100.0, "span_precision": 3, "state": "ok"}, "id": "abc123", "type": "slo_status"}}"#;
    const MONITOR_SLO_GET_BODY: &str = r#"{"data": {"id": "abc123", "name": "Monitor SLO", "type": "monitor", "thresholds": [{"timeframe": "7d", "target": 99.5}]}, "errors": []}"#;
    const MONITOR_SLO_HISTORY_BODY: &str = r#"{"data": {"overall": {"error_budget_remaining": {"custom": 86.111}, "sli_value": 99.93}}}"#;

    #[tokio::test]
    async fn test_slos_status_monitor_type_falls_back_to_history() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let from_ts = 1_700_000_000_i64;
        let to_ts = from_ts + 7 * 86_400;

        let status_mock = server
            .mock("GET", "/api/v2/slo/abc123/status")
            .match_query(mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(MONITOR_SLO_STATUS_BODY)
            .create_async()
            .await;
        let get_mock = server
            .mock("GET", "/api/v1/slo/abc123")
            .match_query(mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(MONITOR_SLO_GET_BODY)
            .create_async()
            .await;
        let history_mock = server
            .mock("GET", "/api/v1/slo/abc123/history")
            .match_query(mockito::Matcher::UrlEncoded("target".into(), "99.5".into()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(MONITOR_SLO_HISTORY_BODY)
            .create_async()
            .await;

        let result = super::status(&cfg, "abc123", from_ts, to_ts).await;
        assert!(result.is_ok(), "slos status failed: {:?}", result.err());
        status_mock.assert_async().await;
        get_mock.assert_async().await;
        history_mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_slos_status_metric_type_does_not_call_history() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let from_ts = 1_700_000_000_i64;
        let to_ts = from_ts + 7 * 86_400;

        let status_mock = server
            .mock("GET", "/api/v2/slo/abc123/status")
            .match_query(mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(MONITOR_SLO_STATUS_BODY)
            .create_async()
            .await;
        let get_mock = server
            .mock("GET", "/api/v1/slo/abc123")
            .match_query(mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"data": {"id": "abc123", "name": "Metric SLO", "type": "metric", "thresholds": [{"timeframe": "7d", "target": 99.5}]}, "errors": []}"#,
            )
            .create_async()
            .await;
        let history_mock = server
            .mock("GET", "/api/v1/slo/abc123/history")
            .match_query(mockito::Matcher::Any)
            .expect(0)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(MONITOR_SLO_HISTORY_BODY)
            .create_async()
            .await;

        let result = super::status(&cfg, "abc123", from_ts, to_ts).await;
        assert!(result.is_ok(), "slos status failed: {:?}", result.err());
        status_mock.assert_async().await;
        get_mock.assert_async().await;
        history_mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_monitor_error_budget_remaining_extracts_custom_value() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let from_ts = 1_700_000_000_i64;
        let to_ts = from_ts + 7 * 86_400;

        server
            .mock("GET", "/api/v1/slo/abc123")
            .match_query(mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(MONITOR_SLO_GET_BODY)
            .create_async()
            .await;
        server
            .mock("GET", "/api/v1/slo/abc123/history")
            .match_query(mockito::Matcher::UrlEncoded("target".into(), "99.5".into()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(MONITOR_SLO_HISTORY_BODY)
            .create_async()
            .await;

        let budget = super::monitor_error_budget_remaining(&cfg, "abc123", from_ts, to_ts)
            .await
            .unwrap()
            .expect("expected a monitor error budget");
        assert_eq!(budget.remaining_pct, 86.111);
        assert_eq!(budget.target, 99.5);
        cleanup_env();
    }

    #[tokio::test]
    async fn test_monitor_error_budget_remaining_none_for_metric_type() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let from_ts = 1_700_000_000_i64;
        let to_ts = from_ts + 7 * 86_400;

        server
            .mock("GET", "/api/v1/slo/abc123")
            .match_query(mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"data": {"id": "abc123", "name": "Metric SLO", "type": "metric", "thresholds": [{"timeframe": "7d", "target": 99.5}]}, "errors": []}"#,
            )
            .create_async()
            .await;

        let budget = super::monitor_error_budget_remaining(&cfg, "abc123", from_ts, to_ts)
            .await
            .unwrap();
        assert!(budget.is_none());
        cleanup_env();
    }
}
