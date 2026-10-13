use anyhow::Result;
use datadog_api_client::datadogV2::api_experiments::{
    ExperimentsAPI, GetMetricOptionalParams, GetSubjectTypeOptionalParams,
    ListExperimentProtocolsOptionalParams, ListExperimentsOptionalParams,
    ListMetricCollectionsOptionalParams, ListMetricsOptionalParams, ListSubjectTypesOptionalParams,
    RefreshExperimentResultsOptionalParams, StartExperimentOptionalParams,
};
use datadog_api_client::datadogV2::model::{
    ExperimentsAnalysisPlanWriteV2Request, ExperimentsCancelExperimentV2Request,
    ExperimentsCancelExperimentV2RequestData, ExperimentsCancelExperimentV2RequestDataAttributes,
    ExperimentsCancelExperimentV2RequestDataType, ExperimentsConcludeExperimentV2Request,
    ExperimentsConcludeExperimentV2RequestData,
    ExperimentsConcludeExperimentV2RequestDataAttributes,
    ExperimentsConcludeExperimentV2RequestDataType,
    ExperimentsCreateExperimentMetricGroupV2Request, ExperimentsCreateExperimentV2Request,
    ExperimentsCreateMetricCollectionV2Request, ExperimentsCreateMetricV2Request,
    ExperimentsCreateSubjectTypeV2Request, ExperimentsPatchExperimentMetricGroupV2Request,
    ExperimentsPatchExperimentV2Request, ExperimentsPatchMetricCollectionV2Request,
    ExperimentsPatchSubjectTypeV2Request, ExperimentsUpdateMetricV2Request,
};

use crate::config::Config;
use crate::formatter;
use crate::util;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_api(cfg: &Config) -> ExperimentsAPI {
    crate::make_api!(ExperimentsAPI, cfg)
}

fn parse_id(id: &str, what: &str) -> Result<uuid::Uuid> {
    id.parse::<uuid::Uuid>()
        .map_err(|e| anyhow::anyhow!("invalid {what} ID: {e:?}"))
}

