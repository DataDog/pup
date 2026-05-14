use anyhow::Result;
use datadog_api_client::datadogV2::api_case_management::{
    CaseManagementAPI, SearchCasesOptionalParams,
};
use datadog_api_client::datadogV2::model::{
    CaseAssign, CaseAssignAttributes, CaseAssignRequest, CaseComment, CaseCommentAttributes,
    CaseCommentRequest, CaseCreate, CaseCreateAttributes, CaseCreateRelationships,
    CaseCreateRequest, CaseEmpty, CaseEmptyRequest, CaseNotificationRuleCreateRequest,
    CaseNotificationRuleUpdateRequest, CasePriority, CaseResourceType, CaseStatus,
    CaseUpdatePriority, CaseUpdatePriorityAttributes, CaseUpdatePriorityRequest, CaseUpdateStatus,
    CaseUpdateStatusAttributes, CaseUpdateStatusRequest, CaseUpdateTitle,
    CaseUpdateTitleAttributes, CaseUpdateTitleRequest, JiraIssueCreateRequest,
    JiraIssueLinkRequest, ProjectCreate, ProjectCreateAttributes, ProjectCreateRequest,
    ProjectRelationship, ProjectRelationshipData, ProjectResourceType, ProjectUpdateRequest,
    ServiceNowTicketCreateRequest,
};

use crate::config::Config;
use crate::formatter;

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

pub async fn comment(cfg: &Config, case_id: &str, body: &str) -> Result<()> {
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

pub async fn delete_comment(cfg: &Config, case_id: &str, comment_id: &str) -> Result<()> {
    let api = make_api(cfg);
    api.delete_case_comment(case_id.to_string(), comment_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete comment: {e:?}"))?;
    println!("Comment {comment_id} deleted from case {case_id}.");
    Ok(())
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
    async fn test_cases_comment() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": {}}"#).await;
        let _ = super::comment(&cfg, "case1", "hello").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_cases_delete_comment() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{}"#).await;
        let _ = super::delete_comment(&cfg, "case1", "comment1").await;
        cleanup_env();
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
