use anyhow::Result;

use crate::config::Config;
use crate::formatter;
use crate::raw_client;
use crate::util;
use crate::util_ext;

// ---- Custom Costs ----

pub async fn custom_costs_list(
    cfg: &Config,
    page_size: Option<i64>,
    status: Option<String>,
    sort: Option<String>,
) -> Result<()> {
    let mut params: Vec<(String, String)> = Vec::new();
    if let Some(ps) = page_size {
        params.push(("page[size]".into(), ps.to_string()));
    }
    if let Some(s) = status {
        params.push(("filter[status]".into(), s));
    }
    if let Some(s) = sort {
        params.push(("sort".into(), s));
    }
    let q: Vec<(&str, &str)> = params
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let value = raw_client::raw_get(cfg, "/api/v2/cost/custom_costs", &q).await?;
    formatter::output(cfg, &value)
}

pub async fn custom_costs_get(cfg: &Config, file_id: &str) -> Result<()> {
    let path = format!(
        "/api/v2/cost/custom_costs/{}",
        util_ext::percent_encode(file_id)
    );
    let value = raw_client::raw_get(cfg, &path, &[]).await?;
    formatter::output(cfg, &value)
}

pub async fn custom_costs_upload(cfg: &Config, file: &str, version: Option<String>) -> Result<()> {
    let mut params: Vec<(String, String)> = Vec::new();
    if let Some(v) = version {
        params.push(("version".into(), v));
    }
    let q: Vec<(&str, &str)> = params
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    let file_content =
        std::fs::read(file).map_err(|e| anyhow::anyhow!("failed to read '{file}': {e}"))?;
    let filename = std::path::Path::new(file)
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow::anyhow!("file path '{file}' contains non-UTF-8 characters"))?;

    // Use a random boundary to avoid collisions with file content.
    let boundary = uuid::Uuid::new_v4().simple().to_string();
    let mut body = Vec::new();
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; \
             filename=\"{filename}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(&file_content);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    let content_type = format!("multipart/form-data; boundary={boundary}");
    // The API may return an empty body on success or a JSON status object.
    let resp = raw_client::raw_request(
        cfg,
        "PUT",
        "/api/v2/cost/custom_costs",
        &q,
        Some(body),
        Some(&content_type),
        "application/json",
        &[],
    )
    .await?;

    if resp.bytes.is_empty() {
        eprintln!("Upload accepted.");
        return Ok(());
    }
    let value: serde_json::Value = serde_json::from_slice(&resp.bytes)
        .map_err(|e| anyhow::anyhow!("failed to parse upload response: {e}"))?;
    formatter::output(cfg, &value)
}

pub async fn custom_costs_delete(cfg: &Config, file_id: &str) -> Result<()> {
    let path = format!(
        "/api/v2/cost/custom_costs/{}",
        util_ext::percent_encode(file_id)
    );
    raw_client::raw_delete(cfg, &path).await?;
    eprintln!("Custom cost file '{file_id}' deleted.");
    Ok(())
}

// ---- Tag Descriptions ----

pub async fn tag_desc_list(cfg: &Config, cloud: Option<String>) -> Result<()> {
    let mut params: Vec<(String, String)> = Vec::new();
    if let Some(c) = cloud {
        params.push(("filter[cloud]".into(), c));
    }
    let q: Vec<(&str, &str)> = params
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let value = raw_client::raw_get(cfg, "/api/v2/cost/tag_descriptions", &q).await?;
    formatter::output(cfg, &value)
}

pub async fn tag_desc_get(cfg: &Config, tag_key: &str, cloud: Option<String>) -> Result<()> {
    let mut params: Vec<(String, String)> = vec![("tag_key".into(), tag_key.to_string())];
    if let Some(c) = cloud {
        params.push(("filter[cloud]".into(), c));
    }
    let q: Vec<(&str, &str)> = params
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let value = raw_client::raw_get(cfg, "/api/v2/cost/tag_description", &q).await?;
    formatter::output(cfg, &value)
}

pub async fn tag_desc_generate(cfg: &Config, tag_key: &str) -> Result<()> {
    let q = [("tag_key", tag_key)];
    let value = raw_client::raw_get(cfg, "/api/v2/cost/tag_description/generate", &q).await?;
    formatter::output(cfg, &value)
}

