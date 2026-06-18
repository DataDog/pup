use anyhow::{bail, Result};
use datadog_api_client::datadogV2::api_incidents::{
    CreateGlobalIncidentHandleOptionalParams, GetIncidentOptionalParams,
    ImportIncidentOptionalParams, IncidentsAPI, ListGlobalIncidentHandlesOptionalParams,
    ListIncidentAttachmentsOptionalParams, SearchIncidentsOptionalParams,
    UpdateGlobalIncidentHandleOptionalParams,
};
use datadog_api_client::datadogV2::model::{IncidentImportRequest, IncidentSearchSortOrder};

use crate::config::Config;
use crate::formatter;
use crate::util;

// ---------------------------------------------------------------------------
// Helper: build an IncidentsAPI with bearer-token support
// ---------------------------------------------------------------------------

fn make_api(cfg: &Config) -> IncidentsAPI {
    crate::make_api!(IncidentsAPI, cfg)
}

// ---------------------------------------------------------------------------
// Core incident operations
// ---------------------------------------------------------------------------

pub async fn list(cfg: &Config, query: Option<String>, limit: i64) -> Result<()> {
    let api = make_api(cfg);
    let params = SearchIncidentsOptionalParams::default()
        .page_size(limit)
        .sort(IncidentSearchSortOrder::CREATED_DESCENDING);
    let q = query.unwrap_or_else(|| "state:active".to_string());
    let resp = api
        .search_incidents(q, params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list incidents: {:?}", e))?;
    formatter::output(cfg, &resp)?;
    Ok(())
}

pub async fn get(cfg: &Config, incident_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let resp = api
        .get_incident(
            incident_id.to_string(),
            GetIncidentOptionalParams::default(),
        )
        .await
        .map_err(|e| anyhow::anyhow!("failed to get incident: {:?}", e))?;
    formatter::output(cfg, &resp)
}

// ---------------------------------------------------------------------------
// Attachments
// ---------------------------------------------------------------------------

pub async fn attachments_list(cfg: &Config, incident_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let resp = api
        .list_incident_attachments(
            incident_id.to_string(),
            ListIncidentAttachmentsOptionalParams::default(),
        )
        .await
        .map_err(|e| anyhow::anyhow!("failed to list incident attachments: {:?}", e))?;
    formatter::output(cfg, &resp)
}

pub async fn attachments_delete(
    cfg: &Config,
    incident_id: &str,
    attachment_id: &str,
) -> Result<()> {
    let url = format!(
        "{}/api/v2/incidents/{}/attachments/{}",
        cfg.api_base_url(),
        incident_id,
        attachment_id
    );
    let client = reqwest::Client::new();
    let mut req = client.delete(&url);

    if let Some(token) = &cfg.access_token {
        req = req.header("Authorization", format!("Bearer {token}"));
    } else if let (Some(api_key), Some(app_key)) = (&cfg.api_key, &cfg.app_key) {
        req = req
            .header("DD-API-KEY", api_key.as_str())
            .header("DD-APPLICATION-KEY", app_key.as_str());
    } else {
        bail!("no authentication configured");
    }

    let resp = req.header("Accept", "application/json").send().await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        bail!("failed to delete incident attachment (HTTP {status}): {body}");
    }
    println!("Incident attachment {attachment_id} deleted from incident {incident_id}.");
    Ok(())
}

// ---------------------------------------------------------------------------
// Global incident settings
// ---------------------------------------------------------------------------

pub async fn settings_get(cfg: &Config) -> Result<()> {
    let api = make_api(cfg);
    let resp = api
        .get_global_incident_settings()
        .await
        .map_err(|e| anyhow::anyhow!("failed to get incident settings: {:?}", e))?;
    formatter::output(cfg, &resp)
}

