use crate::config::Config;
use crate::formatter;
use crate::util_ext;
use anyhow::Result;
use datadog_api_client::datadogV1::api_slack_integration::SlackIntegrationAPI;
use datadog_api_client::datadogV1::api_webhooks_integration::WebhooksIntegrationAPI;
use datadog_api_client::datadogV2::api_integrations::IntegrationsAPI;
use datadog_api_client::datadogV2::api_jira_integration::JiraIntegrationAPI;
use datadog_api_client::datadogV2::api_service_now_integration::ServiceNowIntegrationAPI;
use datadog_api_client::datadogV2::model::{
    JiraIssueTemplateCreateRequest, JiraIssueTemplateUpdateRequest,
    ServiceNowTemplateCreateRequest, ServiceNowTemplateUpdateRequest,
};

// ---- Jira ----

pub async fn jira_accounts_list(cfg: &Config) -> Result<()> {
    let api = crate::make_api!(JiraIntegrationAPI, cfg);
    let resp = api
        .list_jira_accounts()
        .await
        .map_err(|e| anyhow::anyhow!("failed to list Jira accounts: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn jira_templates_list(cfg: &Config) -> Result<()> {
    let api = crate::make_api!(JiraIntegrationAPI, cfg);
    let resp = api
        .list_jira_issue_templates()
        .await
        .map_err(|e| anyhow::anyhow!("failed to list Jira templates: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn jira_templates_get(cfg: &Config, template_id: &str) -> Result<()> {
    let api = crate::make_api!(JiraIntegrationAPI, cfg);
    let uuid = util_ext::parse_uuid(template_id, "template")?;
    let resp = api
        .get_jira_issue_template(uuid)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get Jira template: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn jira_accounts_delete(cfg: &Config, account_id: &str) -> Result<()> {
    let api = crate::make_api!(JiraIntegrationAPI, cfg);
    let uuid = util_ext::parse_uuid(account_id, "account")?;
    api.delete_jira_account(uuid)
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete Jira account: {e:?}"))?;
    println!("Jira account {account_id} deleted.");
    Ok(())
}

pub async fn jira_templates_create(cfg: &Config, file: &str) -> Result<()> {
    let api = crate::make_api!(JiraIntegrationAPI, cfg);
    let body: JiraIssueTemplateCreateRequest = crate::util::read_json_file(file)?;
    let resp = api
        .create_jira_issue_template(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create Jira template: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn jira_templates_update(cfg: &Config, template_id: &str, file: &str) -> Result<()> {
    let api = crate::make_api!(JiraIntegrationAPI, cfg);
    let uuid = util_ext::parse_uuid(template_id, "template")?;
    let body: JiraIssueTemplateUpdateRequest = crate::util::read_json_file(file)?;
    let resp = api
        .update_jira_issue_template(uuid, body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update Jira template: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn jira_templates_delete(cfg: &Config, template_id: &str) -> Result<()> {
    let api = crate::make_api!(JiraIntegrationAPI, cfg);
    let uuid = util_ext::parse_uuid(template_id, "template")?;
    api.delete_jira_issue_template(uuid)
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete Jira template: {e:?}"))?;
    println!("Jira template {template_id} deleted.");
    Ok(())
}

// ---- ServiceNow ----

pub async fn servicenow_instances_list(cfg: &Config) -> Result<()> {
    let api = crate::make_api!(ServiceNowIntegrationAPI, cfg);
    let resp = api
        .list_service_now_instances()
        .await
        .map_err(|e| anyhow::anyhow!("failed to list ServiceNow instances: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn servicenow_templates_list(cfg: &Config) -> Result<()> {
    let api = crate::make_api!(ServiceNowIntegrationAPI, cfg);
    let resp = api
        .list_service_now_templates()
        .await
        .map_err(|e| anyhow::anyhow!("failed to list ServiceNow templates: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn servicenow_templates_get(cfg: &Config, template_id: &str) -> Result<()> {
    let api = crate::make_api!(ServiceNowIntegrationAPI, cfg);
    let uuid = util_ext::parse_uuid(template_id, "template")?;
    let resp = api
        .get_service_now_template(uuid)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get ServiceNow template: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn servicenow_templates_create(cfg: &Config, file: &str) -> Result<()> {
    let api = crate::make_api!(ServiceNowIntegrationAPI, cfg);
    let body: ServiceNowTemplateCreateRequest = crate::util::read_json_file(file)?;
    let resp = api
        .create_service_now_template(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create ServiceNow template: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn servicenow_templates_update(
    cfg: &Config,
    template_id: &str,
    file: &str,
) -> Result<()> {
    let api = crate::make_api!(ServiceNowIntegrationAPI, cfg);
    let uuid = util_ext::parse_uuid(template_id, "template")?;
    let body: ServiceNowTemplateUpdateRequest = crate::util::read_json_file(file)?;
    let resp = api
        .update_service_now_template(uuid, body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update ServiceNow template: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn servicenow_templates_delete(cfg: &Config, template_id: &str) -> Result<()> {
    let api = crate::make_api!(ServiceNowIntegrationAPI, cfg);
    let uuid = util_ext::parse_uuid(template_id, "template")?;
    api.delete_service_now_template(uuid)
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete ServiceNow template: {e:?}"))?;
    println!("ServiceNow template {template_id} deleted.");
    Ok(())
}

pub async fn servicenow_users_list(cfg: &Config, instance_name: &str) -> Result<()> {
    let api = crate::make_api!(ServiceNowIntegrationAPI, cfg);
    let resp = api
        .list_service_now_users(util_ext::parse_uuid(instance_name, "instance")?)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list ServiceNow users: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn servicenow_assignment_groups_list(cfg: &Config, instance_name: &str) -> Result<()> {
    let api = crate::make_api!(ServiceNowIntegrationAPI, cfg);
    let resp = api
        .list_service_now_assignment_groups(util_ext::parse_uuid(instance_name, "instance")?)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list ServiceNow assignment groups: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn servicenow_business_services_list(cfg: &Config, instance_name: &str) -> Result<()> {
    let api = crate::make_api!(ServiceNowIntegrationAPI, cfg);
    let resp = api
        .list_service_now_business_services(util_ext::parse_uuid(instance_name, "instance")?)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list ServiceNow business services: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---- Slack ----

pub async fn slack_list(cfg: &Config) -> Result<()> {
    let api = crate::make_api!(SlackIntegrationAPI, cfg);
    let resp = api
        .get_slack_integration_channels("main".to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to list Slack channels: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---- PagerDuty ----

pub async fn pagerduty_list(_cfg: &Config) -> Result<()> {
    anyhow::bail!(
        "listing PagerDuty services is not supported by the current API version \
         - use 'get' with a specific service name instead"
    )
}

// ---- Webhooks ----

pub async fn webhooks_list(cfg: &Config) -> Result<()> {
    let api = crate::make_api!(WebhooksIntegrationAPI, cfg);
    let resp = api
        .get_webhooks_integration("main".to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to list webhooks: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---- Integrations v2 list ----

pub async fn list(cfg: &Config) -> Result<()> {
    let api = crate::make_api!(IntegrationsAPI, cfg);
    let resp = api
        .list_integrations()
        .await
        .map_err(|e| anyhow::anyhow!("failed to list integrations: {e:?}"))?;
    formatter::output(cfg, &resp)
}

#[cfg(test)]
mod tests {

    use crate::test_support::*;

    #[tokio::test]
    async fn test_integrations_jira_accounts_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::jira_accounts_list(&cfg).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_integrations_jira_templates_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::jira_templates_list(&cfg).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_integrations_servicenow_instances_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::servicenow_instances_list(&cfg).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_integrations_servicenow_templates_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::servicenow_templates_list(&cfg).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_integrations_slack_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::slack_list(&cfg).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_integrations_webhooks_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::webhooks_list(&cfg).await;
        cleanup_env();
    }
}