pub async fn tag_desc_upsert(
    cfg: &Config,
    tag_key: &str,
    description: &str,
    cloud: Option<String>,
) -> Result<()> {
    let mut params: Vec<(String, String)> = vec![
        ("tag_key".into(), tag_key.to_string()),
        ("description".into(), description.to_string()),
    ];
    if let Some(c) = cloud {
        params.push(("cloud".into(), c));
    }
    let q: Vec<(&str, &str)> = params
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let resp = raw_client::raw_request(
        cfg,
        "PUT",
        "/api/v2/cost/tag_descriptions",
        &q,
        None,
        None,
        "application/json",
        &[],
    )
    .await?;
    if resp.bytes.is_empty() {
        eprintln!("Tag description for '{tag_key}' updated.");
        return Ok(());
    }
    let value: serde_json::Value = serde_json::from_slice(&resp.bytes)
        .map_err(|e| anyhow::anyhow!("failed to parse upsert response: {e}"))?;
    formatter::output(cfg, &value)
}

pub async fn tag_desc_delete(cfg: &Config, tag_key: &str, cloud: Option<String>) -> Result<()> {
    let mut params: Vec<(String, String)> = vec![("tag_key".into(), tag_key.to_string())];
    if let Some(c) = cloud {
        params.push(("cloud".into(), c));
    }
    let q: Vec<(&str, &str)> = params
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let resp = raw_client::raw_request(
        cfg,
        "DELETE",
        "/api/v2/cost/tag_descriptions",
        &q,
        None,
        None,
        "application/json",
        &[],
    )
    .await?;
    if resp.bytes.is_empty() {
        eprintln!("Tag description for '{tag_key}' deleted.");
        return Ok(());
    }
    let value: serde_json::Value = serde_json::from_slice(&resp.bytes)
        .map_err(|e| anyhow::anyhow!("failed to parse delete response: {e}"))?;
    formatter::output(cfg, &value)
}

// ---- Tag Metadata ----

async fn tag_meta_get(
    cfg: &Config,
    sub_path: &str,
    month: &str,
    provider: Option<String>,
) -> Result<()> {
    let mut params: Vec<(String, String)> = vec![("filter[month]".into(), month.to_string())];
    if let Some(p) = provider {
        params.push(("filter[provider]".into(), p));
    }
    let q: Vec<(&str, &str)> = params
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let value = raw_client::raw_get(cfg, sub_path, &q).await?;
    formatter::output(cfg, &value)
}

pub async fn tag_meta_list(
    cfg: &Config,
    month: &str,
    provider: Option<String>,
    metric: Option<String>,
    tag_key: Option<String>,
    daily: bool,
) -> Result<()> {
    let mut params: Vec<(String, String)> = vec![("filter[month]".into(), month.to_string())];
    if let Some(p) = provider {
        params.push(("filter[provider]".into(), p));
    }
    if let Some(m) = metric {
        params.push(("filter[metric]".into(), m));
    }
    if let Some(k) = tag_key {
        params.push(("filter[tag_key]".into(), k));
    }
    if daily {
        params.push(("filter[daily]".into(), "true".into()));
    }
    let q: Vec<(&str, &str)> = params
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let value = raw_client::raw_get(cfg, "/api/v2/cost/tag_metadata", &q).await?;
    formatter::output(cfg, &value)
}

pub async fn tag_meta_sources(cfg: &Config, month: &str, provider: Option<String>) -> Result<()> {
    tag_meta_get(
        cfg,
        "/api/v2/cost/tag_metadata/tag_sources",
        month,
        provider,
    )
    .await
}

pub async fn tag_meta_metrics(cfg: &Config, month: &str, provider: Option<String>) -> Result<()> {
    tag_meta_get(cfg, "/api/v2/cost/tag_metadata/metrics", month, provider).await
}

pub async fn tag_meta_orchestrators(
    cfg: &Config,
    month: &str,
    provider: Option<String>,
) -> Result<()> {
    tag_meta_get(
        cfg,
        "/api/v2/cost/tag_metadata/orchestrators",
        month,
        provider,
    )
    .await
}

pub async fn tag_meta_currency(cfg: &Config, month: &str, provider: Option<String>) -> Result<()> {
    tag_meta_get(cfg, "/api/v2/cost/tag_metadata/currency", month, provider).await
}

