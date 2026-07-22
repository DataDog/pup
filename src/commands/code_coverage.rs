use anyhow::Result;
use datadog_api_client::datadogV2::api_code_coverage::CodeCoverageAPI;
use datadog_api_client::datadogV2::model::{
    BranchCoverageSummaryRequest, BranchCoverageSummaryRequestAttributes,
    BranchCoverageSummaryRequestData, BranchCoverageSummaryRequestType,
    CommitCoverageSummaryRequest, CommitCoverageSummaryRequestAttributes,
    CommitCoverageSummaryRequestData, CommitCoverageSummaryRequestType,
};

use crate::config::Config;
use crate::formatter;

pub async fn branch_summary(cfg: &Config, repo: String, branch: String) -> Result<()> {
    let api = crate::make_api!(CodeCoverageAPI, cfg);
    let body = BranchCoverageSummaryRequest::new(BranchCoverageSummaryRequestData::new(
        BranchCoverageSummaryRequestAttributes::new(branch).repository_url(repo),
        BranchCoverageSummaryRequestType::CI_APP_COVERAGE_BRANCH_SUMMARY_REQUEST,
    ));
    let resp = api
        .get_code_coverage_branch_summary(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get branch summary: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn commit_summary(cfg: &Config, repo: String, commit: String) -> Result<()> {
    let api = crate::make_api!(CodeCoverageAPI, cfg);
    let body = CommitCoverageSummaryRequest::new(CommitCoverageSummaryRequestData::new(
        CommitCoverageSummaryRequestAttributes::new(commit).repository_url(repo),
        CommitCoverageSummaryRequestType::CI_APP_COVERAGE_COMMIT_SUMMARY_REQUEST,
    ));
    let resp = api
        .get_code_coverage_commit_summary(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get commit summary: {e:?}"))?;
    formatter::output(cfg, &resp)
}

#[cfg(test)]
mod tests {

    use crate::test_support::*;

    #[tokio::test]
    async fn test_code_coverage_branch_summary() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data": {}}"#).await;
        let _ = super::branch_summary(&cfg, "repo".into(), "main".into()).await;
        cleanup_env();
    }
}
