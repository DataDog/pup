use anyhow::Result;

use datadog_api_client::datadogV2::api_deployment_gates::{
    DeploymentGatesAPI, ListDeploymentGatesOptionalParams,
};

use crate::config::Config;
use crate::formatter;
use crate::util;

// ---------------------------------------------------------------------------
// Helper: build a DeploymentGatesAPI with OAuth2 support
// ---------------------------------------------------------------------------

fn make_api(cfg: &Config) -> DeploymentGatesAPI {
    crate::make_api!(DeploymentGatesAPI, cfg)
}

// ---------------------------------------------------------------------------
// Gates CRUD
// ---------------------------------------------------------------------------

pub const LIST_PAGING: crate::pagination::CursorSpec = crate::pagination::CursorSpec {
    size: crate::pagination::PageSizeSpec {
        default: None,
        max: Some(1000),
    },
};

pub async fn list(cfg: &Config, page_cursor: Option<String>, page_size: Option<i64>) -> Result<()> {
    let api = make_api(cfg);
    let mut params = ListDeploymentGatesOptionalParams::default();
    if let Some(cursor) = page_cursor {
        params = params.page_cursor(cursor);
    }
    if let Some(size) = page_size {
        params = params.page_size(size);
    }
    let resp = api
        .list_deployment_gates(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list deployment gates: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn get(cfg: &Config, gate_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let resp = api
        .get_deployment_gate(gate_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get deployment gate: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn create(cfg: &Config, file: &str) -> Result<()> {
    let body: datadog_api_client::datadogV2::model::CreateDeploymentGateParams =
        util::read_json_file(file)?;
    let api = make_api(cfg);
    let resp = api
        .create_deployment_gate(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create deployment gate: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn update(cfg: &Config, gate_id: &str, file: &str) -> Result<()> {
    let body: datadog_api_client::datadogV2::model::UpdateDeploymentGateParams =
        util::read_json_file(file)?;
    let api = make_api(cfg);
    let resp = api
        .update_deployment_gate(gate_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update deployment gate: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn delete(cfg: &Config, gate_id: &str) -> Result<()> {
    let api = make_api(cfg);
    api.delete_deployment_gate(gate_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete deployment gate: {e:?}"))?;
    eprintln!("Deployment gate {gate_id} deleted.");
    Ok(())
}

// ---------------------------------------------------------------------------
// Evaluations
// ---------------------------------------------------------------------------

pub async fn evaluation_get(cfg: &Config, evaluation_id: &str) -> Result<()> {
    let id = evaluation_id
        .parse::<uuid::Uuid>()
        .map_err(|e| anyhow::anyhow!("invalid evaluation ID: {e:?}"))?;
    let api = make_api(cfg);
    let resp = api
        .get_deployment_gates_evaluation_result(id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get deployment gates evaluation result: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn evaluation_trigger(cfg: &Config, file: &str) -> Result<()> {
    let body: datadog_api_client::datadogV2::model::DeploymentGatesEvaluationRequest =
        util::read_json_file(file)?;
    let api = make_api(cfg);
    let resp = api
        .trigger_deployment_gates_evaluation(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to trigger deployment gates evaluation: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---------------------------------------------------------------------------
// Rules
// ---------------------------------------------------------------------------

pub async fn rules_list(cfg: &Config, gate_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let resp = api
        .get_deployment_gate_rules(gate_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to list deployment gate rules: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn rule_get(cfg: &Config, gate_id: &str, rule_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let resp = api
        .get_deployment_rule(gate_id.to_string(), rule_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get deployment rule: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn rule_create(cfg: &Config, gate_id: &str, file: &str) -> Result<()> {
    let body: datadog_api_client::datadogV2::model::CreateDeploymentRuleParams =
        util::read_json_file(file)?;
    let api = make_api(cfg);
    let resp = api
        .create_deployment_rule(gate_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create deployment rule: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn rule_update(cfg: &Config, gate_id: &str, rule_id: &str, file: &str) -> Result<()> {
    let body: datadog_api_client::datadogV2::model::UpdateDeploymentRuleParams =
        util::read_json_file(file)?;
    let api = make_api(cfg);
    let resp = api
        .update_deployment_rule(gate_id.to_string(), rule_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update deployment rule: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn rule_delete(cfg: &Config, gate_id: &str, rule_id: &str) -> Result<()> {
    let api = make_api(cfg);
    api.delete_deployment_rule(gate_id.to_string(), rule_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete deployment rule: {e:?}"))?;
    eprintln!("Deployment rule {rule_id} deleted from gate {gate_id}.");
    Ok(())
}