// ---- Tags ----

pub async fn tags_list(
    cfg: &Config,
    metric: Option<String>,
    match_str: Option<String>,
    tags: Vec<String>,
) -> Result<()> {
    let mut params: Vec<(String, String)> = Vec::new();
    if let Some(m) = metric {
        params.push(("filter[metric]".into(), m));
    }
    if let Some(m) = match_str {
        params.push(("filter[match]".into(), m));
    }
    for t in tags {
        params.push(("filter[tags][]".into(), t));
    }
    let q: Vec<(&str, &str)> = params
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let value = raw_client::raw_get(cfg, "/api/v2/cost/tags", &q).await?;
    formatter::output(cfg, &value)
}

// ---- Tag Keys ----

pub async fn tag_keys_list(cfg: &Config, metric: Option<String>, tags: Vec<String>) -> Result<()> {
    let mut params: Vec<(String, String)> = Vec::new();
    if let Some(m) = metric {
        params.push(("filter[metric]".into(), m));
    }
    for t in tags {
        params.push(("filter[tags][]".into(), t));
    }
    let q: Vec<(&str, &str)> = params
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let value = raw_client::raw_get(cfg, "/api/v2/cost/tag_keys", &q).await?;
    formatter::output(cfg, &value)
}

pub async fn tag_keys_get(cfg: &Config, key: &str, metric: Option<String>) -> Result<()> {
    let path = format!("/api/v2/cost/tag_keys/{}", util_ext::percent_encode(key));
    let mut params: Vec<(String, String)> = Vec::new();
    if let Some(m) = metric {
        params.push(("filter[metric]".into(), m));
    }
    let q: Vec<(&str, &str)> = params
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let value = raw_client::raw_get(cfg, &path, &q).await?;
    formatter::output(cfg, &value)
}

// ---- Budgets ----

pub async fn budgets_list(cfg: &Config) -> Result<()> {
    let value = raw_client::raw_get(cfg, "/api/v2/cost/budgets", &[]).await?;
    formatter::output(cfg, &value)
}

pub async fn budgets_get(
    cfg: &Config,
    budget_id: &str,
    start: Option<String>,
    end: Option<String>,
    actual: bool,
    forecast: bool,
) -> Result<()> {
    let path = format!(
        "/api/v2/cost/budget/{}",
        util_ext::percent_encode(budget_id)
    );
    let mut params: Vec<(String, String)> = Vec::new();
    match (start, end) {
        (Some(s), Some(e)) => {
            params.push((
                "start".into(),
                util_ext::parse_time_to_unix_millis(&s)?.to_string(),
            ));
            params.push((
                "end".into(),
                util_ext::parse_time_to_unix_millis(&e)?.to_string(),
            ));
        }
        (None, None) => {}
        (Some(_), None) => anyhow::bail!("--end is required when --start is provided"),
        (None, Some(_)) => anyhow::bail!("--start is required when --end is provided"),
    }
    if actual {
        params.push(("actual".into(), "true".into()));
    }
    if forecast {
        params.push(("forecast".into(), "true".into()));
    }
    let q: Vec<(&str, &str)> = params
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let value = raw_client::raw_get(cfg, &path, &q).await?;
    formatter::output(cfg, &value)
}

pub async fn budgets_upsert(cfg: &Config, file: &str) -> Result<()> {
    let body: serde_json::Value = util::read_json_file(file)?;
    let body_bytes =
        serde_json::to_vec(&body).map_err(|e| anyhow::anyhow!("failed to serialize: {e}"))?;
    let resp = raw_client::raw_request(
        cfg,
        "PUT",
        "/api/v2/cost/budget",
        &[],
        Some(body_bytes),
        Some("application/json"),
        "application/json",
        &[],
    )
    .await?;
    if resp.bytes.is_empty() {
        eprintln!("Budget saved.");
        return Ok(());
    }
    let value: serde_json::Value = serde_json::from_slice(&resp.bytes)
        .map_err(|e| anyhow::anyhow!("failed to parse budget response: {e}"))?;
    formatter::output(cfg, &value)
}

