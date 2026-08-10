use anyhow::Result;
use std::collections::BTreeMap;

use datadog_api_client::datadogV2::api_action_connection::ActionConnectionAPI;
use datadog_api_client::datadogV2::api_workflow_automation::{
    ListWorkflowInstancesOptionalParams, WorkflowAutomationAPI,
};

use crate::config::Config;
use crate::formatter::{self, Metadata};
use crate::raw_client;
use crate::util;
use crate::util_ext;

// ---------------------------------------------------------------------------
// Helper: build a WorkflowAutomationAPI
// ---------------------------------------------------------------------------

fn make_api(cfg: &Config) -> WorkflowAutomationAPI {
    crate::make_api!(WorkflowAutomationAPI, cfg)
}

// ---------------------------------------------------------------------------
// Workflow CRUD
// ---------------------------------------------------------------------------

pub async fn get(cfg: &Config, workflow_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let resp = api
        .get_workflow(workflow_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get workflow: {:?}", e))?;
    formatter::output(cfg, &resp)
}

pub async fn create(cfg: &Config, file: &str) -> Result<()> {
    let body: datadog_api_client::datadogV2::model::CreateWorkflowRequest =
        util::read_json_file(file)?;
    let api = make_api(cfg);
    let resp = api
        .create_workflow(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create workflow: {:?}", e))?;
    formatter::output(cfg, &resp)
}

pub async fn update(cfg: &Config, workflow_id: &str, file: &str) -> Result<()> {
    let body: datadog_api_client::datadogV2::model::UpdateWorkflowRequest =
        util::read_json_file(file)?;
    let api = make_api(cfg);
    let resp = api
        .update_workflow(workflow_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update workflow: {:?}", e))?;
    formatter::output(cfg, &resp)
}

pub async fn diff(
    cfg: &Config,
    workflow_id: &str,
    file: &str,
    only: &[String],
    ignore: &[String],
) -> Result<()> {
    let candidate: serde_json::Value = util::read_json_file(file)?;
    let live = raw_client::raw_get(cfg, &format!("/api/v2/workflows/{workflow_id}"), &[])
        .await
        .map_err(|e| anyhow::anyhow!("failed to get workflow: {e:?}"))?;

    let mut options = util_ext::ResourceDiffOptions::new(
        "workflows diff",
        "pup workflows update",
        "workflow",
        workflow_id,
    );
    options.readonly_paths = util_ext::READONLY_WORKFLOW_FIELDS;
    options.only = only;
    options.ignore = ignore;
    options.no_changes_message = Some(format!("No changes - workflow {workflow_id} is in sync."));
    util_ext::format_resource_diff(cfg, &live, &candidate, &options)
}

pub async fn delete(cfg: &Config, workflow_id: &str) -> Result<()> {
    let api = make_api(cfg);
    api.delete_workflow(workflow_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete workflow: {:?}", e))?;
    eprintln!("Workflow {workflow_id} deleted.");
    Ok(())
}

// ---------------------------------------------------------------------------
// Workflow execution
// ---------------------------------------------------------------------------

pub async fn run(
    cfg: &Config,
    workflow_id: &str,
    payload: Option<String>,
    payload_file: Option<String>,
    wait: bool,
    timeout: &str,
) -> Result<()> {
    let api = crate::make_api!(WorkflowAutomationAPI, cfg);

    let input_payload: Option<BTreeMap<String, serde_json::Value>> = match (&payload, &payload_file)
    {
        (Some(_), Some(_)) => {
            return Err(anyhow::anyhow!(
                "cannot specify both --payload and --payload-file"
            ))
        }
        (Some(json_str), None) => Some(serde_json::from_str(json_str)?),
        (None, Some(path)) => {
            let contents = std::fs::read_to_string(path)
                .map_err(|e| anyhow::anyhow!("failed to read payload file {path:?}: {e}"))?;
            Some(serde_json::from_str(&contents)?)
        }
        (None, None) => None,
    };

    let mut request = datadog_api_client::datadogV2::model::WorkflowInstanceCreateRequest::new();
    if let Some(p) = input_payload {
        request = request.meta(
            datadog_api_client::datadogV2::model::WorkflowInstanceCreateMeta::new().payload(p),
        );
    }

    let response = api
        .create_workflow_instance(workflow_id.to_string(), request)
        .await
        .map_err(|e| anyhow::anyhow!("failed to execute workflow: {:?}", e))?;

    if !wait {
        formatter::output(cfg, &response)?;
        return Ok(());
    }

    let instance_id = response
        .data
        .as_ref()
        .and_then(|d| d.id.as_ref())
        .ok_or_else(|| anyhow::anyhow!("no instance ID in response"))?
        .clone();

    eprintln!("Instance {instance_id} started, waiting for completion...");

    let timeout_duration =
        std::time::Duration::from_millis(util_ext::parse_duration_to_millis(timeout)? as u64);
    let start = std::time::Instant::now();

    loop {
        if start.elapsed() > timeout_duration {
            return Err(anyhow::anyhow!(
                "timed out after {} waiting for instance {instance_id}",
                timeout
            ));
        }

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        let api = crate::make_api!(WorkflowAutomationAPI, cfg);
        let status = api
            .get_workflow_instance(workflow_id.to_string(), instance_id.clone())
            .await
            .map_err(|e| anyhow::anyhow!("failed to get instance status: {:?}", e))?;

        let state = status
            .data
            .as_ref()
            .and_then(|d| d.attributes.as_ref())
            .and_then(|a| a.additional_properties.get("instanceStatus"))
            .and_then(|v| v.get("detailsKind"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        eprintln!("  status: {state}");

        // Treat any state other than the known in-progress states as terminal.
        // This avoids polling forever if the API introduces new terminal states.
        match state {
            "" | "IN_PROGRESS" => continue,
            _ => {
                formatter::output(cfg, &status)?;
                return Ok(());
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Workflow instances
// ---------------------------------------------------------------------------

pub async fn instance_list(cfg: &Config, workflow_id: &str, limit: i64, page: i64) -> Result<()> {
    let api = make_api(cfg);

    if !(1..=100).contains(&limit) {
        anyhow::bail!("--limit must be between 1 and 100, got {limit}");
    }
    let mut params = ListWorkflowInstancesOptionalParams::default();
    params = params.page_size(limit);
    if page > 0 {
        params = params.page_number(page);
    }

    let resp = api
        .list_workflow_instances(workflow_id.to_string(), params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list workflow instances: {:?}", e))?;

    let count = resp.data.as_ref().map(|d| d.len());
    let meta = Metadata {
        count,
        truncated: false,
        command: Some("workflows instances list".to_string()),
        next_action: None,
    };
    formatter::format_and_print(
        &resp,
        &cfg.output_format,
        cfg.agent_mode,
        Some(&meta),
        cfg.jq.as_deref(),
    )
}

pub async fn instance_get(cfg: &Config, workflow_id: &str, instance_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let resp = api
        .get_workflow_instance(workflow_id.to_string(), instance_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get workflow instance: {:?}", e))?;
    formatter::output(cfg, &resp)
}

pub async fn instance_cancel(cfg: &Config, workflow_id: &str, instance_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let resp = api
        .cancel_workflow_instance(workflow_id.to_string(), instance_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to cancel workflow instance: {:?}", e))?;
    formatter::output(cfg, &resp)
}

// ---------------------------------------------------------------------------
// Action Connections
// ---------------------------------------------------------------------------

fn make_connection_api(cfg: &Config) -> ActionConnectionAPI {
    crate::make_api_no_auth!(ActionConnectionAPI, cfg)
}

pub async fn connections_get(cfg: &Config, connection_id: &str) -> Result<()> {
    let api = make_connection_api(cfg);
    let resp = api
        .get_action_connection(connection_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get action connection: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn connections_create(cfg: &Config, file: &str) -> Result<()> {
    let body: datadog_api_client::datadogV2::model::CreateActionConnectionRequest =
        util::read_json_file(file)?;
    let api = make_connection_api(cfg);
    let resp = api
        .create_action_connection(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create action connection: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn connections_update(cfg: &Config, connection_id: &str, file: &str) -> Result<()> {
    let body: datadog_api_client::datadogV2::model::UpdateActionConnectionRequest =
        util::read_json_file(file)?;
    let api = make_connection_api(cfg);
    let resp = api
        .update_action_connection(connection_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update action connection: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn connections_delete(cfg: &Config, connection_id: &str) -> Result<()> {
    let api = make_connection_api(cfg);
    api.delete_action_connection(connection_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete action connection: {e:?}"))?;
    eprintln!("Action connection {connection_id} deleted.");
    Ok(())
}

#[cfg(test)]
mod tests {

    use crate::test_support::*;

    #[tokio::test]
    async fn test_workflows_diff_detects_changes() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        let live_body = r#"{
            "data": {
                "id": "wf-123",
                "type": "workflows",
                "attributes": {
                    "action_id": "wf-123",
                    "name": "Old workflow",
                    "description": "Deploy service",
                    "spec": {"steps": [{"name": "deploy", "timeout": 60}]},
                    "updatedAt": "2024-01-01T00:00:00Z"
                }
            }
        }"#;
        let _mock = mock_any(&mut server, "GET", live_body).await;

        let candidate = r#"{
            "data": {
                "type": "workflows",
                "attributes": {
                    "name": "New workflow",
                    "description": "Deploy service",
                    "spec": {"steps": [{"name": "deploy", "timeout": 90}]}
                }
            }
        }"#;
        let path = write_temp_json("pup_workflows_diff_detects_changes.json", candidate);

        let result = super::diff(&cfg, "wf-123", path.to_str().unwrap(), &[], &[]).await;
        let _ = std::fs::remove_file(path);
        assert!(result.is_ok(), "workflows diff failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_workflows_diff_file_not_found() {
        let cfg = test_config("http://unused.local");
        let result = super::diff(&cfg, "wf-123", "/nonexistent/path.json", &[], &[]).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("failed to read file"));
    }

    #[tokio::test]
    async fn test_workflows_diff_invalid_json() {
        let path = write_temp_json("pup_workflows_diff_invalid_json.json", "not valid json {{{");
        let cfg = test_config("http://unused.local");
        let result = super::diff(&cfg, "wf-123", path.to_str().unwrap(), &[], &[]).await;
        let _ = std::fs::remove_file(path);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("failed to parse JSON"));
    }

    #[tokio::test]
    async fn test_connections_get() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "GET", r#"{}"#).await;
        let result = super::connections_get(&cfg, "conn-id").await;
        assert!(result.is_ok(), "connections get failed: {:?}", result.err());
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_connections_delete() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "DELETE", "").await;
        let result = super::connections_delete(&cfg, "conn-id").await;
        assert!(
            result.is_ok(),
            "connections delete failed: {:?}",
            result.err()
        );
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_scorecard_rules_delete() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = server
            .mock("DELETE", mockito::Matcher::Any)
            .match_query(mockito::Matcher::Any)
            .with_status(204)
            .create_async()
            .await;
        let result = crate::commands::scorecards::rules_delete(&cfg, "rule-123").await;
        assert!(
            result.is_ok(),
            "scorecard rules delete failed: {:?}",
            result.err()
        );
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_instance_list_limit_too_small() {
        let cfg = test_config("http://unused.local");
        let result = super::instance_list(&cfg, "wf-id", 0, 0).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("--limit must be between 1 and 100"));
    }

    #[tokio::test]
    async fn test_instance_list_limit_too_large() {
        let cfg = test_config("http://unused.local");
        let result = super::instance_list(&cfg, "wf-id", 101, 0).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("--limit must be between 1 and 100"));
    }
}
