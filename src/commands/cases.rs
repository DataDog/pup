use anyhow::Result;
use datadog_api_client::datadogV2::api_case_management::{
    CaseManagementAPI, CountCasesOptionalParams, ListCaseLinksOptionalParams,
    SearchCasesOptionalParams,
};
use datadog_api_client::datadogV2::api_case_management_attribute::CaseManagementAttributeAPI;
use datadog_api_client::datadogV2::api_case_management_type::CaseManagementTypeAPI;
use datadog_api_client::datadogV2::model::{
    AutomationRuleCreateRequest, AutomationRuleUpdateRequest, CaseAggregateRequest, CaseAssign,
    CaseAssignAttributes, CaseAssignRequest, CaseBulkUpdateRequest, CaseComment,
    CaseCommentAttributes, CaseCommentRequest, CaseCreate, CaseCreateAttributes,
    CaseCreateRelationships, CaseCreateRequest, CaseEmpty, CaseEmptyRequest, CaseInsightsRequest,
    CaseLinkAttributes, CaseLinkCreate, CaseLinkCreateRequest, CaseLinkResourceType,
    CaseNotificationRuleCreateRequest, CaseNotificationRuleUpdateRequest, CasePriority,
    CaseResourceType, CaseStatus, CaseTypeCreateRequest, CaseTypeUpdateRequest,
    CaseUpdateAttributesRequest, CaseUpdateCustomAttributeRequest, CaseUpdateDescription,
    CaseUpdateDescriptionAttributes, CaseUpdateDescriptionRequest, CaseUpdateDueDate,
    CaseUpdateDueDateAttributes, CaseUpdateDueDateRequest, CaseUpdatePriority,
    CaseUpdatePriorityAttributes, CaseUpdatePriorityRequest, CaseUpdateResolvedReason,
    CaseUpdateResolvedReasonAttributes, CaseUpdateResolvedReasonRequest, CaseUpdateStatus,
    CaseUpdateStatusAttributes, CaseUpdateStatusRequest, CaseUpdateTitle,
    CaseUpdateTitleAttributes, CaseUpdateTitleRequest, CaseViewCreateRequest,
    CaseViewUpdateRequest, CustomAttributeConfigCreateRequest, CustomAttributeConfigUpdateRequest,
    IncidentRelationshipData, IncidentResourceType, JiraIssueCreateRequest, JiraIssueLinkRequest,
    NotebookCreateRequest, ProjectCreate, ProjectCreateAttributes, ProjectCreateRequest,
    ProjectRelationship, ProjectRelationshipData, ProjectResourceType, ProjectUpdateRequest,
    RelationshipToIncidentRequest, ServiceNowTicketCreateRequest,
};

use crate::config::Config;
use crate::formatter;
use crate::raw_client;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse a priority string into a `CasePriority`.
///
/// Accepts the canonical names (P1..P5, NOT_DEFINED), case-insensitive.
fn parse_priority(s: &str) -> Result<CasePriority> {
    Ok(match s.to_uppercase().as_str() {
        "P1" => CasePriority::P1,
        "P2" => CasePriority::P2,
        "P3" => CasePriority::P3,
        "P4" => CasePriority::P4,
        "P5" => CasePriority::P5,
        "NOT_DEFINED" => CasePriority::NOT_DEFINED,
        _ => anyhow::bail!("invalid priority: {s} (use P1, P2, P3, P4, P5, NOT_DEFINED)"),
    })
}

// ---------------------------------------------------------------------------
// Helper: build a CaseManagementAPI with bearer-token support
// ---------------------------------------------------------------------------

fn make_api(cfg: &Config) -> CaseManagementAPI {
    crate::make_api!(CaseManagementAPI, cfg)
}

// ---------------------------------------------------------------------------
// Core case operations
// ---------------------------------------------------------------------------