pub async fn budgets_delete(cfg: &Config, budget_id: &str) -> Result<()> {
    let path = format!(
        "/api/v2/cost/budget/{}",
        util_ext::percent_encode(budget_id)
    );
    raw_client::raw_delete(cfg, &path).await?;
    eprintln!("Budget '{budget_id}' deleted.");
    Ok(())
}

pub async fn budgets_validate(cfg: &Config, file: &str) -> Result<()> {
    let body: serde_json::Value = util::read_json_file(file)?;
    let value = raw_client::raw_post(cfg, "/api/v2/cost/budget/validate", body).await?;
    formatter::output(cfg, &value)
}

// ---- Commitments ----

/// Query parameters shared by all commitment program endpoints.
struct CommitmentQuery<'a> {
    provider: &'a str,
    product: &'a str,
    from_ms: i64,
    to_ms: i64,
    commitment_type: &'a str,
    filter_by: Option<&'a str>,
}

impl CommitmentQuery<'_> {
    fn to_params(&self) -> Vec<(String, String)> {
        let mut params = vec![
            ("start".into(), self.from_ms.to_string()),
            ("end".into(), self.to_ms.to_string()),
            ("provider".into(), self.provider.to_string()),
            ("product".into(), self.product.to_string()),
            ("commitmentType".into(), self.commitment_type.to_string()),
        ];
        if let Some(f) = self.filter_by {
            params.push(("filterBy".into(), f.to_string()));
        }
        params
    }
}

async fn commitment_call(cfg: &Config, path: &str, q: &CommitmentQuery<'_>) -> Result<()> {
    let params = q.to_params();
    let refs: Vec<(&str, &str)> = params
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let value = raw_client::raw_get(cfg, path, &refs).await?;
    formatter::output(cfg, &value)
}

/// Parse caller-supplied time strings and build a [`CommitmentQuery`].
fn parse_commitment_query<'a>(
    provider: &'a str,
    product: &'a str,
    from: &str,
    to: &str,
    commitment_type: &'a Option<String>,
    filter_by: &'a Option<String>,
) -> anyhow::Result<CommitmentQuery<'a>> {
    let from_ms = util_ext::parse_time_to_unix_millis(from)?;
    let to_ms = util_ext::parse_time_to_unix_millis(to)?;
    Ok(CommitmentQuery {
        provider,
        product,
        from_ms,
        to_ms,
        commitment_type: commitment_type.as_deref().unwrap_or("RI"),
        filter_by: filter_by.as_deref(),
    })
}

pub async fn commitments_utilization(
    cfg: &Config,
    provider: &str,
    product: &str,
    from: &str,
    to: &str,
    commitment_type: Option<String>,
    filter_by: Option<String>,
) -> Result<()> {
    let q = parse_commitment_query(provider, product, from, to, &commitment_type, &filter_by)?;
    commitment_call(cfg, "/api/v2/cost/commitments/utilization/scalar", &q).await
}

pub async fn commitments_coverage(
    cfg: &Config,
    provider: &str,
    product: &str,
    from: &str,
    to: &str,
    commitment_type: Option<String>,
    filter_by: Option<String>,
) -> Result<()> {
    let q = parse_commitment_query(provider, product, from, to, &commitment_type, &filter_by)?;
    commitment_call(cfg, "/api/v2/cost/commitments/coverage/scalar", &q).await
}

pub async fn commitments_savings(
    cfg: &Config,
    provider: &str,
    product: &str,
    from: &str,
    to: &str,
    commitment_type: Option<String>,
    filter_by: Option<String>,
) -> Result<()> {
    let q = parse_commitment_query(provider, product, from, to, &commitment_type, &filter_by)?;
    commitment_call(cfg, "/api/v2/cost/commitments/savings/scalar", &q).await
}

pub async fn commitments_hotspots(
    cfg: &Config,
    provider: &str,
    product: &str,
    from: &str,
    to: &str,
    commitment_type: Option<String>,
    filter_by: Option<String>,
) -> Result<()> {
    let q = parse_commitment_query(provider, product, from, to, &commitment_type, &filter_by)?;
    commitment_call(
        cfg,
        "/api/v2/cost/commitments/on-demand-hot-spots/scalar",
        &q,
    )
    .await
}