pub async fn settings_update(cfg: &Config, file: &str) -> Result<()> {
    let body = util::read_json_file(file)?;
    let api = make_api(cfg);
    let resp = api
        .update_global_incident_settings(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update incident settings: {:?}", e))?;
    formatter::output(cfg, &resp)
}

// ---------------------------------------------------------------------------
// Global incident handles
// ---------------------------------------------------------------------------

pub async fn handles_list(cfg: &Config) -> Result<()> {
    let api = make_api(cfg);
    let params = ListGlobalIncidentHandlesOptionalParams::default();
    let resp = api
        .list_global_incident_handles(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list incident handles: {:?}", e))?;
    formatter::output(cfg, &resp)
}

pub async fn handles_create(cfg: &Config, file: &str) -> Result<()> {
    let body = util::read_json_file(file)?;
    let api = make_api(cfg);
    let resp = api
        .create_global_incident_handle(body, CreateGlobalIncidentHandleOptionalParams::default())
        .await
        .map_err(|e| anyhow::anyhow!("failed to create incident handle: {:?}", e))?;
    formatter::output(cfg, &resp)
}

pub async fn handles_update(cfg: &Config, file: &str) -> Result<()> {
    let body = util::read_json_file(file)?;
    let api = make_api(cfg);
    let resp = api
        .update_global_incident_handle(body, UpdateGlobalIncidentHandleOptionalParams::default())
        .await
        .map_err(|e| anyhow::anyhow!("failed to update incident handle: {:?}", e))?;
    formatter::output(cfg, &resp)
}

pub async fn handles_delete(cfg: &Config, _handle_id: &str) -> Result<()> {
    let api = make_api(cfg);
    api.delete_global_incident_handle()
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete incident handle: {:?}", e))?;
    println!("Incident handle deleted.");
    Ok(())
}

// ---------------------------------------------------------------------------
// Postmortem templates
// ---------------------------------------------------------------------------

pub async fn postmortem_templates_list(cfg: &Config) -> Result<()> {
    let api = make_api(cfg);
    let resp = api
        .list_incident_postmortem_templates()
        .await
        .map_err(|e| anyhow::anyhow!("failed to list postmortem templates: {:?}", e))?;
    formatter::output(cfg, &resp)
}

pub async fn postmortem_templates_get(cfg: &Config, template_id: &str) -> Result<()> {
    let api = make_api(cfg);
    let resp = api
        .get_incident_postmortem_template(template_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get postmortem template: {:?}", e))?;
    formatter::output(cfg, &resp)
}

pub async fn postmortem_templates_create(cfg: &Config, file: &str) -> Result<()> {
    let body = util::read_json_file(file)?;
    let api = make_api(cfg);
    let resp = api
        .create_incident_postmortem_template(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create postmortem template: {:?}", e))?;
    formatter::output(cfg, &resp)
}

pub async fn postmortem_templates_update(
    cfg: &Config,
    template_id: &str,
    file: &str,
) -> Result<()> {
    let body = util::read_json_file(file)?;
    let api = make_api(cfg);
    let resp = api
        .update_incident_postmortem_template(template_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update postmortem template: {:?}", e))?;
    formatter::output(cfg, &resp)
}

pub async fn postmortem_templates_delete(cfg: &Config, template_id: &str) -> Result<()> {
    let api = make_api(cfg);
    api.delete_incident_postmortem_template(template_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete postmortem template: {:?}", e))?;
    println!("Postmortem template {template_id} deleted.");
    Ok(())
}

// ---- Import ----

pub async fn import(cfg: &Config, file: &str) -> Result<()> {
    let body: IncidentImportRequest = util::read_json_file(file)?;
    let api = make_api(cfg);
    let resp = api
        .import_incident(body, ImportIncidentOptionalParams::default())
        .await
        .map_err(|e| anyhow::anyhow!("failed to import incident: {e:?}"))?;
    formatter::output(cfg, &resp)
}

#[cfg(test)]
mod tests {

    use crate::test_support::*;

    #[tokio::test]
    async fn test_incidents_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::list(&cfg, None, 10).await;
        cleanup_env();
    }

    /// When a `--query` is supplied, it must be forwarded as the
    /// `query` URL parameter on the incidents-search request.
    #[tokio::test]
    async fn test_incidents_list_forwards_query() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());

        // Strict mock: only matches when the request URL contains
        // `query=state:resolved`. Any other query string falls through
        // and the request fails.
        let _mock = s
            .mock("GET", mockito::Matcher::Any)
            .match_query(mockito::Matcher::UrlEncoded(
                "query".into(),
                "state:resolved".into(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": {}}"#)
            .create_async()
            .await;

        let result = super::list(&cfg, Some("state:resolved".to_string()), 10).await;
        assert!(
            result.is_ok(),
            "list with --query=state:resolved should forward the query: {:?}",
            result.err()
        );
        cleanup_env();
    }

    /// When `--query` is omitted, the default `state:active` must still be
    /// sent on the wire, preserving the previous CLI behavior.
    #[tokio::test]
    async fn test_incidents_list_defaults_to_state_active() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());

        let _mock = s
            .mock("GET", mockito::Matcher::Any)
            .match_query(mockito::Matcher::UrlEncoded(
                "query".into(),
                "state:active".into(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": {}}"#)
            .create_async()
            .await;

        let result = super::list(&cfg, None, 10).await;
        assert!(
            result.is_ok(),
            "list with no --query should default to state:active: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_incidents_get() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": {}}"#).await;
        let _ = super::get(&cfg, "inc1").await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_incidents_attachments_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let result = super::attachments_list(&cfg, "inc1").await;
        assert!(
            result.is_ok(),
            "attachments_list should succeed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_incidents_attachments_list_error() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let _mock = s
            .mock("GET", mockito::Matcher::Any)
            .match_query(mockito::Matcher::Any)
            .with_status(404)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors":["Not Found"]}"#)
            .create_async()
            .await;
        let result = super::attachments_list(&cfg, "missing").await;
        assert!(result.is_err(), "attachments_list should fail on 404");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_incidents_attachments_delete() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let _mock = s
            .mock("DELETE", "/api/v2/incidents/inc1/attachments/att1")
            .with_status(204)
            .create_async()
            .await;
        let result = super::attachments_delete(&cfg, "inc1", "att1").await;
        assert!(
            result.is_ok(),
            "attachments_delete should succeed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_incidents_attachments_delete_error() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let _mock = s
            .mock("DELETE", "/api/v2/incidents/inc1/attachments/missing")
            .with_status(404)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors":["Not Found"]}"#)
            .create_async()
            .await;
        let result = super::attachments_delete(&cfg, "inc1", "missing").await;
        assert!(result.is_err(), "attachments_delete should fail on 404");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_incidents_settings_get() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": {}}"#).await;
        let _ = super::settings_get(&cfg).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_incidents_handles_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::handles_list(&cfg).await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_incidents_postmortem_templates_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": []}"#).await;
        let _ = super::postmortem_templates_list(&cfg).await;
        cleanup_env();
    }
}
