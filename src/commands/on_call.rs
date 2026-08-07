use anyhow::Result;
use datadog_api_client::datadogV2::api_on_call::{
    CreateOnCallEscalationPolicyOptionalParams, CreateOnCallScheduleOptionalParams,
    GetOnCallEscalationPolicyOptionalParams, GetOnCallScheduleOptionalParams,
    GetUserNotificationRuleOptionalParams, ListUserNotificationRulesOptionalParams, OnCallAPI,
    UpdateOnCallEscalationPolicyOptionalParams, UpdateOnCallScheduleOptionalParams,
    UpdateUserNotificationRuleOptionalParams,
};
use datadog_api_client::datadogV2::api_on_call_paging::OnCallPagingAPI;
use datadog_api_client::datadogV2::api_teams::{
    GetTeamMembershipsOptionalParams, ListTeamsOptionalParams, TeamsAPI,
};
use datadog_api_client::datadogV2::model::{
    CreateOnCallNotificationRuleRequest, CreatePageRequest, CreateUserNotificationChannelRequest,
    EscalationPolicyCreateRequest, EscalationPolicyUpdateRequest, GetTeamMembershipsSort,
    RelationshipToUserTeamUser, RelationshipToUserTeamUserData, ScheduleCreateRequest,
    ScheduleUpdateRequest, TeamCreate, TeamCreateAttributes, TeamCreateRequest, TeamType,
    TeamUpdate, TeamUpdateAttributes, TeamUpdateRequest, UpdateOnCallNotificationRuleRequest,
    UserTeamAttributes, UserTeamCreate, UserTeamRelationships, UserTeamRequest, UserTeamRole,
    UserTeamType, UserTeamUpdate, UserTeamUpdateRequest, UserTeamUserType,
};

use crate::config::Config;
use crate::formatter;
use crate::raw_client;
use crate::util;
use crate::util_ext;

fn is_uuid(s: &str) -> bool {
    uuid::Uuid::parse_str(s).is_ok()
}