pub async fn commitments_utilization_ts(
    cfg: &Config,
    provider: &str,
    product: &str,
    from: &str,
    to: &str,
    commitment_type: Option<String>,
    filter_by: Option<String>,
) -> Result<()> {
    let q = parse_commitment_query(provider, product, from, to, &commitment_type, &filter_by)?;
    commitment_call(cfg, "/api/v2/cost/commitments/utilization/timeseries", &q).await
}

pub async fn commitments_coverage_ts(
    cfg: &Config,
    provider: &str,
    product: &str,
    from: &str,
    to: &str,
    commitment_type: Option<String>,
    filter_by: Option<String>,
) -> Result<()> {
    let q = parse_commitment_query(provider, product, from, to, &commitment_type, &filter_by)?;
    commitment_call(cfg, "/api/v2/cost/commitments/coverage/timeseries", &q).await
}

pub async fn commitments_savings_ts(
    cfg: &Config,
    provider: &str,
    product: &str,
    from: &str,
    to: &str,
    commitment_type: Option<String>,
    filter_by: Option<String>,
) -> Result<()> {
    let q = parse_commitment_query(provider, product, from, to, &commitment_type, &filter_by)?;
    commitment_call(cfg, "/api/v2/cost/commitments/savings/timeseries", &q).await
}

pub async fn commitments_list(
    cfg: &Config,
    provider: &str,
    product: &str,
    from: &str,
    to: &str,
    commitment_type: Option<String>,
    filter_by: Option<String>,
) -> Result<()> {
    let q = parse_commitment_query(provider, product, from, to, &commitment_type, &filter_by)?;
    commitment_call(cfg, "/api/v2/cost/commitments/commitment-list", &q).await
}

#[cfg(test)]
mod tests {
    use crate::test_support::*;

    use super::*;
    use crate::config::OutputFormat;

    fn test_cfg() -> Config {
        Config {
            api_key: None,
            app_key: None,
            access_token: None,
            site: "datadoghq.com".into(),
            site_explicit: false,
            org: None,
            output_format: OutputFormat::Json,
            auto_approve: false,
            agent_mode: false,
            read_only: false,
            jq: None,
        }
    }

    #[test]
    fn test_commitment_query_required_params() {
        let q = CommitmentQuery {
            provider: "aws",
            product: "EC2",
            from_ms: 1000,
            to_ms: 2000,
            commitment_type: "RI",
            filter_by: None,
        };
        let p = q.to_params();
        assert_eq!(p.len(), 5);
        assert!(p.iter().any(|(k, v)| k == "provider" && v == "aws"));
        assert!(p.iter().any(|(k, v)| k == "product" && v == "EC2"));
        assert!(p.iter().any(|(k, v)| k == "start" && v == "1000"));
        assert!(p.iter().any(|(k, v)| k == "end" && v == "2000"));
        assert!(p.iter().any(|(k, v)| k == "commitmentType" && v == "RI"));
    }

    #[test]
    fn test_commitment_query_with_filter() {
        let q = CommitmentQuery {
            provider: "azure",
            product: "VirtualMachines",
            from_ms: 0,
            to_ms: 1,
            commitment_type: "SP",
            filter_by: Some("env:prod"),
        };
        let p = q.to_params();
        assert_eq!(p.len(), 6);
        assert!(p.iter().any(|(k, v)| k == "provider" && v == "azure"));
        assert!(p.iter().any(|(k, v)| k == "commitmentType" && v == "SP"));
        assert!(p.iter().any(|(k, v)| k == "filterBy" && v == "env:prod"));
    }

    #[test]
    fn test_commitment_query_no_filter() {
        let q = CommitmentQuery {
            provider: "aws",
            product: "RDS",
            from_ms: 0,
            to_ms: 0,
            commitment_type: "RI",
            filter_by: None,
        };
        let p = q.to_params();
        assert!(!p.iter().any(|(k, _)| k == "filterBy"));
    }

    #[test]
    fn test_parse_commitment_query_default_commitment_type() {
        // When no commitment_type is supplied the default "RI" must be applied.
        let ct = None::<String>;
        let fb = None::<String>;
        let q = parse_commitment_query("aws", "EC2", "1700000000", "1700003600", &ct, &fb).unwrap();
        assert_eq!(q.commitment_type, "RI");
    }

