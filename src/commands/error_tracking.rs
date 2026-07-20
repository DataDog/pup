use anyhow::Result;
use datadog_api_client::datadogV2::api_error_tracking::{
    ErrorTrackingAPI, GetIssueOptionalParams, SearchIssuesOptionalParams,
};
use datadog_api_client::datadogV2::model::{
    IssueState, IssuesSearchRequest, IssuesSearchRequestData, IssuesSearchRequestDataAttributes,
    IssuesSearchRequestDataAttributesOrderBy, IssuesSearchRequestDataAttributesPersona,
    IssuesSearchRequestDataAttributesTrack, IssuesSearchRequestDataType,
};

use crate::config::Config;
use crate::formatter;
use crate::util_ext;

#[allow(clippy::too_many_arguments)]
pub async fn issues_search(
    cfg: &Config,
    query: Option<String>,
    limit: i32,
    from: String,
    to: String,
    order_by: String,
    track: Option<String>,
    persona: Option<String>,
    state: Option<String>,
    team: Option<String>,
    assignee: Option<String>,
) -> Result<()> {
    let api = crate::make_api!(ErrorTrackingAPI, cfg);

    let from_ms = util_ext::parse_time_to_unix_millis(&from)?;
    let to_ms = util_ext::parse_time_to_unix_millis(&to)?;

    let order_by_val = match order_by.to_uppercase().as_str() {
        "TOTAL_COUNT" => IssuesSearchRequestDataAttributesOrderBy::TOTAL_COUNT,
        "FIRST_SEEN" => IssuesSearchRequestDataAttributesOrderBy::FIRST_SEEN,
        "IMPACTED_SESSIONS" => IssuesSearchRequestDataAttributesOrderBy::IMPACTED_SESSIONS,
        "PRIORITY" => IssuesSearchRequestDataAttributesOrderBy::PRIORITY,
        other => anyhow::bail!(
            "invalid --order-by value: {other:?}\nExpected: TOTAL_COUNT, FIRST_SEEN, IMPACTED_SESSIONS, PRIORITY"
        ),
    };

    let query_str = query.unwrap_or_else(|| "*".to_string());
    let mut attrs =
        IssuesSearchRequestDataAttributes::new(from_ms, query_str, to_ms).order_by(order_by_val);
    if let Some(ref t) = track {
        let track_value = match t.to_lowercase().as_str() {
            "trace" => IssuesSearchRequestDataAttributesTrack::TRACE,
            "logs" => IssuesSearchRequestDataAttributesTrack::LOGS,
            "rum" => IssuesSearchRequestDataAttributesTrack::RUM,
            other => anyhow::bail!(
                "invalid track value '{}': must be trace, logs, or rum",
                other
            ),
        };
        attrs = attrs.track(track_value);
    }
    if let Some(ref p) = persona {
        let persona_value = match p.to_uppercase().as_str() {
            "ALL" => IssuesSearchRequestDataAttributesPersona::ALL,
            "BROWSER" => IssuesSearchRequestDataAttributesPersona::BROWSER,
            "MOBILE" => IssuesSearchRequestDataAttributesPersona::MOBILE,
            "BACKEND" => IssuesSearchRequestDataAttributesPersona::BACKEND,
            other => anyhow::bail!(
                "invalid persona value '{}': must be ALL, BROWSER, MOBILE, or BACKEND",
                other
            ),
        };
        attrs = attrs.persona(persona_value);
    }
    if let Some(ref s) = state {
        let state_value = match s.to_uppercase().as_str() {
            "OPEN" => IssueState::OPEN,
            "ACKNOWLEDGED" => IssueState::ACKNOWLEDGED,
            "RESOLVED" => IssueState::RESOLVED,
            "IGNORED" => IssueState::IGNORED,
            "EXCLUDED" => IssueState::EXCLUDED,
            other => anyhow::bail!(
                "invalid --state value '{}': must be OPEN, ACKNOWLEDGED, RESOLVED, IGNORED, or EXCLUDED",
                other
            ),
        };
        attrs = attrs.states(vec![state_value]);
    }
    if let Some(ref t) = team {
        let team_id = uuid::Uuid::parse_str(t)
            .map_err(|_| anyhow::anyhow!("invalid --team value '{}': must be a valid UUID", t))?;
        attrs = attrs.team_ids(vec![team_id]);
    }
    if let Some(ref a) = assignee {
        let assignee_id = uuid::Uuid::parse_str(a).map_err(|_| {
            anyhow::anyhow!("invalid --assignee value '{}': must be a valid UUID", a)
        })?;
        attrs = attrs.assignee_ids(vec![assignee_id]);
    }
    let data = IssuesSearchRequestData::new(attrs, IssuesSearchRequestDataType::SEARCH_REQUEST);
    let body = IssuesSearchRequest::new(data);
    let params = SearchIssuesOptionalParams::default();

    let mut resp = api
        .search_issues(body, params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to search issues: {e:?}"))?;

    if resp.data.as_ref().is_some_and(|d| d.is_empty()) {
        eprintln!("No error tracking issues found matching the specified criteria.");
        return Ok(());
    }

    if limit > 0 {
        if let Some(data) = resp.data.as_mut() {
            data.truncate(limit as usize);
        }
    }

    formatter::output(cfg, &resp)
}

pub async fn issues_get(cfg: &Config, issue_id: &str) -> Result<()> {
    let api = crate::make_api!(ErrorTrackingAPI, cfg);
    let resp = api
        .get_issue(issue_id.to_string(), GetIssueOptionalParams::default())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get issue: {e:?}"))?;
    formatter::output(cfg, &resp)
}

#[cfg(test)]
mod tests {

    use crate::test_support::*;
    use clap::CommandFactory;

    #[tokio::test]
    async fn test_error_tracking_issues_search() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::issues_search(
            &cfg,
            None,
            10,
            "1d".into(),
            "now".into(),
            "TOTAL_COUNT".into(),
            Some("trace".into()),
            None,
            None,
            None,
            None,
        )
        .await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_error_tracking_issues_search_persona() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::issues_search(
            &cfg,
            None,
            10,
            "1d".into(),
            "now".into(),
            "TOTAL_COUNT".into(),
            None,
            Some("BROWSER".into()),
            None,
            None,
            None,
        )
        .await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_error_tracking_issues_search_track_case_insensitive() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::issues_search(
            &cfg,
            None,
            10,
            "1d".into(),
            "now".into(),
            "TOTAL_COUNT".into(),
            Some("RUM".into()),
            None,
            None,
            None,
            None,
        )
        .await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_error_tracking_issues_search_invalid_order_by() {
        let cfg = test_config("http://unused.local");
        let result = super::issues_search(
            &cfg,
            None,
            10,
            "1d".into(),
            "now".into(),
            "INVALID".into(),
            Some("trace".into()),
            None,
            None,
            None,
            None,
        )
        .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid --order-by value"));
    }

    #[tokio::test]
    async fn test_issues_search_with_state() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let result = super::issues_search(
            &cfg,
            None,
            10,
            "1d".into(),
            "now".into(),
            "TOTAL_COUNT".into(),
            Some("trace".into()),
            None,
            Some("OPEN".into()),
            None,
            None,
        )
        .await;
        assert!(result.is_ok());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_issues_search_invalid_state() {
        let cfg = test_config("http://unused.local");
        let result = super::issues_search(
            &cfg,
            None,
            10,
            "1d".into(),
            "now".into(),
            "TOTAL_COUNT".into(),
            Some("trace".into()),
            None,
            Some("BADSTATE".into()),
            None,
            None,
        )
        .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid --state value"));
    }

    #[tokio::test]
    async fn test_issues_search_invalid_team_uuid() {
        let cfg = test_config("http://unused.local");
        let result = super::issues_search(
            &cfg,
            None,
            10,
            "1d".into(),
            "now".into(),
            "TOTAL_COUNT".into(),
            Some("trace".into()),
            None,
            None,
            Some("not-a-uuid".into()),
            None,
        )
        .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid --team value"));
    }

    #[tokio::test]
    async fn test_issues_search_invalid_assignee_uuid() {
        let cfg = test_config("http://unused.local");
        let result = super::issues_search(
            &cfg,
            None,
            10,
            "1d".into(),
            "now".into(),
            "TOTAL_COUNT".into(),
            Some("trace".into()),
            None,
            None,
            None,
            Some("not-a-uuid".into()),
        )
        .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid --assignee value"));
    }

    #[test]
    fn test_error_tracking_clap_mutual_exclusivity() {
        let result = crate::Cli::command().try_get_matches_from([
            "pup",
            "error-tracking",
            "issues",
            "search",
            "--track",
            "trace",
            "--persona",
            "ALL",
        ]);
        assert!(
            result.is_err(),
            "expected error when both --track and --persona are provided"
        );
    }

    #[test]
    fn test_error_tracking_clap_neither_provided() {
        let result = crate::Cli::command().try_get_matches_from([
            "pup",
            "error-tracking",
            "issues",
            "search",
        ]);
        assert!(
            result.is_err(),
            "expected error when neither --track nor --persona is provided"
        );
    }

    #[test]
    fn test_error_tracking_clap_state_accepted() {
        let result = crate::Cli::command().try_get_matches_from([
            "pup",
            "error-tracking",
            "issues",
            "search",
            "--track",
            "trace",
            "--state",
            "OPEN",
        ]);
        assert!(
            result.is_ok(),
            "expected success when --state is provided alongside --track"
        );
    }
}