/// Shared list filters for metrics, metric collections, and subject types.
pub struct ListFilters {
    pub search: Option<String>,
    pub sort: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

// ---------------------------------------------------------------------------
// Experiments
// ---------------------------------------------------------------------------

pub async fn list(
    cfg: &Config,
    filters: ListFilters,
    status: Vec<String>,
    tags: Vec<String>,
) -> Result<()> {
    let mut params = ListExperimentsOptionalParams::default();
    if let Some(v) = filters.search {
        params = params.search(v);
    }
    if let Some(v) = filters.sort {
        params = params.sort(v);
    }
    if let Some(v) = filters.limit {
        params = params.page_limit(v);
    }
    if let Some(v) = filters.offset {
        params = params.page_offset(v);
    }
    if !status.is_empty() {
        params = params.status(status);
    }
    if !tags.is_empty() {
        params = params.tags(tags);
    }
    let resp = make_api(cfg)
        .list_experiments(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list experiments: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn get(cfg: &Config, experiment_id: &str) -> Result<()> {
    let id = parse_id(experiment_id, "experiment")?;
    let resp = make_api(cfg)
        .get_experiment(id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get experiment: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn create(cfg: &Config, file: &str) -> Result<()> {
    let body: ExperimentsCreateExperimentV2Request = util::read_json_file(file)?;
    let resp = make_api(cfg)
        .create_experiment(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create experiment: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn update(cfg: &Config, experiment_id: &str, file: &str) -> Result<()> {
    let id = parse_id(experiment_id, "experiment")?;
    let body: ExperimentsPatchExperimentV2Request = util::read_json_file(file)?;
    let resp = make_api(cfg)
        .patch_experiment(id, body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update experiment: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn delete(cfg: &Config, experiment_id: &str) -> Result<()> {
    let id = parse_id(experiment_id, "experiment")?;
    make_api(cfg)
        .delete_experiment(id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete experiment: {e:?}"))?;
    eprintln!("Experiment {experiment_id} deleted.");
    Ok(())
}

pub async fn start(cfg: &Config, experiment_id: &str) -> Result<()> {
    let id = parse_id(experiment_id, "experiment")?;
    make_api(cfg)
        .start_experiment(id, StartExperimentOptionalParams::default())
        .await
        .map_err(|e| anyhow::anyhow!("failed to start experiment: {e:?}"))?;
    eprintln!("Experiment {experiment_id} started.");
    Ok(())
}

pub async fn conclude(cfg: &Config, experiment_id: &str, decision_variant: &str) -> Result<()> {
    let id = parse_id(experiment_id, "experiment")?;
    let body = ExperimentsConcludeExperimentV2Request::new(
        ExperimentsConcludeExperimentV2RequestData::new(
            ExperimentsConcludeExperimentV2RequestDataAttributes::new(decision_variant.to_string()),
            ExperimentsConcludeExperimentV2RequestDataType::CONCLUDE_EXPERIMENT_REQUEST,
        ),
    );
    make_api(cfg)
        .conclude_experiment(id, body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to conclude experiment: {e:?}"))?;
    eprintln!("Experiment {experiment_id} concluded with variant {decision_variant}.");
    Ok(())
}

pub async fn cancel(cfg: &Config, experiment_id: &str, reason: &str) -> Result<()> {
    let id = parse_id(experiment_id, "experiment")?;
    let body =
        ExperimentsCancelExperimentV2Request::new(ExperimentsCancelExperimentV2RequestData::new(
            ExperimentsCancelExperimentV2RequestDataAttributes::new(reason.to_string()),
            ExperimentsCancelExperimentV2RequestDataType::CANCEL_EXPERIMENT_REQUEST,
        ));
    make_api(cfg)
        .cancel_experiment(id, body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to cancel experiment: {e:?}"))?;
    eprintln!("Experiment {experiment_id} cancelled.");
    Ok(())
}

pub async fn diagnostics(cfg: &Config, experiment_id: &str) -> Result<()> {
    let id = parse_id(experiment_id, "experiment")?;
    let resp = make_api(cfg)
        .get_experiment_diagnostics(id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get experiment diagnostics: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn traffic_summary(cfg: &Config, experiment_id: &str) -> Result<()> {
    let id = parse_id(experiment_id, "experiment")?;
    let resp = make_api(cfg)
        .get_experiment_traffic_summary(id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get experiment traffic summary: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn results(cfg: &Config, experiment_id: &str) -> Result<()> {
    let id = parse_id(experiment_id, "experiment")?;
    let resp = make_api(cfg)
        .get_experiment_results(id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get experiment results: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn refresh(cfg: &Config, experiment_id: &str, full: bool) -> Result<()> {
    let id = parse_id(experiment_id, "experiment")?;
    let mut params = RefreshExperimentResultsOptionalParams::default();
    if full {
        params = params.full_refresh(true);
    }
    let resp = make_api(cfg)
        .refresh_experiment_results(id, params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to refresh experiment results: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---------------------------------------------------------------------------
// Analysis plan
// ---------------------------------------------------------------------------

pub async fn analysis_plan_get(cfg: &Config, experiment_id: &str) -> Result<()> {
    let id = parse_id(experiment_id, "experiment")?;
    let resp = make_api(cfg)
        .get_experiment_analysis_plan(id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get analysis plan: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn analysis_plan_update(cfg: &Config, experiment_id: &str, file: &str) -> Result<()> {
    let id = parse_id(experiment_id, "experiment")?;
    let body: ExperimentsAnalysisPlanWriteV2Request = util::read_json_file(file)?;
    let resp = make_api(cfg)
        .update_experiment_analysis_plan_attributes(id, body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update analysis plan: {e:?}"))?;
    formatter::output(cfg, &resp)
}

// ---------------------------------------------------------------------------
// Metric groups (metrics attached to an experiment)
// ---------------------------------------------------------------------------

pub async fn metric_groups_list(cfg: &Config, experiment_id: &str) -> Result<()> {
    let id = parse_id(experiment_id, "experiment")?;
    let resp = make_api(cfg)
        .list_experiment_metric_groups(id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list metric groups: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn metric_groups_create(cfg: &Config, experiment_id: &str, file: &str) -> Result<()> {
    let id = parse_id(experiment_id, "experiment")?;
    let body: ExperimentsCreateExperimentMetricGroupV2Request = util::read_json_file(file)?;
    let resp = make_api(cfg)
        .create_experiment_metric_group(id, body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create metric group: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn metric_groups_create_from_collection(
    cfg: &Config,
    experiment_id: &str,
    metric_collection_id: &str,
) -> Result<()> {
    let id = parse_id(experiment_id, "experiment")?;
    let collection_id = parse_id(metric_collection_id, "metric collection")?;
    let resp = make_api(cfg)
        .create_experiment_metric_group_from_collection(id, collection_id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create metric group from collection: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn metric_groups_update(
    cfg: &Config,
    experiment_id: &str,
    metric_group_id: &str,
    file: &str,
) -> Result<()> {
    let id = parse_id(experiment_id, "experiment")?;
    let group_id = parse_id(metric_group_id, "metric group")?;
    let body: ExperimentsPatchExperimentMetricGroupV2Request = util::read_json_file(file)?;
    let resp = make_api(cfg)
        .update_experiment_metric_group(id, group_id, body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update metric group: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn metric_groups_delete(
    cfg: &Config,
    experiment_id: &str,
    metric_group_id: &str,
) -> Result<()> {
    let id = parse_id(experiment_id, "experiment")?;
    let group_id = parse_id(metric_group_id, "metric group")?;
    make_api(cfg)
        .delete_experiment_metric_group(id, group_id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete metric group: {e:?}"))?;
    eprintln!("Metric group {metric_group_id} deleted from experiment {experiment_id}.");
    Ok(())
}

// ---------------------------------------------------------------------------
// Metrics
// ---------------------------------------------------------------------------

pub async fn metrics_list(cfg: &Config, filters: ListFilters) -> Result<()> {
    let mut params = ListMetricsOptionalParams::default();
    if let Some(v) = filters.search {
        params = params.search(v);
    }
    if let Some(v) = filters.sort {
        params = params.sort(v);
    }
    if let Some(v) = filters.limit {
        params = params.page_limit(v);
    }
    if let Some(v) = filters.offset {
        params = params.page_offset(v);
    }
    let resp = make_api(cfg)
        .list_metrics(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list experiment metrics: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn metrics_get(cfg: &Config, metric_id: &str) -> Result<()> {
    let id = parse_id(metric_id, "metric")?;
    let resp = make_api(cfg)
        .get_metric(id, GetMetricOptionalParams::default())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get experiment metric: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn metrics_create(cfg: &Config, file: &str) -> Result<()> {
    let body: ExperimentsCreateMetricV2Request = util::read_json_file(file)?;
    let resp = make_api(cfg)
        .create_metric(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create experiment metric: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn metrics_update(cfg: &Config, metric_id: &str, file: &str) -> Result<()> {
    let id = parse_id(metric_id, "metric")?;
    let body: ExperimentsUpdateMetricV2Request = util::read_json_file(file)?;
    let resp = make_api(cfg)
        .update_metric(id, body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update experiment metric: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn metrics_delete(cfg: &Config, metric_id: &str) -> Result<()> {
    let id = parse_id(metric_id, "metric")?;
    make_api(cfg)
        .delete_metric(id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete experiment metric: {e:?}"))?;
    eprintln!("Metric {metric_id} deleted.");
    Ok(())
}

// ---------------------------------------------------------------------------
// Metric collections
// ---------------------------------------------------------------------------

pub async fn metric_collections_list(cfg: &Config, filters: ListFilters) -> Result<()> {
    let mut params = ListMetricCollectionsOptionalParams::default();
    if let Some(v) = filters.search {
        params = params.search(v);
    }
    if let Some(v) = filters.sort {
        params = params.sort(v);
    }
    if let Some(v) = filters.limit {
        params = params.page_limit(v);
    }
    if let Some(v) = filters.offset {
        params = params.page_offset(v);
    }
    let resp = make_api(cfg)
        .list_metric_collections(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list metric collections: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn metric_collections_get(cfg: &Config, metric_collection_id: &str) -> Result<()> {
    let id = parse_id(metric_collection_id, "metric collection")?;
    let resp = make_api(cfg)
        .get_metric_collection(id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get metric collection: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn metric_collections_create(cfg: &Config, file: &str) -> Result<()> {
    let body: ExperimentsCreateMetricCollectionV2Request = util::read_json_file(file)?;
    let resp = make_api(cfg)
        .create_metric_collection(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create metric collection: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn metric_collections_update(
    cfg: &Config,
    metric_collection_id: &str,
    file: &str,
) -> Result<()> {
    let id = parse_id(metric_collection_id, "metric collection")?;
    let body: ExperimentsPatchMetricCollectionV2Request = util::read_json_file(file)?;
    let resp = make_api(cfg)
        .update_metric_collection(id, body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update metric collection: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn metric_collections_delete(cfg: &Config, metric_collection_id: &str) -> Result<()> {
    let id = parse_id(metric_collection_id, "metric collection")?;
    make_api(cfg)
        .delete_metric_collection(id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete metric collection: {e:?}"))?;
    eprintln!("Metric collection {metric_collection_id} deleted.");
    Ok(())
}

// ---------------------------------------------------------------------------
// Subject types
// ---------------------------------------------------------------------------

pub async fn subject_types_list(cfg: &Config, filters: ListFilters) -> Result<()> {
    let mut params = ListSubjectTypesOptionalParams::default();
    if let Some(v) = filters.search {
        params = params.search(v);
    }
    if let Some(v) = filters.sort {
        params = params.sort(v);
    }
    if let Some(v) = filters.limit {
        params = params.page_limit(v);
    }
    if let Some(v) = filters.offset {
        params = params.page_offset(v);
    }
    let resp = make_api(cfg)
        .list_subject_types(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list subject types: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn subject_types_get(cfg: &Config, subject_type_id: &str) -> Result<()> {
    let id = parse_id(subject_type_id, "subject type")?;
    let resp = make_api(cfg)
        .get_subject_type(id, GetSubjectTypeOptionalParams::default())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get subject type: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn subject_types_create(cfg: &Config, file: &str) -> Result<()> {
    let body: ExperimentsCreateSubjectTypeV2Request = util::read_json_file(file)?;
    let resp = make_api(cfg)
        .create_subject_type(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create subject type: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn subject_types_update(cfg: &Config, subject_type_id: &str, file: &str) -> Result<()> {
    let id = parse_id(subject_type_id, "subject type")?;
    let body: ExperimentsPatchSubjectTypeV2Request = util::read_json_file(file)?;
    let resp = make_api(cfg)
        .patch_subject_type(id, body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update subject type: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn subject_types_delete(cfg: &Config, subject_type_id: &str) -> Result<()> {
    let id = parse_id(subject_type_id, "subject type")?;
    make_api(cfg)
        .delete_subject_type(id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete subject type: {e:?}"))?;
    eprintln!("Subject type {subject_type_id} deleted.");
    Ok(())
}

pub async fn subject_types_set_default(cfg: &Config, subject_type_id: &str) -> Result<()> {
    let id = parse_id(subject_type_id, "subject type")?;
    make_api(cfg)
        .set_default_subject_type(id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to set default subject type: {e:?}"))?;
    eprintln!("Subject type {subject_type_id} set as default.");
    Ok(())
}

// ---------------------------------------------------------------------------
// Protocols (read-only experiment templates)
// ---------------------------------------------------------------------------

pub async fn protocols_list(
    cfg: &Config,
    query: Option<String>,
    sort: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<()> {
    let mut params = ListExperimentProtocolsOptionalParams::default();
    if let Some(v) = query {
        params = params.filter_query(v);
    }
    if let Some(v) = sort {
        params = params.sort(v);
    }
    if let Some(v) = limit {
        params = params.page_limit(v);
    }
    if let Some(v) = offset {
        params = params.page_offset(v);
    }
    let resp = make_api(cfg)
        .list_experiment_protocols(params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list experiment protocols: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn protocols_get(cfg: &Config, protocol_id: &str) -> Result<()> {
    let id = parse_id(protocol_id, "protocol")?;
    let resp = make_api(cfg)
        .get_experiment_protocol(id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get experiment protocol: {e:?}"))?;
    formatter::output(cfg, &resp)
}

#[cfg(test)]
mod tests {
    use super::ListFilters;
    use crate::test_support::*;
    use mockito::Matcher;

    const EXP_ID: &str = "00000000-0000-4000-8000-000000000001";
    const OTHER_ID: &str = "00000000-0000-4000-8000-000000000002";

    fn no_filters() -> ListFilters {
        ListFilters {
            search: None,
            sort: None,
            limit: None,
            offset: None,
        }
    }

    async fn mock_ok(
        s: &mut mockito::Server,
        method: &str,
        path: &str,
        body: &str,
    ) -> mockito::Mock {
        s.mock(method, path)
            .match_query(Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create_async()
            .await
    }

    #[tokio::test]
    async fn test_list_sends_filters() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let m = s
            .mock("GET", "/api/v2/experiments")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("search".into(), "checkout".into()),
                Matcher::UrlEncoded("page[limit]".into(), "5".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data":[]}"#)
            .create_async()
            .await;
        let filters = ListFilters {
            search: Some("checkout".into()),
            limit: Some(5),
            ..no_filters()
        };
        let result = super::list(&cfg, filters, vec!["running".into()], vec![]).await;
        assert!(result.is_ok(), "list failed: {:?}", result.err());
        m.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_list_error() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        s.mock("GET", Matcher::Any)
            .match_query(Matcher::Any)
            .with_status(403)
            .with_body(r#"{"errors":["forbidden"]}"#)
            .create_async()
            .await;
        let result = super::list(&cfg, no_filters(), vec![], vec![]).await;
        assert!(result.is_err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_get_rejects_invalid_id() {
        let _lock = lock_env().await;
        let s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let err = super::get(&cfg, "not-a-uuid").await.unwrap_err();
        assert!(err.to_string().contains("invalid experiment ID"), "{err}");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_get_not_found() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        s.mock("GET", format!("/api/v2/experiments/{EXP_ID}").as_str())
            .with_status(404)
            .with_body(r#"{"errors":["not found"]}"#)
            .create_async()
            .await;
        assert!(super::get(&cfg, EXP_ID).await.is_err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_create_missing_file() {
        let _lock = lock_env().await;
        let s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        assert!(super::create(&cfg, "/nonexistent/experiment.json")
            .await
            .is_err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_delete() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let m = s
            .mock("DELETE", format!("/api/v2/experiments/{EXP_ID}").as_str())
            .with_status(204)
            .create_async()
            .await;
        let result = super::delete(&cfg, EXP_ID).await;
        assert!(result.is_ok(), "delete failed: {:?}", result.err());
        m.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_start() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let m = mock_ok(
            &mut s,
            "POST",
            &format!("/api/v2/experiments/{EXP_ID}/start"),
            "",
        )
        .await;
        let result = super::start(&cfg, EXP_ID).await;
        assert!(result.is_ok(), "start failed: {:?}", result.err());
        m.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_start_readiness_failure() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        s.mock(
            "POST",
            format!("/api/v2/experiments/{EXP_ID}/start").as_str(),
        )
        .with_status(400)
        .with_body(r#"{"errors":["experiment has no primary metric"]}"#)
        .create_async()
        .await;
        assert!(super::start(&cfg, EXP_ID).await.is_err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_conclude_sends_decision_variant() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let m = s
            .mock(
                "POST",
                format!("/api/v2/experiments/{EXP_ID}/conclude").as_str(),
            )
            .match_body(Matcher::PartialJson(serde_json::json!({"data":{
                "type":"conclude-experiment-request",
                "attributes":{"decision_variant_key":"treatment"}
            }})))
            .with_status(200)
            .create_async()
            .await;
        let result = super::conclude(&cfg, EXP_ID, "treatment").await;
        assert!(result.is_ok(), "conclude failed: {:?}", result.err());
        m.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_cancel_sends_reason() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let m = s
            .mock(
                "POST",
                format!("/api/v2/experiments/{EXP_ID}/cancel").as_str(),
            )
            .match_body(Matcher::PartialJson(serde_json::json!({"data":{
                "type":"cancel-experiment-request",
                "attributes":{"reason":"bad rollout"}
            }})))
            .with_status(200)
            .create_async()
            .await;
        let result = super::cancel(&cfg, EXP_ID, "bad rollout").await;
        assert!(result.is_ok(), "cancel failed: {:?}", result.err());
        m.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_diagnostics_error() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        s.mock(
            "GET",
            format!("/api/v2/experiments/{EXP_ID}/diagnostics").as_str(),
        )
        .with_status(500)
        .with_body(r#"{"errors":["internal"]}"#)
        .create_async()
        .await;
        assert!(super::diagnostics(&cfg, EXP_ID).await.is_err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_metric_groups_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let m = mock_ok(
            &mut s,
            "GET",
            &format!("/api/v2/experiments/{EXP_ID}/metric-groups"),
            r#"{"data":[]}"#,
        )
        .await;
        let result = super::metric_groups_list(&cfg, EXP_ID).await;
        assert!(
            result.is_ok(),
            "metric groups list failed: {:?}",
            result.err()
        );
        m.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_metric_groups_delete() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let m = s
            .mock(
                "DELETE",
                format!("/api/v2/experiments/{EXP_ID}/metric-groups/{OTHER_ID}").as_str(),
            )
            .with_status(204)
            .create_async()
            .await;
        let result = super::metric_groups_delete(&cfg, EXP_ID, OTHER_ID).await;
        assert!(
            result.is_ok(),
            "metric group delete failed: {:?}",
            result.err()
        );
        m.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_metric_groups_delete_rejects_invalid_group_id() {
        let _lock = lock_env().await;
        let s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let err = super::metric_groups_delete(&cfg, EXP_ID, "bad")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("invalid metric group ID"), "{err}");
        cleanup_env();
    }

    #[tokio::test]
    async fn test_metric_groups_create_invalid_body() {
        let _lock = lock_env().await;
        let s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let file = std::env::temp_dir().join(format!("pup-exp-group-{}.json", std::process::id()));
        std::fs::write(&file, r#"{"not":"a metric group"}"#).unwrap();
        let result = super::metric_groups_create(&cfg, EXP_ID, file.to_str().unwrap()).await;
        std::fs::remove_file(&file).unwrap();
        assert!(result.is_err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_metrics_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let m = mock_ok(
            &mut s,
            "GET",
            "/api/v2/experiments/metrics",
            r#"{"data":[]}"#,
        )
        .await;
        let result = super::metrics_list(&cfg, no_filters()).await;
        assert!(result.is_ok(), "metrics list failed: {:?}", result.err());
        m.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_metrics_delete_error() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        s.mock(
            "DELETE",
            format!("/api/v2/experiments/metrics/{OTHER_ID}").as_str(),
        )
        .with_status(409)
        .with_body(r#"{"errors":["metric is in use"]}"#)
        .create_async()
        .await;
        assert!(super::metrics_delete(&cfg, OTHER_ID).await.is_err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_metric_collections_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let m = mock_ok(
            &mut s,
            "GET",
            "/api/v2/experiments/metric-collections",
            r#"{"data":[]}"#,
        )
        .await;
        let result = super::metric_collections_list(&cfg, no_filters()).await;
        assert!(
            result.is_ok(),
            "collections list failed: {:?}",
            result.err()
        );
        m.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_subject_types_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let m = mock_ok(
            &mut s,
            "GET",
            "/api/v2/experiments/subject-types",
            r#"{"data":[]}"#,
        )
        .await;
        let result = super::subject_types_list(&cfg, no_filters()).await;
        assert!(
            result.is_ok(),
            "subject types list failed: {:?}",
            result.err()
        );
        m.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_subject_types_set_default() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let m = mock_ok(
            &mut s,
            "POST",
            &format!("/api/v2/experiments/subject-types/{OTHER_ID}/default"),
            "",
        )
        .await;
        let result = super::subject_types_set_default(&cfg, OTHER_ID).await;
        assert!(result.is_ok(), "set default failed: {:?}", result.err());
        m.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_protocols_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let m = mock_ok(
            &mut s,
            "GET",
            "/api/v2/experiments/protocols",
            r#"{"data":[]}"#,
        )
        .await;
        let result = super::protocols_list(&cfg, None, None, None, None).await;
        assert!(result.is_ok(), "protocols list failed: {:?}", result.err());
        m.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_protocols_get_rejects_invalid_id() {
        let _lock = lock_env().await;
        let s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        let err = super::protocols_get(&cfg, "123").await.unwrap_err();
        assert!(err.to_string().contains("invalid protocol ID"), "{err}");
        cleanup_env();
    }
}