    #[test]
    fn test_parse_commitment_query_invalid_from() {
        let ct = None::<String>;
        let fb = None::<String>;
        let result = parse_commitment_query("aws", "EC2", "not-a-time", "1h", &ct, &fb);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_commitment_query_invalid_to() {
        let ct = None::<String>;
        let fb = None::<String>;
        let result = parse_commitment_query("aws", "EC2", "1h", "not-a-time", &ct, &fb);
        assert!(result.is_err());
    }

    #[test]
    fn test_budgets_get_start_without_end() {
        // Verify that passing only --start returns a useful error.
        let rt = tokio::runtime::Runtime::new().unwrap();
        let cfg = test_cfg();
        let err = rt
            .block_on(budgets_get(
                &cfg,
                "budget-1",
                Some("1h".into()),
                None,
                false,
                false,
            ))
            .unwrap_err();
        assert!(
            err.to_string().contains("--end"),
            "expected mention of --end, got: {err}"
        );
    }

    #[test]
    fn test_budgets_get_end_without_start() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let cfg = test_cfg();
        let err = rt
            .block_on(budgets_get(
                &cfg,
                "budget-1",
                None,
                Some("now".into()),
                false,
                false,
            ))
            .unwrap_err();
        assert!(
            err.to_string().contains("--start"),
            "expected mention of --start, got: {err}"
        );
    }

    #[test]
    fn test_custom_costs_upload_missing_file() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let cfg = test_cfg();
        let err = rt
            .block_on(custom_costs_upload(
                &cfg,
                "/tmp/__pup_nonexistent_cost_file__.csv",
                None,
            ))
            .unwrap_err();
        assert!(
            err.to_string().contains("failed to read"),
            "expected read error, got: {err}"
        );
    }

    // Verify tag_desc_upsert and tag_desc_delete reach raw_request (and fail at
    // the auth check) for both the with-cloud and without-cloud code paths. This
    // ensures the refactored query-param building runs without panic.
    #[test]
    fn test_tag_desc_upsert_no_auth() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let cfg = test_cfg();
        let err = rt
            .block_on(tag_desc_upsert(&cfg, "team", "The team tag", None))
            .unwrap_err();
        assert!(
            err.to_string().contains("no authentication configured"),
            "expected auth error, got: {err}"
        );
    }

    #[test]
    fn test_tag_desc_upsert_with_cloud_no_auth() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let cfg = test_cfg();
        let err = rt
            .block_on(tag_desc_upsert(
                &cfg,
                "team",
                "The team tag",
                Some("aws".into()),
            ))
            .unwrap_err();
        assert!(
            err.to_string().contains("no authentication configured"),
            "expected auth error, got: {err}"
        );
    }

    #[test]
    fn test_tag_desc_delete_no_auth() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let cfg = test_cfg();
        let err = rt
            .block_on(tag_desc_delete(&cfg, "team", None))
            .unwrap_err();
        assert!(
            err.to_string().contains("no authentication configured"),
            "expected auth error, got: {err}"
        );
    }

    #[test]
    fn test_tag_desc_delete_with_cloud_no_auth() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let cfg = test_cfg();
        let err = rt
            .block_on(tag_desc_delete(&cfg, "team", Some("azure".into())))
            .unwrap_err();
        assert!(
            err.to_string().contains("no authentication configured"),
            "expected auth error, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_tag_desc_upsert_success() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "PUT", r#"{}"#).await;
        let result = super::tag_desc_upsert(&cfg, "team", "The team tag", None).await;
        assert!(result.is_ok(), "tag_desc_upsert failed: {:?}", result.err());
        cleanup_env();
    }

    /// Verifies that the optional `version` query parameter is passed through
    /// correctly after the refactor from path-string embedding to params vec.
    #[tokio::test]
    async fn test_custom_costs_upload_with_version_success() {
        let _lock = lock_env().await;
        let tmp = std::env::temp_dir().join("pup_test_upload_version.csv");
        std::fs::write(&tmp, b"cost,amount\nec2,100\n").unwrap();
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "PUT", r#"{}"#).await;
        let result =
            super::custom_costs_upload(&cfg, tmp.to_str().unwrap(), Some("v2".into())).await;
        assert!(
            result.is_ok(),
            "custom_costs_upload with version failed: {:?}",
            result.err()
        );
        let _ = std::fs::remove_file(&tmp);
        cleanup_env();
    }
}