pub async fn search(
    cfg: &Config,
    query: Option<String>,
    page_size: i64,
    page_number: i64,
) -> Result<()> {
    let api = make_api(cfg);
    let mut params = SearchCasesOptionalParams::default()
        .page_size(page_size)
        .page_number(page_number);
    if let Some(q) = query {
        params = params.filter(q);
    }
    let resp = api
        .search_cases(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to search cases: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn get(cfg: &Config, case_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let resp = api
        .get_case(case_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get case: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn create(cfg: &Config, file: &str) -> Result<()> {
    let api = make_api(cfg);
    let body: CaseCreateRequest = crate::util::read_json_file(file)?;
    let resp = api
        .create_case(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create case: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn create_from_flags(
    cfg: &Config,
    title: &str,
    type_id: &str,
    priority: &str,
    description: Option<&str>,
    project_id: Option<&str>,
) -> Result<()> {
    let api = make_api(cfg);
    let priority_val = parse_priority(priority)?;
    let mut attrs =
        CaseCreateAttributes::new(title.to_string(), type_id.to_string()).priority(priority_val);
    if let Some(desc) = description {
        attrs = attrs.description(desc.to_string());
    }
    let mut case_create = CaseCreate::new(attrs, CaseResourceType::CASE);
    if let Some(pid) = project_id {
        case_create =
            case_create.relationships(CaseCreateRelationships::new(ProjectRelationship::new(
                ProjectRelationshipData::new(pid.to_string(), ProjectResourceType::PROJECT),
            )));
    }
    let body = CaseCreateRequest::new(case_create);
    let resp = api
        .create_case(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create case: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---------------------------------------------------------------------------
// Comments
// ---------------------------------------------------------------------------

pub async fn comments_create(cfg: &Config, case_id: &str, body: &str) -> Result<()> {
    let api = make_api(cfg);
    let req = CaseCommentRequest::new(CaseComment::new(
        CaseCommentAttributes::new(body.to_string()),
        CaseResourceType::CASE,
    ));
    let resp = api
        .comment_case(case_id.to_string(), req)
        .await
        .map_err(|e| anyhow::anyhow!("failed to add comment: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn comments_delete(cfg: &Config, case_id: &str, comment_id: &str) -> Result<()> {
    let api = make_api(cfg);
    api.delete_case_comment(case_id.to_string(), comment_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete comment: {e:?}"))?;
    println!("Comment {comment_id} deleted from case {case_id}.");
    Ok(())
}

/// Update a comment's body. The endpoint is `PUT /api/v2/cases/{case_id}/
/// comment/{cell_id}` — not yet in the public spec. Responds 200 with an
/// empty body on success, so we use `raw_request` rather than `raw_put`
/// (which expects parseable JSON).
pub async fn comments_update(
    cfg: &Config,
    case_id: &str,
    comment_id: &str,
    body: &str,
) -> Result<()> {
    let path = format!("/api/v2/cases/{case_id}/comment/{comment_id}");
    let payload = serde_json::json!({
        "data": {
            "type": "case",
            "attributes": { "comment": body },
        }
    });
    let payload_bytes = serde_json::to_vec(&payload)?;
    raw_client::raw_request(
        cfg,
        "PUT",
        &path,
        &[],
        Some(payload_bytes),
        Some("application/json"),
        "application/json",
        &[],
    )
    .await
    .map_err(|e| anyhow::anyhow!("failed to update comment: {e:?}"))?;
    println!("Comment {comment_id} updated on case {case_id}.");
    Ok(())
}

/// List a case's comments by filtering the timeline to `COMMENT` cells.
pub async fn comments_list(cfg: &Config, case_id: &str) -> Result<()> {
    let resp = fetch_timeline(cfg, case_id).await?;
    let comments = comment_cells(&resp);
    formatter::output(cfg, &serde_json::json!({ "data": comments }))
}

/// Get a single comment by id by filtering the timeline. Errors if no cell
/// with the given id exists — matches the typical `GET /resource/{id}` shape.
pub async fn comments_get(cfg: &Config, case_id: &str, comment_id: &str) -> Result<()> {
    let resp = fetch_timeline(cfg, case_id).await?;
    let found = comment_cells(&resp)
        .into_iter()
        .find(|c| c.get("id").and_then(|v| v.as_str()) == Some(comment_id))
        .ok_or_else(|| {
            anyhow::anyhow!("comment {comment_id} not found in timeline for case {case_id}")
        })?;
    formatter::output(cfg, &serde_json::json!({ "data": found }))
}

// ---------------------------------------------------------------------------
// Timeline (uses raw HTTP — the DD API client doesn't expose this endpoint
// yet, even though it declares the response model).
// ---------------------------------------------------------------------------

/// Fetch the full timeline for a case. Returns every cell (comments,
/// attribute updates, status changes, etc.).
pub async fn timeline(cfg: &Config, case_id: &str) -> Result<()> {
    let resp = fetch_timeline(cfg, case_id).await?;
    formatter::output(cfg, &resp)
}

async fn fetch_timeline(cfg: &Config, case_id: &str) -> Result<serde_json::Value> {
    let path = format!("/api/v2/cases/{case_id}/timelines");
    raw_client::raw_get(cfg, &path, &[])
        .await
        .map_err(|e| anyhow::anyhow!("failed to get case timeline: {e:?}"))
}

/// Return all `COMMENT` cells from a timeline response, preserving order.
fn comment_cells(resp: &serde_json::Value) -> Vec<serde_json::Value> {
    resp.get("data")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|cell| {
                    cell.get("attributes")
                        .and_then(|a| a.get("type"))
                        .and_then(|t| t.as_str())
                        == Some("COMMENT")
                })
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Projects
// ---------------------------------------------------------------------------

pub async fn projects_list(cfg: &Config) -> Result<()> {
    let api = make_api(cfg);
    let resp = api
        .get_projects()
        .await
        .map_err(|e| anyhow::anyhow!("failed to list projects: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn projects_get(cfg: &Config, project_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let resp = api
        .get_project(project_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get project: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn projects_delete(cfg: &Config, project_id: &str) -> Result<()> {
    let api = make_api(cfg);
    api.delete_project(project_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete project: {e:?}"))?;
    println!("Project {project_id} deleted.");
    Ok(())
}

pub async fn projects_create(cfg: &Config, name: &str, key: &str) -> Result<()> {
    let api = make_api(cfg);
    let body = ProjectCreateRequest::new(ProjectCreate::new(
        ProjectCreateAttributes::new(key.to_string(), name.to_string()),
        ProjectResourceType::PROJECT,
    ));
    let resp = api
        .create_project(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create project: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---------------------------------------------------------------------------
// Archive / Unarchive
// ---------------------------------------------------------------------------

pub async fn archive(cfg: &Config, case_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let data = CaseEmpty::new(CaseResourceType::CASE);
    let body = CaseEmptyRequest::new(data);
    let resp = api
        .archive_case(case_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to archive case: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn unarchive(cfg: &Config, case_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let data = CaseEmpty::new(CaseResourceType::CASE);
    let body = CaseEmptyRequest::new(data);
    let resp = api
        .unarchive_case(case_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to unarchive case: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---------------------------------------------------------------------------
// Assign / Update priority / Update status
// ---------------------------------------------------------------------------

pub async fn assign(cfg: &Config, case_id: &str, user_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let body = CaseAssignRequest::new(CaseAssign::new(
        CaseAssignAttributes::new(user_id.to_string()),
        CaseResourceType::CASE,
    ));
    let resp = api
        .assign_case(case_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to assign case: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn update_priority(cfg: &Config, case_id: &str, priority: &str) -> Result<()> {
    let api = make_api(cfg);
    let priority_val = parse_priority(priority)?;
    let body = CaseUpdatePriorityRequest::new(CaseUpdatePriority::new(
        CaseUpdatePriorityAttributes::new(priority_val),
        CaseResourceType::CASE,
    ));
    let resp = api
        .update_priority(case_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update case priority: {e:?}"))?;
    formatter::output(cfg, &resp)
}

#[allow(deprecated)]
pub async fn update_status(cfg: &Config, case_id: &str, status: &str) -> Result<()> {
    let api = make_api(cfg);
    let status_val = match status.to_uppercase().as_str() {
        "OPEN" => CaseStatus::OPEN,
        "IN_PROGRESS" => CaseStatus::IN_PROGRESS,
        "CLOSED" => CaseStatus::CLOSED,
        _ => anyhow::bail!("invalid status: {status} (use OPEN, IN_PROGRESS, CLOSED)"),
    };
    let body = CaseUpdateStatusRequest::new(CaseUpdateStatus::new(
        CaseUpdateStatusAttributes::new().status(status_val),
        CaseResourceType::CASE,
    ));
    let resp = api
        .update_status(case_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update case status: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---------------------------------------------------------------------------
// Jira integration
// ---------------------------------------------------------------------------

pub async fn jira_create_issue(cfg: &Config, case_id: &str, file: &str) -> Result<()> {
    let api = make_api(cfg);
    let body: JiraIssueCreateRequest = crate::util::read_json_file(file)?;
    api.create_case_jira_issue(case_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create Jira issue for case: {e:?}"))?;
    println!("Jira issue created for case '{case_id}'.");
    Ok(())
}

pub async fn jira_link(cfg: &Config, case_id: &str, file: &str) -> Result<()> {
    let api = make_api(cfg);
    let body: JiraIssueLinkRequest = crate::util::read_json_file(file)?;
    api.link_jira_issue_to_case(case_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to link Jira issue to case: {e:?}"))?;
    println!("Jira issue linked to case '{case_id}'.");
    Ok(())
}

pub async fn jira_unlink(cfg: &Config, case_id: &str) -> Result<()> {
    let api = make_api(cfg);
    api.unlink_jira_issue(case_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to unlink Jira issue from case: {e:?}"))?;
    println!("Jira issue unlinked from case '{case_id}'.");
    Ok(())
}

// ---------------------------------------------------------------------------
// ServiceNow integration
// ---------------------------------------------------------------------------

pub async fn servicenow_create_ticket(cfg: &Config, case_id: &str, file: &str) -> Result<()> {
    let api = make_api(cfg);
    let body: ServiceNowTicketCreateRequest = crate::util::read_json_file(file)?;
    api.create_case_service_now_ticket(case_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create ServiceNow ticket for case: {e:?}"))?;
    println!("ServiceNow ticket created for case '{case_id}'.");
    Ok(())
}

// ---------------------------------------------------------------------------
// Projects notification rules
// ---------------------------------------------------------------------------

pub async fn projects_notification_rules_list(cfg: &Config, project_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let resp = api
        .get_project_notification_rules(project_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to list notification rules: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn projects_notification_rules_create(
    cfg: &Config,
    project_id: &str,
    file: &str,
) -> Result<()> {
    let api = make_api(cfg);
    let body: CaseNotificationRuleCreateRequest = crate::util::read_json_file(file)?;
    let resp = api
        .create_project_notification_rule(project_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create notification rule: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn projects_notification_rules_update(
    cfg: &Config,
    project_id: &str,
    rule_id: &str,
    file: &str,
) -> Result<()> {
    let api = make_api(cfg);
    let body: CaseNotificationRuleUpdateRequest = crate::util::read_json_file(file)?;
    api.update_project_notification_rule(project_id.to_string(), rule_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update notification rule: {e:?}"))?;
    println!("Notification rule '{rule_id}' updated.");
    Ok(())
}

pub async fn projects_notification_rules_delete(
    cfg: &Config,
    project_id: &str,
    rule_id: &str,
) -> Result<()> {
    let api = make_api(cfg);
    api.delete_project_notification_rule(project_id.to_string(), rule_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete notification rule: {e:?}"))?;
    println!("Notification rule '{rule_id}' deleted.");
    Ok(())
}

// ---------------------------------------------------------------------------
// Move case to project
// ---------------------------------------------------------------------------

pub async fn move_to_project(cfg: &Config, case_id: &str, project_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let data = ProjectRelationshipData::new(project_id.to_string(), ProjectResourceType::PROJECT);
    let body = ProjectRelationship::new(data);
    let resp = api
        .move_case_to_project(case_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to move case to project: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---------------------------------------------------------------------------
// Update case title
// ---------------------------------------------------------------------------

pub async fn update_title(cfg: &Config, case_id: &str, title: &str) -> Result<()> {
    let api = make_api(cfg);
    let attrs = CaseUpdateTitleAttributes::new(title.to_string());
    let data = CaseUpdateTitle::new(attrs, CaseResourceType::CASE);
    let body = CaseUpdateTitleRequest::new(data);
    let resp = api
        .update_case_title(case_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update case title: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---------------------------------------------------------------------------
// Update case description
// ---------------------------------------------------------------------------

pub async fn update_description(cfg: &Config, case_id: &str, description: &str) -> Result<()> {
    let api = make_api(cfg);
    let attrs = CaseUpdateDescriptionAttributes::new(description.to_string());
    let data = CaseUpdateDescription::new(attrs, CaseResourceType::CASE);
    let body = CaseUpdateDescriptionRequest::new(data);
    let resp = api
        .update_case_description(case_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update case description: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---------------------------------------------------------------------------
// Update project
// ---------------------------------------------------------------------------

pub async fn projects_update(cfg: &Config, project_id: &str, file: &str) -> Result<()> {
    let api = make_api(cfg);
    let body: ProjectUpdateRequest = crate::util::read_json_file(file)?;
    let resp = api
        .update_project(project_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update project: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---------------------------------------------------------------------------
// Count / Aggregate / Bulk update
// ---------------------------------------------------------------------------

pub async fn count(
    cfg: &Config,
    query: Option<String>,
    group_by: Option<String>,
    limit: Option<i64>,
) -> Result<()> {
    let api = make_api(cfg);
    let mut params = CountCasesOptionalParams::default();
    if let Some(q) = query {
        params = params.query_filter(q);
    }
    if let Some(g) = group_by {
        params = params.group_bys(g);
    }
    if let Some(l) = limit {
        params = params.limit(l);
    }
    let resp = api
        .count_cases(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to count cases: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn aggregate(cfg: &Config, file: &str) -> Result<()> {
    let api = make_api(cfg);
    let body: CaseAggregateRequest = crate::util::read_json_file(file)?;
    let resp = api
        .aggregate_cases(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to aggregate cases: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn bulk_update(cfg: &Config, file: &str) -> Result<()> {
    let api = make_api(cfg);
    let body: CaseBulkUpdateRequest = crate::util::read_json_file(file)?;
    api.bulk_update_cases(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to bulk update cases: {e:?}"))?;
    println!("Cases updated.");
    Ok(())
}

// ---------------------------------------------------------------------------
// Unassign / Attributes / Due date / Resolved reason / Custom attributes
// ---------------------------------------------------------------------------

pub async fn unassign(cfg: &Config, case_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let body = CaseEmptyRequest::new(CaseEmpty::new(CaseResourceType::CASE));
    let resp = api
        .unassign_case(case_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to unassign case: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn update_attributes(cfg: &Config, case_id: &str, file: &str) -> Result<()> {
    let api = make_api(cfg);
    let body: CaseUpdateAttributesRequest = crate::util::read_json_file(file)?;
    let resp = api
        .update_attributes(case_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update case attributes: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn update_due_date(cfg: &Config, case_id: &str, due_date: &str) -> Result<()> {
    let api = make_api(cfg);
    let body = CaseUpdateDueDateRequest::new(CaseUpdateDueDate::new(
        CaseUpdateDueDateAttributes::new(due_date.to_string()),
        CaseResourceType::CASE,
    ));
    let resp = api
        .update_case_due_date(case_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update case due date: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn update_resolved_reason(cfg: &Config, case_id: &str, reason: &str) -> Result<()> {
    let api = make_api(cfg);
    let body = CaseUpdateResolvedReasonRequest::new(CaseUpdateResolvedReason::new(
        CaseUpdateResolvedReasonAttributes::new(reason.to_string()),
        CaseResourceType::CASE,
    ));
    let resp = api
        .update_case_resolved_reason(case_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update case resolved reason: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn update_custom_attribute(
    cfg: &Config,
    case_id: &str,
    key: &str,
    file: &str,
) -> Result<()> {
    let api = make_api(cfg);
    let body: CaseUpdateCustomAttributeRequest = crate::util::read_json_file(file)?;
    let resp = api
        .update_case_custom_attribute(case_id.to_string(), key.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update case custom attribute: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn delete_custom_attribute(cfg: &Config, case_id: &str, key: &str) -> Result<()> {
    let api = make_api(cfg);
    let resp = api
        .delete_case_custom_attribute(case_id.to_string(), key.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete case custom attribute: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---------------------------------------------------------------------------
// Watchers
// ---------------------------------------------------------------------------

pub async fn watchers_list(cfg: &Config, case_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let resp = api
        .list_case_watchers(case_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to list case watchers: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn watchers_add(cfg: &Config, case_id: &str, user_id: &str) -> Result<()> {
    let api = make_api(cfg);
    api.watch_case(case_id.to_string(), user_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to add case watcher: {e:?}"))?;
    println!("User {user_id} is now watching case {case_id}.");
    Ok(())
}

pub async fn watchers_remove(cfg: &Config, case_id: &str, user_id: &str) -> Result<()> {
    let api = make_api(cfg);
    api.unwatch_case(case_id.to_string(), user_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to remove case watcher: {e:?}"))?;
    println!("User {user_id} is no longer watching case {case_id}.");
    Ok(())
}

// ---------------------------------------------------------------------------
// Insights
// ---------------------------------------------------------------------------

pub async fn insights_add(cfg: &Config, case_id: &str, file: &str) -> Result<()> {
    let api = make_api(cfg);
    let body: CaseInsightsRequest = crate::util::read_json_file(file)?;
    let resp = api
        .add_case_insights(case_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to add case insights: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn insights_remove(cfg: &Config, case_id: &str, file: &str) -> Result<()> {
    let api = make_api(cfg);
    let body: CaseInsightsRequest = crate::util::read_json_file(file)?;
    let resp = api
        .remove_case_insights(case_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to remove case insights: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---------------------------------------------------------------------------
// Incident / Notebook relationships
// ---------------------------------------------------------------------------

pub async fn incidents_link(cfg: &Config, case_id: &str, incident_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let body = RelationshipToIncidentRequest::new(IncidentRelationshipData::new(
        incident_id.to_string(),
        IncidentResourceType::INCIDENTS,
    ));
    let resp = api
        .link_incident(case_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to link incident to case: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn notebooks_create(cfg: &Config, case_id: &str, file: &str) -> Result<()> {
    let api = make_api(cfg);
    let body: NotebookCreateRequest = crate::util::read_json_file(file)?;
    api.create_case_notebook(case_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create notebook for case: {e:?}"))?;
    println!("Notebook created for case '{case_id}'.");
    Ok(())
}

// ---------------------------------------------------------------------------
// Links
// ---------------------------------------------------------------------------

pub async fn links_list(
    cfg: &Config,
    entity_type: &str,
    entity_id: &str,
    relationship: Option<String>,
) -> Result<()> {
    let api = make_api(cfg);
    let mut params = ListCaseLinksOptionalParams::default();
    if let Some(r) = relationship {
        params = params.relationship(r);
    }
    let resp = api
        .list_case_links(entity_type.to_string(), entity_id.to_string(), params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list case links: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn links_create(
    cfg: &Config,
    parent_type: &str,
    parent_id: &str,
    child_type: &str,
    child_id: &str,
    relationship: &str,
) -> Result<()> {
    let api = make_api(cfg);
    let attrs = CaseLinkAttributes::new(
        child_id.to_string(),
        child_type.to_string(),
        parent_id.to_string(),
        parent_type.to_string(),
        relationship.to_string(),
    );
    let body = CaseLinkCreateRequest::new(CaseLinkCreate::new(attrs, CaseLinkResourceType::LINK));
    let resp = api
        .create_case_link(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create case link: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn links_delete(cfg: &Config, link_id: &str) -> Result<()> {
    let api = make_api(cfg);
    api.delete_case_link(link_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete case link: {e:?}"))?;
    println!("Case link {link_id} deleted.");
    Ok(())
}

// ---------------------------------------------------------------------------
// Views
// ---------------------------------------------------------------------------

pub async fn views_list(cfg: &Config, project_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let resp = api
        .list_case_views(project_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to list case views: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn views_get(cfg: &Config, view_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let resp = api
        .get_case_view(view_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get case view: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn views_create(cfg: &Config, file: &str) -> Result<()> {
    let api = make_api(cfg);
    let body: CaseViewCreateRequest = crate::util::read_json_file(file)?;
    let resp = api
        .create_case_view(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create case view: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn views_update(cfg: &Config, view_id: &str, file: &str) -> Result<()> {
    let api = make_api(cfg);
    let body: CaseViewUpdateRequest = crate::util::read_json_file(file)?;
    let resp = api
        .update_case_view(view_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update case view: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn views_delete(cfg: &Config, view_id: &str) -> Result<()> {
    let api = make_api(cfg);
    api.delete_case_view(view_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete case view: {e:?}"))?;
    println!("Case view {view_id} deleted.");
    Ok(())
}

// ---------------------------------------------------------------------------
// Project favorites
// ---------------------------------------------------------------------------

pub async fn projects_favorites_list(cfg: &Config) -> Result<()> {
    let api = make_api(cfg);
    let resp = api
        .list_user_case_project_favorites()
        .await
        .map_err(|e| anyhow::anyhow!("failed to list favorite projects: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn projects_favorites_add(cfg: &Config, project_id: &str) -> Result<()> {
    let api = make_api(cfg);
    api.favorite_case_project(project_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to favorite project: {e:?}"))?;
    println!("Project {project_id} added to favorites.");
    Ok(())
}

pub async fn projects_favorites_remove(cfg: &Config, project_id: &str) -> Result<()> {
    let api = make_api(cfg);
    api.unfavorite_case_project(project_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to unfavorite project: {e:?}"))?;
    println!("Project {project_id} removed from favorites.");
    Ok(())
}

// ---------------------------------------------------------------------------
// Project automation rules
// ---------------------------------------------------------------------------

pub async fn projects_automation_rules_list(cfg: &Config, project_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let resp = api
        .list_case_automation_rules(project_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to list automation rules: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn projects_automation_rules_get(
    cfg: &Config,
    project_id: &str,
    rule_id: &str,
) -> Result<()> {
    let api = make_api(cfg);
    let resp = api
        .get_case_automation_rule(project_id.to_string(), rule_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get automation rule: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn projects_automation_rules_create(
    cfg: &Config,
    project_id: &str,
    file: &str,
) -> Result<()> {
    let api = make_api(cfg);
    let body: AutomationRuleCreateRequest = crate::util::read_json_file(file)?;
    let resp = api
        .create_case_automation_rule(project_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create automation rule: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn projects_automation_rules_update(
    cfg: &Config,
    project_id: &str,
    rule_id: &str,
    file: &str,
) -> Result<()> {
    let api = make_api(cfg);
    let body: AutomationRuleUpdateRequest = crate::util::read_json_file(file)?;
    let resp = api
        .update_case_automation_rule(project_id.to_string(), rule_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update automation rule: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn projects_automation_rules_delete(
    cfg: &Config,
    project_id: &str,
    rule_id: &str,
) -> Result<()> {
    let api = make_api(cfg);
    api.delete_case_automation_rule(project_id.to_string(), rule_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete automation rule: {e:?}"))?;
    println!("Automation rule '{rule_id}' deleted.");
    Ok(())
}

pub async fn projects_automation_rules_enable(
    cfg: &Config,
    project_id: &str,
    rule_id: &str,
) -> Result<()> {
    let api = make_api(cfg);
    let resp = api
        .enable_case_automation_rule(project_id.to_string(), rule_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to enable automation rule: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn projects_automation_rules_disable(
    cfg: &Config,
    project_id: &str,
    rule_id: &str,
) -> Result<()> {
    let api = make_api(cfg);
    let resp = api
        .disable_case_automation_rule(project_id.to_string(), rule_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to disable automation rule: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---------------------------------------------------------------------------
// Case types
// ---------------------------------------------------------------------------

fn make_type_api(cfg: &Config) -> CaseManagementTypeAPI {
    crate::make_api!(CaseManagementTypeAPI, cfg)
}

pub async fn types_list(cfg: &Config) -> Result<()> {
    let api = make_type_api(cfg);
    let resp = api
        .get_all_case_types()
        .await
        .map_err(|e| anyhow::anyhow!("failed to list case types: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn types_create(cfg: &Config, file: &str) -> Result<()> {
    let api = make_type_api(cfg);
    let body: CaseTypeCreateRequest = crate::util::read_json_file(file)?;
    let resp = api
        .create_case_type(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create case type: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn types_update(cfg: &Config, type_id: &str, file: &str) -> Result<()> {
    let api = make_type_api(cfg);
    let body: CaseTypeUpdateRequest = crate::util::read_json_file(file)?;
    let resp = api
        .update_case_type(type_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update case type: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn types_delete(cfg: &Config, type_id: &str) -> Result<()> {
    let api = make_type_api(cfg);
    api.delete_case_type(type_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete case type: {e:?}"))?;
    println!("Case type {type_id} deleted.");
    Ok(())
}

// ---------------------------------------------------------------------------
// Custom attribute configs
// ---------------------------------------------------------------------------

fn make_attribute_api(cfg: &Config) -> CaseManagementAttributeAPI {
    crate::make_api!(CaseManagementAttributeAPI, cfg)
}

pub async fn custom_attributes_list(cfg: &Config, type_id: Option<&str>) -> Result<()> {
    let api = make_attribute_api(cfg);
    let resp = match type_id {
        Some(t) => api
            .get_all_custom_attribute_configs_by_case_type(t.to_string())
            .await
            .map_err(|e| anyhow::anyhow!("failed to list custom attributes: {e:?}"))?,
        None => api
            .get_all_custom_attributes()
            .await
            .map_err(|e| anyhow::anyhow!("failed to list custom attributes: {e:?}"))?,
    };
    formatter::output(cfg, &resp)
}

pub async fn custom_attributes_create(cfg: &Config, type_id: &str, file: &str) -> Result<()> {
    let api = make_attribute_api(cfg);
    let body: CustomAttributeConfigCreateRequest = crate::util::read_json_file(file)?;
    let resp = api
        .create_custom_attribute_config(type_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create custom attribute: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn custom_attributes_update(
    cfg: &Config,
    type_id: &str,
    attribute_id: &str,
    file: &str,
) -> Result<()> {
    let api = make_attribute_api(cfg);
    let body: CustomAttributeConfigUpdateRequest = crate::util::read_json_file(file)?;
    let resp = api
        .update_custom_attribute_config(type_id.to_string(), attribute_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update custom attribute: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn custom_attributes_delete(
    cfg: &Config,
    type_id: &str,
    attribute_id: &str,
) -> Result<()> {
    let api = make_attribute_api(cfg);
    api.delete_custom_attribute_config(type_id.to_string(), attribute_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete custom attribute: {e:?}"))?;
    println!("Custom attribute {attribute_id} deleted.");
    Ok(())
}

#[cfg(test)]
mod tests {

    use crate::test_support::*;

    #[tokio::test]
    async fn test_cases_search() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::search(&cfg, None, 10, 0).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_cases_get() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": {}}"#).await;
        let _ = super::get(&cfg, "case1").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_cases_projects_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::projects_list(&cfg).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_cases_projects_get() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": {}}"#).await;
        let _ = super::projects_get(&cfg, "proj1").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_cases_projects_delete() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{}"#).await;
        let _ = super::projects_delete(&cfg, "proj1").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_cases_create_from_flags() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": {}}"#).await;
        let _ = super::create_from_flags(
            &cfg,
            "title",
            "00000000-0000-0000-0000-000000000001",
            "P2",
            Some("description"),
            None,
        )
        .await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_cases_create_from_flags_with_project_id() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": {}}"#).await;
        let _ = super::create_from_flags(
            &cfg,
            "title",
            "00000000-0000-0000-0000-000000000001",
            "NOT_DEFINED",
            None,
            Some("d59d89e1-b144-47e2-ae70-179203edf0ba"),
        )
        .await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_cases_create_from_flags_invalid_priority() {
        let _lock = lock_env().await;
        let cfg = test_config("http://nowhere.invalid");
        let result = super::create_from_flags(
            &cfg,
            "title",
            "00000000-0000-0000-0000-000000000001",
            "P9",
            None,
            None,
        )
        .await;
        cleanup_env();
        assert!(result.is_err(), "expected error for invalid priority P9");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("invalid priority"),
            "expected 'invalid priority' in error, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_cases_comments_create() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": {}}"#).await;
        let _ = super::comments_create(&cfg, "case1", "hello").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_cases_update_description() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": {}}"#).await;
        let _ = super::update_description(&cfg, "case1", "new description").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_cases_comments_delete() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{}"#).await;
        let _ = super::comments_delete(&cfg, "case1", "comment1").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_cases_comments_update() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, "").await;
        super::comments_update(&cfg, "case1", "comment1", "new body")
            .await
            .expect("update should succeed against mock");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_cases_timeline() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        super::timeline(&cfg, "case1")
            .await
            .expect("timeline should succeed");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_cases_comments_list_filters_to_comment_cells() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        // Mixed-cell timeline: only the COMMENT cell should be retained.
        let body = r#"{"data": [
            {"id": "a", "type": "timeline_cell", "attributes": {"type": "COMMENT", "cell_content": {"message": "hi"}}},
            {"id": "b", "type": "timeline_cell", "attributes": {"type": "ATTRIBUTE_UPDATE"}},
            {"id": "c", "type": "timeline_cell", "attributes": {"type": "CASE_CREATED"}}
        ]}"#;
        mock_all(&mut s, body).await;
        super::comments_list(&cfg, "case1")
            .await
            .expect("comments_list should succeed");
        // Also verify the helper directly so the filtering contract is asserted.
        let resp: serde_json::Value = serde_json::from_str(body).unwrap();
        let cells = super::comment_cells(&resp);
        assert_eq!(cells.len(), 1, "only one COMMENT cell expected");
        assert_eq!(cells[0]["id"], "a");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_cases_comments_get_found() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let body = r#"{"data": [
            {"id": "target", "type": "timeline_cell", "attributes": {"type": "COMMENT", "cell_content": {"message": "found me"}}}
        ]}"#;
        mock_all(&mut s, body).await;
        super::comments_get(&cfg, "case1", "target")
            .await
            .expect("comments_get should find the comment");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_cases_comments_get_not_found() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        // Empty timeline — get should report not found.
        mock_all(&mut s, r#"{"data": []}"#).await;
        let err = super::comments_get(&cfg, "case1", "missing")
            .await
            .expect_err("missing comment should error");
        assert!(
            err.to_string().contains("not found"),
            "expected 'not found' in error, got: {err}"
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_cases_comments_get_skips_non_comment_cell_with_same_id() {
        // If a non-COMMENT cell happens to share an id (unlikely but worth
        // defending against), comments_get should ignore it and report not found.
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let body = r#"{"data": [
            {"id": "shared", "type": "timeline_cell", "attributes": {"type": "ATTRIBUTE_UPDATE"}}
        ]}"#;
        mock_all(&mut s, body).await;
        let err = super::comments_get(&cfg, "case1", "shared")
            .await
            .expect_err("non-comment cell should not match comments_get");
        assert!(
            err.to_string().contains("not found"),
            "expected 'not found' in error, got: {err}"
        );
        cleanup_env();
    }

    const CASE_RESP: &str = r#"{"data": {"id": "case1", "type": "case", "attributes": {}}}"#;
    const VIEW: &str = r#"{"id": "view1", "type": "view", "attributes": {"created_at": "2026-01-01T00:00:00Z", "name": "Open bugs", "query": "status:open"}}"#;
    const RULE: &str = r#"{"id": "r1", "type": "rule", "attributes": {"action": {"data": {"handle": "wf"}, "type": "EXECUTE_WORKFLOW"}, "created_at": "2026-01-01T00:00:00Z", "name": "Auto-assign", "state": "ENABLED", "trigger": {"type": "CASE_CREATED"}}}"#;
    const LINK: &str = r#"{"id": "link1", "type": "link", "attributes": {"parent_entity_type": "CASE", "parent_entity_id": "case1", "child_entity_type": "CASE", "child_entity_id": "case2", "relationship": "BLOCKS"}}"#;
    const CASE_TYPE: &str =
        r#"{"id": "t1", "type": "case_type", "attributes": {"name": "Investigation"}}"#;
    const ATTRIBUTE: &str = r#"{"id": "a1", "type": "custom_attribute", "attributes": {"case_type_id": "t1", "display_name": "Region", "is_multi": true, "key": "region", "type": "TEXT"}}"#;

    fn one(item: &str) -> String {
        format!(r#"{{"data": {item}}}"#)
    }

    fn many(item: &str) -> String {
        format!(r#"{{"data": [{item}]}}"#)
    }

    async fn expect_call(
        s: &mut mockito::Server,
        method: &str,
        path: &str,
        body: Option<serde_json::Value>,
        response: &str,
    ) -> mockito::Mock {
        let mut m = s
            .mock(method, path)
            .match_query(mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(response);
        if let Some(b) = body {
            m = m.match_body(mockito::Matcher::PartialJson(b));
        }
        m.create_async().await
    }

    /// Writes `body` to a temp file and returns its path alongside the parsed
    /// JSON, so file-backed tests can assert the file is forwarded verbatim.
    fn body_file(name: &str, body: &str) -> (std::path::PathBuf, serde_json::Value) {
        let path =
            std::env::temp_dir().join(format!("pup-cases-{name}-{}.json", std::process::id()));
        std::fs::write(&path, body).unwrap();
        (path, serde_json::from_str(body).unwrap())
    }

    #[tokio::test]
    async fn test_cases_count_sends_filters() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let m = s
            .mock("GET", "/api/v2/cases/count")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("query_filter".into(), "status:open".into()),
                mockito::Matcher::UrlEncoded("group_bys".into(), "priority".into()),
                mockito::Matcher::UrlEncoded("limit".into(), "5".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"data": {"id": "count", "type": "count", "attributes": {"groups": []}}}"#,
            )
            .create_async()
            .await;
        let result = super::count(
            &cfg,
            Some("status:open".into()),
            Some("priority".into()),
            Some(5),
        )
        .await;
        cleanup_env();
        result.expect("count should succeed");
        m.assert_async().await;
    }

    #[tokio::test]
    async fn test_cases_flag_built_requests() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let mocks = vec![
            expect_call(&mut s, "POST", "/api/v2/cases/case1/unassign", None, CASE_RESP).await,
            expect_call(
                &mut s,
                "POST",
                "/api/v2/cases/case1/due_date",
                Some(serde_json::json!({
                    "data": {"type": "case", "attributes": {"due_date": "2026-11-01"}}
                })),
                CASE_RESP,
            )
            .await,
            expect_call(
                &mut s,
                "POST",
                "/api/v2/cases/case1/resolved_reason",
                Some(serde_json::json!({
                    "data": {"type": "case", "attributes": {"security_resolved_reason": "FALSE_POSITIVE"}}
                })),
                CASE_RESP,
            )
            .await,
            expect_call(
                &mut s,
                "DELETE",
                "/api/v2/cases/case1/custom_attributes/region",
                None,
                CASE_RESP,
            )
            .await,
            expect_call(
                &mut s,
                "POST",
                "/api/v2/cases/case1/relationships/incidents",
                Some(serde_json::json!({"data": {"id": "inc1", "type": "incidents"}})),
                CASE_RESP,
            )
            .await,
        ];
        let results = [
            super::unassign(&cfg, "case1").await,
            super::update_due_date(&cfg, "case1", "2026-11-01").await,
            super::update_resolved_reason(&cfg, "case1", "FALSE_POSITIVE").await,
            super::delete_custom_attribute(&cfg, "case1", "region").await,
            super::incidents_link(&cfg, "case1", "inc1").await,
        ];
        cleanup_env();
        for (i, r) in results.into_iter().enumerate() {
            r.unwrap_or_else(|e| panic!("call #{i} should succeed: {e}"));
        }
        for m in mocks {
            m.assert_async().await;
        }
    }

    #[tokio::test]
    async fn test_cases_file_backed_requests() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let fixtures = [
            (
                "aggregate",
                r#"{"data": {"attributes": {"group_by": {"groups": ["status"], "limit": 14}, "query_filter": "status:open"}, "type": "aggregate"}}"#,
            ),
            (
                "bulk",
                r#"{"data": {"attributes": {"case_ids": ["case1"], "payload": {"priority": "P1"}, "type": "priority"}, "type": "bulk"}}"#,
            ),
            (
                "attrs",
                r#"{"data": {"attributes": {"attributes": {"env": ["prod"]}}, "type": "case"}}"#,
            ),
            (
                "custom",
                r#"{"data": {"attributes": {"type": "TEXT", "is_multi": true, "value": ["us-east-1"]}, "type": "case"}}"#,
            ),
            (
                "insights",
                r#"{"data": {"attributes": {"insights": [{"ref": "/monitors/12345", "resource_id": "12345", "type": "SECURITY_SIGNAL"}]}, "type": "case"}}"#,
            ),
            ("notebook", r#"{"data": {"type": "notebook"}}"#),
            (
                "view",
                r#"{"data": {"attributes": {"name": "Open bugs", "project_id": "proj1", "query": "status:open"}, "type": "view"}}"#,
            ),
            (
                "rule",
                r#"{"data": {"attributes": {"action": {"data": {"handle": "wf"}, "type": "EXECUTE_WORKFLOW"}, "name": "Auto-assign", "state": "ENABLED", "trigger": {"data": {}, "type": "CASE_CREATED"}}, "type": "rule"}}"#,
            ),
            (
                "type",
                r#"{"data": {"attributes": {"description": "Investigations", "name": "Investigation"}, "type": "case_type"}}"#,
            ),
            (
                "attr_cfg",
                r#"{"data": {"attributes": {"display_name": "Region", "is_multi": true, "key": "region", "type": "TEXT"}, "type": "custom_attribute"}}"#,
            ),
        ];
        let mut files = std::collections::HashMap::new();
        for (name, body) in fixtures {
            files.insert(name, body_file(name, body));
        }
        let f = |name: &str| files[name].0.to_str().unwrap().to_string();
        let b = |name: &str| Some(files[name].1.clone());
        let agg = r#"{"data": {"id": "agg", "type": "aggregate", "attributes": {"groups": [], "total": 0}}}"#;
        let (view, rule) = (one(VIEW), one(RULE));
        let (case_type, attribute) = (one(CASE_TYPE), one(ATTRIBUTE));
        let rules = "/api/v2/cases/projects/proj1/rules";
        let attrs = "/api/v2/cases/types/t1/custom_attributes";
        let mocks = vec![
            expect_call(
                &mut s,
                "POST",
                "/api/v2/cases/aggregate",
                b("aggregate"),
                agg,
            )
            .await,
            expect_call(&mut s, "POST", "/api/v2/cases/bulk", b("bulk"), "").await,
            expect_call(
                &mut s,
                "POST",
                "/api/v2/cases/case1/attributes",
                b("attrs"),
                CASE_RESP,
            )
            .await,
            expect_call(
                &mut s,
                "POST",
                "/api/v2/cases/case1/custom_attributes/region",
                b("custom"),
                CASE_RESP,
            )
            .await,
            expect_call(
                &mut s,
                "PUT",
                "/api/v2/cases/case1/insights",
                b("insights"),
                CASE_RESP,
            )
            .await,
            expect_call(
                &mut s,
                "DELETE",
                "/api/v2/cases/case1/insights",
                b("insights"),
                CASE_RESP,
            )
            .await,
            expect_call(
                &mut s,
                "POST",
                "/api/v2/cases/case1/relationships/notebook",
                b("notebook"),
                "",
            )
            .await,
            expect_call(&mut s, "POST", "/api/v2/cases/views", b("view"), &view).await,
            expect_call(&mut s, "PUT", "/api/v2/cases/views/view1", b("view"), &view).await,
            expect_call(&mut s, "POST", rules, b("rule"), &rule).await,
            expect_call(&mut s, "PUT", &format!("{rules}/r1"), b("rule"), &rule).await,
            expect_call(&mut s, "POST", "/api/v2/cases/types", b("type"), &case_type).await,
            expect_call(
                &mut s,
                "PUT",
                "/api/v2/cases/types/t1",
                b("type"),
                &case_type,
            )
            .await,
            expect_call(&mut s, "POST", attrs, b("attr_cfg"), &attribute).await,
            expect_call(
                &mut s,
                "PUT",
                &format!("{attrs}/a1"),
                b("attr_cfg"),
                &attribute,
            )
            .await,
        ];
        let results = [
            super::aggregate(&cfg, &f("aggregate")).await,
            super::bulk_update(&cfg, &f("bulk")).await,
            super::update_attributes(&cfg, "case1", &f("attrs")).await,
            super::update_custom_attribute(&cfg, "case1", "region", &f("custom")).await,
            super::insights_add(&cfg, "case1", &f("insights")).await,
            super::insights_remove(&cfg, "case1", &f("insights")).await,
            super::notebooks_create(&cfg, "case1", &f("notebook")).await,
            super::views_create(&cfg, &f("view")).await,
            super::views_update(&cfg, "view1", &f("view")).await,
            super::projects_automation_rules_create(&cfg, "proj1", &f("rule")).await,
            super::projects_automation_rules_update(&cfg, "proj1", "r1", &f("rule")).await,
            super::types_create(&cfg, &f("type")).await,
            super::types_update(&cfg, "t1", &f("type")).await,
            super::custom_attributes_create(&cfg, "t1", &f("attr_cfg")).await,
            super::custom_attributes_update(&cfg, "t1", "a1", &f("attr_cfg")).await,
        ];
        cleanup_env();
        for (path, _) in files.values() {
            let _ = std::fs::remove_file(path);
        }
        for (i, r) in results.into_iter().enumerate() {
            r.unwrap_or_else(|e| panic!("call #{i} should succeed: {e}"));
        }
        for m in mocks {
            m.assert_async().await;
        }
    }

    #[tokio::test]
    async fn test_cases_watchers() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let watchers = r#"{"data": [{"id": "user1", "type": "watcher", "relationships": {"user": {"data": {"id": "user1", "type": "user"}}}}]}"#;
        let base = "/api/v2/cases/case1/watchers";
        let mocks = vec![
            expect_call(&mut s, "GET", base, None, watchers).await,
            expect_call(&mut s, "POST", &format!("{base}/user1"), None, "").await,
            expect_call(&mut s, "DELETE", &format!("{base}/user1"), None, "").await,
        ];
        let results = [
            super::watchers_list(&cfg, "case1").await,
            super::watchers_add(&cfg, "case1", "user1").await,
            super::watchers_remove(&cfg, "case1", "user1").await,
        ];
        cleanup_env();
        for (i, r) in results.into_iter().enumerate() {
            r.unwrap_or_else(|e| panic!("call #{i} should succeed: {e}"));
        }
        for m in mocks {
            m.assert_async().await;
        }
    }

    #[tokio::test]
    async fn test_cases_links() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let list = s
            .mock("GET", "/api/v2/cases/link")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("entity_type".into(), "CASE".into()),
                mockito::Matcher::UrlEncoded("entity_id".into(), "case1".into()),
                mockito::Matcher::UrlEncoded("relationship".into(), "BLOCKS".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(many(LINK))
            .create_async()
            .await;
        let create = expect_call(
            &mut s,
            "POST",
            "/api/v2/cases/link",
            Some(serde_json::json!({
                "data": {
                    "type": "link",
                    "attributes": {
                        "parent_entity_type": "CASE",
                        "parent_entity_id": "case1",
                        "child_entity_type": "CASE",
                        "child_entity_id": "case2",
                        "relationship": "BLOCKS"
                    }
                }
            })),
            &one(LINK),
        )
        .await;
        let del = expect_call(&mut s, "DELETE", "/api/v2/cases/link/link1", None, "").await;
        let results = [
            super::links_list(&cfg, "CASE", "case1", Some("BLOCKS".into())).await,
            super::links_create(&cfg, "CASE", "case1", "CASE", "case2", "BLOCKS").await,
            super::links_delete(&cfg, "link1").await,
        ];
        cleanup_env();
        for (i, r) in results.into_iter().enumerate() {
            r.unwrap_or_else(|e| panic!("call #{i} should succeed: {e}"));
        }
        for m in [list, create, del] {
            m.assert_async().await;
        }
    }

    #[tokio::test]
    async fn test_cases_views_read_and_delete() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let list = s
            .mock("GET", "/api/v2/cases/views")
            .match_query(mockito::Matcher::UrlEncoded(
                "project_id".into(),
                "proj1".into(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(many(VIEW))
            .create_async()
            .await;
        let get = expect_call(&mut s, "GET", "/api/v2/cases/views/view1", None, &one(VIEW)).await;
        let del = expect_call(&mut s, "DELETE", "/api/v2/cases/views/view1", None, "").await;
        let results = [
            super::views_list(&cfg, "proj1").await,
            super::views_get(&cfg, "view1").await,
            super::views_delete(&cfg, "view1").await,
        ];
        cleanup_env();
        for (i, r) in results.into_iter().enumerate() {
            r.unwrap_or_else(|e| panic!("call #{i} should succeed: {e}"));
        }
        for m in [list, get, del] {
            m.assert_async().await;
        }
    }

    #[tokio::test]
    async fn test_cases_projects_favorites() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let favorites = r#"{"data": [{"id": "proj1", "type": "project_favorite"}]}"#;
        let fav = "/api/v2/cases/projects/proj1/favorites";
        let mocks = vec![
            expect_call(
                &mut s,
                "GET",
                "/api/v2/cases/projects/favorites",
                None,
                favorites,
            )
            .await,
            expect_call(&mut s, "POST", fav, None, "").await,
            expect_call(&mut s, "DELETE", fav, None, "").await,
        ];
        let results = [
            super::projects_favorites_list(&cfg).await,
            super::projects_favorites_add(&cfg, "proj1").await,
            super::projects_favorites_remove(&cfg, "proj1").await,
        ];
        cleanup_env();
        for (i, r) in results.into_iter().enumerate() {
            r.unwrap_or_else(|e| panic!("call #{i} should succeed: {e}"));
        }
        for m in mocks {
            m.assert_async().await;
        }
    }

    #[tokio::test]
    async fn test_cases_projects_automation_rules_actions() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let base = "/api/v2/cases/projects/proj1/rules";
        let rule = one(RULE);
        let mocks = vec![
            expect_call(&mut s, "GET", base, None, &many(RULE)).await,
            expect_call(&mut s, "GET", &format!("{base}/r1"), None, &rule).await,
            expect_call(&mut s, "DELETE", &format!("{base}/r1"), None, "").await,
            expect_call(&mut s, "POST", &format!("{base}/r1/enable"), None, &rule).await,
            expect_call(&mut s, "POST", &format!("{base}/r1/disable"), None, &rule).await,
        ];
        let results = [
            super::projects_automation_rules_list(&cfg, "proj1").await,
            super::projects_automation_rules_get(&cfg, "proj1", "r1").await,
            super::projects_automation_rules_delete(&cfg, "proj1", "r1").await,
            super::projects_automation_rules_enable(&cfg, "proj1", "r1").await,
            super::projects_automation_rules_disable(&cfg, "proj1", "r1").await,
        ];
        cleanup_env();
        for (i, r) in results.into_iter().enumerate() {
            r.unwrap_or_else(|e| panic!("call #{i} should succeed: {e}"));
        }
        for m in mocks {
            m.assert_async().await;
        }
    }

    #[tokio::test]
    async fn test_cases_types_and_custom_attributes_read_and_delete() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let attrs = many(ATTRIBUTE);
        let mocks = vec![
            expect_call(&mut s, "GET", "/api/v2/cases/types", None, &many(CASE_TYPE)).await,
            expect_call(&mut s, "DELETE", "/api/v2/cases/types/t1", None, "").await,
            expect_call(
                &mut s,
                "GET",
                "/api/v2/cases/types/custom_attributes",
                None,
                &attrs,
            )
            .await,
            expect_call(
                &mut s,
                "GET",
                "/api/v2/cases/types/t1/custom_attributes",
                None,
                &attrs,
            )
            .await,
            expect_call(
                &mut s,
                "DELETE",
                "/api/v2/cases/types/t1/custom_attributes/a1",
                None,
                "",
            )
            .await,
        ];
        let results = [
            super::types_list(&cfg).await,
            super::types_delete(&cfg, "t1").await,
            super::custom_attributes_list(&cfg, None).await,
            super::custom_attributes_list(&cfg, Some("t1")).await,
            super::custom_attributes_delete(&cfg, "t1", "a1").await,
        ];
        cleanup_env();
        for (i, r) in results.into_iter().enumerate() {
            r.unwrap_or_else(|e| panic!("call #{i} should succeed: {e}"));
        }
        for m in mocks {
            m.assert_async().await;
        }
    }

    #[tokio::test]
    async fn test_cases_file_commands_reject_missing_file() {
        let _lock = lock_env().await;
        let cfg = test_config("http://nowhere.invalid");
        let f = "/nonexistent/body.json";
        let results = [
            super::aggregate(&cfg, f).await,
            super::bulk_update(&cfg, f).await,
            super::update_attributes(&cfg, "c", f).await,
            super::update_custom_attribute(&cfg, "c", "k", f).await,
            super::insights_add(&cfg, "c", f).await,
            super::insights_remove(&cfg, "c", f).await,
            super::notebooks_create(&cfg, "c", f).await,
            super::views_create(&cfg, f).await,
            super::views_update(&cfg, "v", f).await,
            super::projects_automation_rules_create(&cfg, "p", f).await,
            super::projects_automation_rules_update(&cfg, "p", "r", f).await,
            super::types_create(&cfg, f).await,
            super::types_update(&cfg, "t", f).await,
            super::custom_attributes_create(&cfg, "t", f).await,
            super::custom_attributes_update(&cfg, "t", "a", f).await,
        ];
        cleanup_env();
        for (i, r) in results.into_iter().enumerate() {
            let err = r.expect_err(&format!("command #{i} should fail on missing file"));
            assert!(
                err.to_string().contains("failed to read file"),
                "command #{i}: unexpected error {err}"
            );
        }
    }

    #[tokio::test]
    async fn test_cases_api_errors_propagate() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        for method in ["GET", "POST", "DELETE"] {
            s.mock(method, mockito::Matcher::Any)
                .match_query(mockito::Matcher::Any)
                .with_status(404)
                .with_header("content-type", "application/json")
                .with_body(r#"{"errors": ["not found"]}"#)
                .create_async()
                .await;
        }
        let checks = [
            (
                super::watchers_add(&cfg, "c", "u").await,
                "add case watcher",
            ),
            (super::links_delete(&cfg, "l").await, "delete case link"),
            (super::types_delete(&cfg, "t").await, "delete case type"),
            (super::unassign(&cfg, "c").await, "unassign case"),
            (
                super::projects_automation_rules_enable(&cfg, "p", "r").await,
                "enable automation rule",
            ),
            (super::count(&cfg, None, None, None).await, "count cases"),
        ];
        cleanup_env();
        for (r, ctx) in checks {
            let err = r.expect_err(&format!("{ctx} should fail on 404"));
            assert!(err.to_string().contains(ctx), "expected '{ctx}' in: {err}");
        }
    }

    #[test]
    fn test_parse_priority_valid() {
        for (input, expected) in [
            ("P1", super::CasePriority::P1),
            ("p2", super::CasePriority::P2),
            ("P3", super::CasePriority::P3),
            ("P4", super::CasePriority::P4),
            ("P5", super::CasePriority::P5),
            ("NOT_DEFINED", super::CasePriority::NOT_DEFINED),
            ("not_defined", super::CasePriority::NOT_DEFINED),
        ] {
            let got = super::parse_priority(input).expect("valid priority should parse");
            assert_eq!(
                got, expected,
                "priority {input} should parse to {expected:?}"
            );
        }
    }

    #[test]
    fn test_parse_priority_invalid() {
        let err = super::parse_priority("P9").unwrap_err().to_string();
        assert!(
            err.contains("invalid priority"),
            "expected 'invalid priority' in error, got: {err}"
        );
    }
}