/// Resolve a team identifier that may be either a UUID or a team handle.
///
/// If `input` parses as a UUID it is returned as-is (fast path, no API call).
/// Otherwise, `ListTeams` is called with `filter[keyword]=<input>` and a single
/// page of size 100. The returned teams are filtered locally for exact
/// `attributes.handle == input` match; exactly one match returns `Ok(id)`.
///
/// Errors out (no silent inference):
///   - no team matches the keyword at all,
///   - substring matches exist but none has an exact handle,
///   - more than one team has an exact handle (defensive; API-side invariant).
///
/// Note: the 100-result ceiling is deliberate; we do not loop-paginate.
/// Handle collisions past page 1 will surface as "no exact match" rather than
/// hiding a real team; callers can still pass the UUID directly.
pub(crate) async fn resolve_team_id(cfg: &Config, input: &str) -> Result<String> {
    if is_uuid(input) {
        return Ok(input.to_string());
    }

    let api = crate::make_api!(TeamsAPI, cfg);
    let params = ListTeamsOptionalParams::default()
        .filter_keyword(input.to_string())
        .page_size(100);
    let resp = api
        .list_teams(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to resolve team handle '{input}': {e:?}"))?;

    let teams = resp.data.ok_or_else(|| {
        anyhow::anyhow!("unexpected response from teams API: 'data' field missing")
    })?;
    let total = teams.len();
    let exact: Vec<&datadog_api_client::datadogV2::model::Team> = teams
        .iter()
        .filter(|t| t.attributes.handle.to_lowercase() == input.to_lowercase())
        .collect();

    match exact.len() {
        1 => Ok(exact[0].id.clone()),
        0 if total == 0 => Err(anyhow::anyhow!("no team with handle '{input}'")),
        _ => Err(anyhow::anyhow!(
            "no exact handle match for '{input}' ({total} candidates matched substring)"
        )),
    }
}

fn parse_team_role(role: &str) -> Result<UserTeamRole> {
    // UserTeamRole is #[non_exhaustive] upstream — extend this match when new variants are added.
    match role.to_lowercase().as_str() {
        "admin" => Ok(UserTeamRole::ADMIN),
        other => anyhow::bail!("invalid --role value: {other:?}\nExpected: admin"),
    }
}

pub async fn teams_list(cfg: &Config) -> Result<()> {
    let api = crate::make_api!(TeamsAPI, cfg);
    let resp = api
        .list_teams(ListTeamsOptionalParams::default())
        .await
        .map_err(|e| anyhow::anyhow!("failed to list teams: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn teams_get(cfg: &Config, team_id: &str) -> Result<()> {
    let resolved = resolve_team_id(cfg, team_id).await?;
    let api = crate::make_api!(TeamsAPI, cfg);
    let resp = api
        .get_team(resolved)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get team: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn teams_delete(cfg: &Config, team_id: &str) -> Result<()> {
    let resolved = resolve_team_id(cfg, team_id).await?;
    let api = crate::make_api!(TeamsAPI, cfg);
    let msg = format!("Team '{resolved}' deleted successfully.");
    api.delete_team(resolved)
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete team: {e:?}"))?;
    println!("{msg}");
    Ok(())
}

pub async fn teams_create(cfg: &Config, name: &str, handle: &str) -> Result<()> {
    let api = crate::make_api!(TeamsAPI, cfg);
    let attrs = TeamCreateAttributes::new(handle.to_string(), name.to_string());
    let data = TeamCreate::new(attrs, TeamType::TEAM);
    let body = TeamCreateRequest::new(data);
    let resp = api
        .create_team(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create team: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn teams_update(cfg: &Config, team_id: &str, name: &str, handle: &str) -> Result<()> {
    let resolved = resolve_team_id(cfg, team_id).await?;
    let api = crate::make_api!(TeamsAPI, cfg);
    let attrs = TeamUpdateAttributes::new(handle.to_string(), name.to_string());
    let data = TeamUpdate::new(attrs, TeamType::TEAM);
    let body = TeamUpdateRequest::new(data);
    let resp = api
        .update_team(resolved, body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update team: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn memberships_list(
    cfg: &Config,
    team_id: &str,
    page_size: i64,
    page_number: i64,
    sort: String,
) -> Result<()> {
    let sort_val = match sort.as_str() {
        "manager_name" => GetTeamMembershipsSort::MANAGER_NAME,
        "-manager_name" => GetTeamMembershipsSort::_MANAGER_NAME,
        "name" => GetTeamMembershipsSort::NAME,
        "-name" => GetTeamMembershipsSort::_NAME,
        "handle" => GetTeamMembershipsSort::HANDLE,
        "-handle" => GetTeamMembershipsSort::_HANDLE,
        "email" => GetTeamMembershipsSort::EMAIL,
        "-email" => GetTeamMembershipsSort::_EMAIL,
        other => anyhow::bail!(
            "invalid --sort value: {other:?}\nExpected: name, -name, email, -email, handle, -handle, manager_name, -manager_name"
        ),
    };

    let resolved = resolve_team_id(cfg, team_id).await?;
    let api = crate::make_api!(TeamsAPI, cfg);

    let params = GetTeamMembershipsOptionalParams::default()
        .page_size(page_size)
        .page_number(page_number)
        .sort(sort_val);
    let resp = api
        .get_team_memberships(resolved, params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list memberships: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn memberships_add(
    cfg: &Config,
    team_id: &str,
    user_id: &str,
    role: Option<String>,
) -> Result<()> {
    let mut attrs = UserTeamAttributes::new();
    if let Some(r) = role {
        attrs = attrs.role(Some(parse_team_role(&r)?));
    }
    let resolved = resolve_team_id(cfg, team_id).await?;
    let api = crate::make_api!(TeamsAPI, cfg);
    let user_data =
        RelationshipToUserTeamUserData::new(user_id.to_string(), UserTeamUserType::USERS);
    let user_rel = RelationshipToUserTeamUser::new(user_data);
    let relationships = UserTeamRelationships::new().user(user_rel);
    let data = UserTeamCreate::new(UserTeamType::TEAM_MEMBERSHIPS)
        .attributes(attrs)
        .relationships(relationships);
    let body = UserTeamRequest::new(data);
    let resp = api
        .create_team_membership(resolved, body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to add membership: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn memberships_update(
    cfg: &Config,
    team_id: &str,
    user_id: &str,
    role: &str,
) -> Result<()> {
    let team_role = parse_team_role(role)?;
    let resolved = resolve_team_id(cfg, team_id).await?;
    let api = crate::make_api!(TeamsAPI, cfg);
    let attrs = UserTeamAttributes::new().role(Some(team_role));
    let data = UserTeamUpdate::new(UserTeamType::TEAM_MEMBERSHIPS).attributes(attrs);
    let body = UserTeamUpdateRequest::new(data);
    let resp = api
        .update_team_membership(resolved, user_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update membership: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn memberships_remove(cfg: &Config, team_id: &str, user_id: &str) -> Result<()> {
    let resolved = resolve_team_id(cfg, team_id).await?;
    let api = crate::make_api!(TeamsAPI, cfg);
    let msg = format!("Membership for user {user_id} removed from team {resolved}.");
    api.delete_team_membership(resolved, user_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to remove membership: {e:?}"))?;
    println!("{msg}");
    Ok(())
}

// ---- Escalation Policies ----

pub async fn escalation_policies_get(cfg: &Config, policy_id: &str) -> Result<()> {
    let api = crate::make_api!(OnCallAPI, cfg);
    let resp = api
        .get_on_call_escalation_policy(
            policy_id.to_string(),
            GetOnCallEscalationPolicyOptionalParams::default(),
        )
        .await
        .map_err(|e| anyhow::anyhow!("failed to get escalation policy: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn escalation_policies_create(cfg: &Config, file: &str) -> Result<()> {
    let api = crate::make_api!(OnCallAPI, cfg);
    let body: EscalationPolicyCreateRequest = util::read_json_file(file)?;
    let resp = api
        .create_on_call_escalation_policy(
            body,
            CreateOnCallEscalationPolicyOptionalParams::default(),
        )
        .await
        .map_err(|e| anyhow::anyhow!("failed to create escalation policy: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn escalation_policies_update(cfg: &Config, policy_id: &str, file: &str) -> Result<()> {
    let api = crate::make_api!(OnCallAPI, cfg);
    let body: EscalationPolicyUpdateRequest = util::read_json_file(file)?;
    let resp = api
        .update_on_call_escalation_policy(
            policy_id.to_string(),
            body,
            UpdateOnCallEscalationPolicyOptionalParams::default(),
        )
        .await
        .map_err(|e| anyhow::anyhow!("failed to update escalation policy: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn escalation_policies_delete(cfg: &Config, policy_id: &str) -> Result<()> {
    let api = crate::make_api!(OnCallAPI, cfg);
    api.delete_on_call_escalation_policy(policy_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete escalation policy: {e:?}"))?;
    println!("Escalation policy '{policy_id}' deleted successfully.");
    Ok(())
}

// ---- Schedules ----

pub async fn schedules_get(cfg: &Config, schedule_id: &str) -> Result<()> {
    let api = crate::make_api!(OnCallAPI, cfg);
    let resp = api
        .get_on_call_schedule(
            schedule_id.to_string(),
            GetOnCallScheduleOptionalParams::default(),
        )
        .await
        .map_err(|e| anyhow::anyhow!("failed to get schedule: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn schedules_create(cfg: &Config, file: &str) -> Result<()> {
    let api = crate::make_api!(OnCallAPI, cfg);
    let body: ScheduleCreateRequest = util::read_json_file(file)?;
    let resp = api
        .create_on_call_schedule(body, CreateOnCallScheduleOptionalParams::default())
        .await
        .map_err(|e| anyhow::anyhow!("failed to create schedule: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn schedules_update(cfg: &Config, schedule_id: &str, file: &str) -> Result<()> {
    let api = crate::make_api!(OnCallAPI, cfg);
    let body: ScheduleUpdateRequest = util::read_json_file(file)?;
    let resp = api
        .update_on_call_schedule(
            schedule_id.to_string(),
            body,
            UpdateOnCallScheduleOptionalParams::default(),
        )
        .await
        .map_err(|e| anyhow::anyhow!("failed to update schedule: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn schedules_delete(cfg: &Config, schedule_id: &str) -> Result<()> {
    let api = crate::make_api!(OnCallAPI, cfg);
    api.delete_on_call_schedule(schedule_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete schedule: {e:?}"))?;
    println!("Schedule '{schedule_id}' deleted successfully.");
    Ok(())
}

// ---- Notification Channels ----

pub async fn notification_channels_list(cfg: &Config, user_id: &str) -> Result<()> {
    let api = crate::make_api!(OnCallAPI, cfg);
    let resp = api
        .list_user_notification_channels(user_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to list notification channels: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn notification_channels_get(
    cfg: &Config,
    user_id: &str,
    channel_id: &str,
) -> Result<()> {
    let api = crate::make_api!(OnCallAPI, cfg);
    let resp = api
        .get_user_notification_channel(user_id.to_string(), channel_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get notification channel: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn notification_channels_create(cfg: &Config, user_id: &str, file: &str) -> Result<()> {
    let api = crate::make_api!(OnCallAPI, cfg);
    let body: CreateUserNotificationChannelRequest = util::read_json_file(file)?;
    let resp = api
        .create_user_notification_channel(user_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create notification channel: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn notification_channels_delete(
    cfg: &Config,
    user_id: &str,
    channel_id: &str,
) -> Result<()> {
    let api = crate::make_api!(OnCallAPI, cfg);
    api.delete_user_notification_channel(user_id.to_string(), channel_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete notification channel: {e:?}"))?;
    println!("Notification channel '{channel_id}' for user '{user_id}' deleted successfully.");
    Ok(())
}

// ---- Notification Rules ----

pub async fn notification_rules_list(cfg: &Config, user_id: &str) -> Result<()> {
    let api = crate::make_api!(OnCallAPI, cfg);
    let resp = api
        .list_user_notification_rules(
            user_id.to_string(),
            ListUserNotificationRulesOptionalParams::default(),
        )
        .await
        .map_err(|e| anyhow::anyhow!("failed to list notification rules: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn notification_rules_get(cfg: &Config, user_id: &str, rule_id: &str) -> Result<()> {
    let api = crate::make_api!(OnCallAPI, cfg);
    let resp = api
        .get_user_notification_rule(
            user_id.to_string(),
            rule_id.to_string(),
            GetUserNotificationRuleOptionalParams::default(),
        )
        .await
        .map_err(|e| anyhow::anyhow!("failed to get notification rule: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn notification_rules_create(cfg: &Config, user_id: &str, file: &str) -> Result<()> {
    let api = crate::make_api!(OnCallAPI, cfg);
    let body: CreateOnCallNotificationRuleRequest = util::read_json_file(file)?;
    let resp = api
        .create_user_notification_rule(user_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create notification rule: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn notification_rules_update(
    cfg: &Config,
    user_id: &str,
    rule_id: &str,
    file: &str,
) -> Result<()> {
    let api = crate::make_api!(OnCallAPI, cfg);
    let body: UpdateOnCallNotificationRuleRequest = util::read_json_file(file)?;
    let resp = api
        .update_user_notification_rule(
            user_id.to_string(),
            rule_id.to_string(),
            body,
            UpdateUserNotificationRuleOptionalParams::default(),
        )
        .await
        .map_err(|e| anyhow::anyhow!("failed to update notification rule: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn notification_rules_delete(cfg: &Config, user_id: &str, rule_id: &str) -> Result<()> {
    let api = crate::make_api!(OnCallAPI, cfg);
    api.delete_user_notification_rule(user_id.to_string(), rule_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete notification rule: {e:?}"))?;
    println!("Notification rule '{rule_id}' for user '{user_id}' deleted successfully.");
    Ok(())
}

// ---- Pages ----

pub async fn pages_create(cfg: &Config, file: &str) -> Result<()> {
    let api = crate::make_api!(OnCallPagingAPI, cfg);
    let body: CreatePageRequest = util::read_json_file(file)?;
    let resp = api
        .create_on_call_page(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create page: {e:?}"))?;
    formatter::output(cfg, &resp)
}

/// Fetches a single on-call page by ID.
///
/// Uses `raw_client::raw_get` because `datadog-api-client` does not yet
/// expose a `get_on_call_page` binding.
pub async fn pages_get(cfg: &Config, page_id: &str) -> Result<()> {
    let path = format!(
        "/api/v2/on-call/pages/{}",
        util_ext::percent_encode(page_id)
    );
    let resp = raw_client::raw_get(cfg, &path, &[])
        .await
        .map_err(|e| anyhow::anyhow!("failed to get page: {e:?}"))?;
    formatter::output(cfg, &resp)
}

/// Lists on-call pages, optionally filtered by team handle (server-side) and
/// responder user id (client-side).
///
/// Uses `raw_client::raw_get` against the unstable endpoint because
/// `datadog-api-client` exposes no list binding, and the stable v2 collection
/// endpoint currently returns an empty body.
pub async fn pages_list(
    cfg: &Config,
    team: Option<&str>,
    responder: Option<&str>,
    page_size: u32,
    sort: &str,
) -> Result<()> {
    if !(1..=1000).contains(&page_size) {
        anyhow::bail!("invalid page_size: {page_size}. Expected a value from 1 to 1000");
    }
    validate_pages_sort(sort)?;

    let page_size = page_size.to_string();
    let team_filter = team.map(|t| format!("team:{t}"));
    let mut query = vec![("page[size]", page_size.as_str()), ("sort", sort)];
    if let Some(filter) = team_filter.as_deref() {
        query.push(("filter", filter));
    }

    let mut resp = raw_client::raw_get(cfg, "/api/unstable/on-call/pages", &query)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list pages: {e:?}"))?;

    if let Some(responder) = responder {
        filter_pages_by_responder(&mut resp, responder);
    }

    formatter::output(cfg, &resp)
}

fn validate_pages_sort(sort: &str) -> Result<()> {
    match sort {
        "created_at" | "-created_at" => Ok(()),
        other => {
            anyhow::bail!("invalid --sort value: {other:?}\nExpected: created_at, -created_at")
        }
    }
}

fn filter_pages_by_responder(resp: &mut serde_json::Value, responder: &str) {
    if let Some(pages) = resp
        .get_mut("data")
        .and_then(serde_json::Value::as_array_mut)
    {
        pages.retain(|page| page_has_responder(page, responder));
    }
}

fn page_has_responder(page: &serde_json::Value, responder: &str) -> bool {
    page.get("relationships")
        .and_then(|relationships| relationships.get("responders"))
        .and_then(|responders| responders.get("data"))
        .and_then(serde_json::Value::as_array)
        .is_some_and(|responders| {
            responders.iter().any(|responder_ref| {
                responder_ref.get("id").and_then(serde_json::Value::as_str) == Some(responder)
            })
        })
}

#[cfg(test)]
mod tests {
    use crate::test_support::*;

    use super::*;

    #[test]
    fn test_is_uuid_accepts_canonical() {
        assert!(is_uuid("00000000-0000-0000-0000-000000000000"));
        assert!(is_uuid("abcdef01-2345-6789-abcd-ef0123456789"));
        // Uppercase hex is also valid.
        assert!(is_uuid("ABCDEF01-2345-6789-ABCD-EF0123456789"));
    }

    #[test]
    fn test_is_uuid_rejects_handle() {
        assert!(!is_uuid("example-team"));
        assert!(!is_uuid("team-handle-with-dashes"));
        assert!(!is_uuid(""));
    }

    #[test]
    fn test_is_uuid_rejects_wrong_length() {
        // Too short (last segment is 11 hex chars).
        assert!(!is_uuid("00000000-0000-0000-0000-00000000000"));
        // Too long (last segment is 13 hex chars).
        assert!(!is_uuid("00000000-0000-0000-0000-0000000000000"));
        // Non-hex character ('g').
        assert!(!is_uuid("g0000000-0000-0000-0000-000000000000"));
        // Missing dashes.
        assert!(!is_uuid("000000000000000000000000000000000000"));
    }

    #[tokio::test]
    async fn test_on_call_teams_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::teams_list(&cfg).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_on_call_teams_get() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": {}}"#).await;
        // Canonical UUID input takes the fast path in `resolve_team_id`, so
        // only the `get_team` endpoint needs a mock response.
        let _ = super::teams_get(&cfg, "00000000-0000-0000-0000-000000000000").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_on_call_teams_get_by_handle() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        // ListTeams keyword-filter response with one exactly-matching handle.
        let list_body = r#"{
            "data": [
                {
                    "id": "00000000-0000-0000-0000-000000000000",
                    "type": "team",
                    "attributes": {
                        "name": "Example Team",
                        "handle": "example-team",
                        "description": null,
                        "avatar": null,
                        "banner": null,
                        "visible_modules": null,
                        "hidden_modules": null,
                        "created_at": null,
                        "modified_at": null,
                        "summary": null,
                        "link_count": 0,
                        "user_count": 0,
                        "team_links": null
                    }
                }
            ]
        }"#;
        let get_body = r#"{
            "data": {
                "id": "00000000-0000-0000-0000-000000000000",
                "type": "team",
                "attributes": {
                    "name": "Example Team",
                    "handle": "example-team"
                }
            }
        }"#;
        // `mockito` picks the first matching mock; `Matcher::Any` on the path
        // means both GETs resolve here. We register two GET mocks; each mock
        // is consumed once by default, so ListTeams hits the first, GetTeam
        // the second.
        s.mock("GET", mockito::Matcher::Any)
            .match_query(mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(list_body)
            .create_async()
            .await;
        s.mock("GET", mockito::Matcher::Any)
            .match_query(mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(get_body)
            .create_async()
            .await;
        let result = super::teams_get(&cfg, "example-team").await;
        assert!(
            result.is_ok(),
            "teams_get by handle failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_on_call_teams_delete() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{}"#).await;
        let _ = super::teams_delete(&cfg, "t1").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_on_call_escalation_policies_get() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": {"type": "policies"}}"#).await;
        let result = super::escalation_policies_get(&cfg, "p1").await;
        assert!(
            result.is_ok(),
            "escalation policies get failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_on_call_escalation_policies_delete() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{}"#).await;
        let result = super::escalation_policies_delete(&cfg, "p1").await;
        assert!(
            result.is_ok(),
            "escalation policies delete failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_on_call_escalation_policies_get_error() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        s.mock("GET", mockito::Matcher::Any)
            .match_query(mockito::Matcher::Any)
            .with_status(500)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors": ["internal error"]}"#)
            .create_async()
            .await;
        let result = super::escalation_policies_get(&cfg, "p1").await;
        assert!(result.is_err(), "expected error on 500 response");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_on_call_schedules_get() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": {"type": "schedules"}}"#).await;
        let result = super::schedules_get(&cfg, "s1").await;
        assert!(result.is_ok(), "schedules get failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_on_call_schedules_delete() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{}"#).await;
        let result = super::schedules_delete(&cfg, "s1").await;
        assert!(
            result.is_ok(),
            "schedules delete failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_on_call_schedules_get_error() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        s.mock("GET", mockito::Matcher::Any)
            .match_query(mockito::Matcher::Any)
            .with_status(500)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors": ["internal error"]}"#)
            .create_async()
            .await;
        let result = super::schedules_get(&cfg, "s1").await;
        assert!(result.is_err(), "expected error on 500 response");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_on_call_notification_channels_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let result = super::notification_channels_list(&cfg, "u1").await;
        assert!(
            result.is_ok(),
            "notification channels list failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_on_call_notification_channels_get() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": {"type": "notification_channels"}}"#).await;
        let result = super::notification_channels_get(&cfg, "u1", "c1").await;
        assert!(
            result.is_ok(),
            "notification channels get failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_on_call_notification_channels_delete() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{}"#).await;
        let result = super::notification_channels_delete(&cfg, "u1", "c1").await;
        assert!(
            result.is_ok(),
            "notification channels delete failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_on_call_notification_channels_list_error() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        s.mock("GET", mockito::Matcher::Any)
            .match_query(mockito::Matcher::Any)
            .with_status(500)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors": ["internal error"]}"#)
            .create_async()
            .await;
        let result = super::notification_channels_list(&cfg, "u1").await;
        assert!(result.is_err(), "expected error on 500 response");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_on_call_notification_rules_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let result = super::notification_rules_list(&cfg, "u1").await;
        assert!(
            result.is_ok(),
            "notification rules list failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_on_call_notification_rules_get() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": {"type": "notification_rules"}}"#).await;
        let result = super::notification_rules_get(&cfg, "u1", "r1").await;
        assert!(
            result.is_ok(),
            "notification rules get failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_on_call_notification_rules_delete() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{}"#).await;
        let result = super::notification_rules_delete(&cfg, "u1", "r1").await;
        assert!(
            result.is_ok(),
            "notification rules delete failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_on_call_notification_rules_list_error() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        s.mock("GET", mockito::Matcher::Any)
            .match_query(mockito::Matcher::Any)
            .with_status(500)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors": ["internal error"]}"#)
            .create_async()
            .await;
        let result = super::notification_rules_list(&cfg, "u1").await;
        assert!(result.is_err(), "expected error on 500 response");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_on_call_pages_get() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        s.mock("GET", "/api/v2/on-call/pages/12345")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": {"id": "12345", "type": "pages"}}"#)
            .create_async()
            .await;
        let result = super::pages_get(&cfg, "12345").await;
        assert!(result.is_ok(), "pages_get failed: {:?}", result.err());
        cleanup_env();
    }

    // Regression test for #638: the on-call pages GET endpoint can return a 200
    // with an empty body (content-length: 0). pages_get must succeed instead of
    // failing with "EOF while parsing value at line 1 column 0".
    #[tokio::test]
    async fn test_on_call_pages_get_empty_body() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        s.mock("GET", "/api/v2/on-call/pages/12345")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("")
            .create_async()
            .await;
        let result = super::pages_get(&cfg, "12345").await;
        assert!(
            result.is_ok(),
            "pages_get with empty body failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_on_call_pages_get_not_found() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        s.mock("GET", "/api/v2/on-call/pages/missing")
            .with_status(404)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors": ["page not found"]}"#)
            .create_async()
            .await;
        let result = super::pages_get(&cfg, "missing").await;
        assert!(result.is_err(), "expected error on 404 response");
        cleanup_env();
    }

    // Uses an ID with reserved URL characters ('/' and '?') so the mock's
    // path-exact matcher only succeeds if `util_ext::percent_encode` is actually
    // applied. A refactor that drops the encoder would fail this test.
    #[tokio::test]
    async fn test_on_call_pages_get_percent_encodes_id() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        s.mock("GET", "/api/v2/on-call/pages/abc%2Fdef%3Fx")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": {"id": "abc/def?x", "type": "pages"}}"#)
            .create_async()
            .await;
        let result = super::pages_get(&cfg, "abc/def?x").await;
        assert!(result.is_ok(), "pages_get failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_on_call_pages_list_by_team() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let mock = s
            .mock("GET", "/api/unstable/on-call/pages")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("page[size]".into(), "42".into()),
                mockito::Matcher::UrlEncoded("sort".into(), "-created_at".into()),
                mockito::Matcher::UrlEncoded("filter".into(), "team:core-platform".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": []}"#)
            .create_async()
            .await;
        let result = super::pages_list(&cfg, Some("core-platform"), None, 42, "-created_at").await;
        assert!(result.is_ok(), "pages_list failed: {:?}", result.err());
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_on_call_pages_list_rejects_invalid_page_size() {
        let _lock = lock_env().await;
        let cfg = test_config("http://unused.local");
        let result = super::pages_list(&cfg, None, None, 0, "-created_at").await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid page_size"));
        cleanup_env();
    }

    #[tokio::test]
    async fn test_on_call_pages_list_rejects_invalid_sort() {
        let _lock = lock_env().await;
        let cfg = test_config("http://unused.local");
        let result = super::pages_list(&cfg, None, None, 100, "started_at").await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid --sort value"));
        cleanup_env();
    }

    #[test]
    fn test_page_has_responder_matches_and_rejects() {
        let page = serde_json::json!({
            "relationships": {
                "responders": {
                    "data": [
                        { "id": "user-1", "type": "users" },
                        { "id": "user-2", "type": "users" }
                    ]
                }
            }
        });

        assert!(super::page_has_responder(&page, "user-2"));
        assert!(!super::page_has_responder(&page, "user-3"));
    }

    #[test]
    fn test_filter_pages_by_responder() {
        let mut resp = serde_json::json!({
            "data": [
                {
                    "id": "page-1",
                    "relationships": {
                        "responders": {
                            "data": [{ "id": "user-1", "type": "users" }]
                        }
                    }
                },
                {
                    "id": "page-2",
                    "relationships": {
                        "responders": {
                            "data": [{ "id": "user-2", "type": "users" }]
                        }
                    }
                },
                { "id": "page-3" }
            ]
        });

        super::filter_pages_by_responder(&mut resp, "user-2");
        let pages = resp["data"].as_array().unwrap();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0]["id"], "page-2");
    }

    #[tokio::test]
    async fn test_memberships_list_invalid_sort() {
        let cfg = test_config("http://unused.local");
        let result = super::memberships_list(&cfg, "team-id", 10, 0, "bogus".into()).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid --sort value"));
    }

    #[tokio::test]
    async fn test_memberships_list_valid_sort() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let team_uuid = "00000000-0000-0000-0000-000000000001";
        mock_all(&mut s, r#"{"data": []}"#).await;
        let result = super::memberships_list(&cfg, team_uuid, 10, 0, "name".into()).await;
        assert!(
            result.is_ok(),
            "memberships_list failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_resolve_team_id_case_insensitive() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        // API returns handle in lowercase; caller passes mixed-case — must still resolve.
        let list_body = r#"{
            "data": [
                {
                    "id": "00000000-0000-0000-0000-000000000002",
                    "type": "team",
                    "attributes": {
                        "name": "Example Team",
                        "handle": "example-team",
                        "description": null,
                        "avatar": null,
                        "banner": null,
                        "visible_modules": null,
                        "hidden_modules": null,
                        "created_at": null,
                        "modified_at": null,
                        "summary": null,
                        "link_count": 0,
                        "user_count": 0,
                        "team_links": null
                    }
                }
            ]
        }"#;
        s.mock("GET", mockito::Matcher::Any)
            .match_query(mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(list_body)
            .create_async()
            .await;
        let result = super::resolve_team_id(&cfg, "Example-Team").await;
        assert!(
            result.is_ok(),
            "resolve_team_id case-insensitive failed: {:?}",
            result.err()
        );
        assert_eq!(result.unwrap(), "00000000-0000-0000-0000-000000000002");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_memberships_add_invalid_role() {
        let cfg = test_config("http://unused.local");
        let result =
            super::memberships_add(&cfg, "team-id", "user-id", Some("editor".into())).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid --role value"));
    }

    #[tokio::test]
    async fn test_memberships_update_invalid_role() {
        let cfg = test_config("http://unused.local");
        let result = super::memberships_update(&cfg, "team-id", "user-id", "editor").await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid --role value"));
    }
}
