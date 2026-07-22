use anyhow::Result;

use crate::config::Config;
use crate::formatter;
use crate::raw_client::HttpError;
use crate::useragent;
use crate::util_ext;

const SKILLS_PATH: &str = "/api/v2/onboarding/skills";
const SESSIONS_PATH: &str = "/api/v2/onboarding/sessions";

const VALID_STATUSES: &[&str] = &["in_progress", "completed", "failed", "abandoned"];
// Terminal statuses require at least one skill_id; the backend allows in_progress to be empty.
const TERMINAL_STATUSES: &[&str] = &["completed", "failed", "abandoned"];
const VALID_INTENTS: &[&str] = &["explore", "install", "reference"];

/// List the available Datadog skills (`GET /api/v2/onboarding/skills`).
///
/// The API has no tag filter, so `tags` is applied in memory against each
/// skill's `attributes.tags`: a skill is kept only if it carries every
/// requested tag.
pub async fn list(cfg: &Config, org_id: Option<i64>, tags: &[String]) -> Result<()> {
    let org = org_id.map(|o| o.to_string());
    let query: Vec<(&str, &str)> = match &org {
        Some(o) => vec![("org_id", o.as_str())],
        None => vec![],
    };
    let mut resp = get_json(cfg, SKILLS_PATH, &query).await?;
    if !tags.is_empty() {
        filter_by_tags(&mut resp, tags);
    }
    formatter::output(cfg, &resp)
}

fn filter_by_tags(doc: &mut serde_json::Value, tags: &[String]) {
    let Some(items) = doc.get_mut("data").and_then(|d| d.as_array_mut()) else {
        return;
    };
    items.retain(|item| {
        let Some(skill_tags) = item.pointer("/attributes/tags").and_then(|t| t.as_array()) else {
            return false;
        };
        tags.iter()
            .all(|want| skill_tags.iter().any(|t| t.as_str() == Some(want.as_str())))
    });
}

/// Fetch a single skill's instructions as raw markdown
/// (`GET /api/v2/onboarding/skills/{skill_id}?format=md`).
///
/// Always requests the markdown representation (frontmatter + body) rather than
/// the JSON:API envelope — it's what an assistant consumes to follow the skill.
pub async fn get(
    cfg: &Config,
    skill_id: &str,
    intent: Option<&str>,
    session_id: Option<&str>,
    org_id: Option<i64>,
) -> Result<()> {
    if skill_id.trim().is_empty() {
        anyhow::bail!("skill_id must not be empty");
    }

    if let Some(intent) = intent {
        if !VALID_INTENTS.contains(&intent) {
            anyhow::bail!(
                "invalid intent '{intent}'; expected one of: {}",
                VALID_INTENTS.join(", ")
            );
        }
    }

    let org = org_id.map(|o| o.to_string());
    let mut query: Vec<(&str, &str)> = Vec::new();
    if let Some(intent) = intent {
        query.push(("intent", intent));
    }
    // The API's query param is still `onboarding_run_id`; only the CLI flag is `--session-id`.
    if let Some(session_id) = session_id {
        query.push(("onboarding_run_id", session_id));
    }
    if let Some(o) = &org {
        query.push(("org_id", o.as_str()));
    }
    query.push(("format", "md"));

    let path = format!("{SKILLS_PATH}/{}", util_ext::percent_encode(skill_id));
    let markdown = get_markdown(cfg, &path, &query).await?;
    print!("{markdown}");
    if !markdown.ends_with('\n') {
        println!();
    }
    Ok(())
}

/// Record a session outcome (`POST /api/v2/onboarding/sessions`).
pub async fn sessions_create(
    cfg: &Config,
    session_id: &str,
    skill_ids: &[String],
    summary: &str,
    status: &str,
) -> Result<()> {
    // Unlike the public list/get routes, recording a session writes
    // org-attributed telemetry: the API requires an authenticated caller and
    // derives the org from the credentials (no org_id in the request).
    cfg.validate_auth()?;

    if session_id.trim().is_empty() {
        anyhow::bail!("--session-id must not be empty");
    }

    if !VALID_STATUSES.contains(&status) {
        anyhow::bail!(
            "invalid status '{status}'; expected one of: {}",
            VALID_STATUSES.join(", ")
        );
    }

    if skill_ids.iter().any(|id| id.trim().is_empty()) {
        anyhow::bail!("--skill-id must not be empty");
    }

    if TERMINAL_STATUSES.contains(&status) && skill_ids.is_empty() {
        anyhow::bail!("at least one --skill-id is required when status is '{status}'");
    }

    let body = serde_json::json!({
        "data": {
            "type": "onboarding_session",
            "id": session_id,
            "attributes": {
                "skill_ids": skill_ids,
                "summary": summary,
                "status": status,
            },
        },
    });

    let resp = post(cfg, SESSIONS_PATH, body).await?;
    formatter::output(cfg, &resp)
}

// list/get are public (OpenAuth), so unlike `client::apply_auth` this attaches
// credentials when present but never fails without them. sessions_create needs
// auth and enforces it via cfg.validate_auth() before calling post().
fn apply_optional_auth(mut req: reqwest::RequestBuilder, cfg: &Config) -> reqwest::RequestBuilder {
    if let Some(token) = &cfg.access_token {
        req = req.header("Authorization", format!("Bearer {token}"));
    } else if let (Some(api_key), Some(app_key)) = (&cfg.api_key, &cfg.app_key) {
        req = req
            .header("DD-API-KEY", api_key.as_str())
            .header("DD-APPLICATION-KEY", app_key.as_str());
    }
    req
}

async fn get_json(cfg: &Config, path: &str, query: &[(&str, &str)]) -> Result<serde_json::Value> {
    let url = format!("{}{}", cfg.api_base_url(), path);
    let resp = send_get(cfg, &url, query, "application/vnd.api+json").await?;
    read_json(resp, "GET").await
}

async fn get_markdown(cfg: &Config, path: &str, query: &[(&str, &str)]) -> Result<String> {
    let url = format!("{}{}", cfg.api_base_url(), path);
    let resp = send_get(cfg, &url, query, "text/markdown").await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let url = resp.url().to_string();
        let body = resp.text().await.unwrap_or_default();
        return Err(HttpError {
            status: status.as_u16(),
            method: "GET".to_string(),
            url,
            body,
        }
        .into());
    }
    Ok(resp.text().await?)
}

async fn send_get(
    cfg: &Config,
    url: &str,
    query: &[(&str, &str)],
    accept: &str,
) -> Result<reqwest::Response> {
    let client = reqwest::Client::new();
    let mut req = client.get(url);
    if !query.is_empty() {
        req = req.query(query);
    }
    req = apply_optional_auth(req, cfg);
    Ok(req
        .header("Accept", accept)
        .header("User-Agent", useragent::get())
        .send()
        .await?)
}

async fn post(cfg: &Config, path: &str, body: serde_json::Value) -> Result<serde_json::Value> {
    let url = format!("{}{}", cfg.api_base_url(), path);
    let client = reqwest::Client::new();
    let mut req = client.post(&url);
    req = apply_optional_auth(req, cfg);
    let resp = req
        .header("Content-Type", "application/vnd.api+json")
        .header("Accept", "application/vnd.api+json")
        .header("User-Agent", useragent::get())
        .json(&body)
        .send()
        .await?;
    read_json(resp, "POST").await
}

async fn read_json(resp: reqwest::Response, method: &str) -> Result<serde_json::Value> {
    if !resp.status().is_success() {
        let status = resp.status();
        let url = resp.url().to_string();
        let body = resp.text().await.unwrap_or_default();
        return Err(HttpError {
            status: status.as_u16(),
            method: method.to_string(),
            url,
            body,
        }
        .into());
    }
    Ok(resp.json().await?)
}

#[cfg(test)]
mod tests {
    use crate::test_support::*;

    #[tokio::test]
    async fn skills_list_ok() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = server
            .mock("GET", "/api/v2/onboarding/skills")
            .with_status(200)
            .with_header("content-type", "application/vnd.api+json")
            .with_body(r#"{"data":[]}"#)
            .create_async()
            .await;
        let result = super::list(&cfg, None, &[]).await;
        assert!(result.is_ok(), "skills list failed: {:?}", result.err());
        cleanup_env();
    }

    #[tokio::test]
    async fn skills_list_forwards_org_id() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let mock = server
            .mock("GET", "/api/v2/onboarding/skills")
            .match_query(mockito::Matcher::UrlEncoded("org_id".into(), "42".into()))
            .with_status(200)
            .with_header("content-type", "application/vnd.api+json")
            .with_body(r#"{"data":[]}"#)
            .create_async()
            .await;
        let result = super::list(&cfg, Some(42), &[]).await;
        assert!(result.is_ok(), "skills list failed: {:?}", result.err());
        mock.assert_async().await;
        cleanup_env();
    }

    fn sample_catalog() -> serde_json::Value {
        serde_json::json!({
            "data": [
                {"id": "agent-install-docker", "attributes": {"tags": ["agent", "docker", "container"]}},
                {"id": "aws-integration-setup", "attributes": {"tags": ["integration", "aws"]}},
                {"id": "orchestrator", "attributes": {"tags": ["orchestrator"]}},
                {"id": "untagged", "attributes": {"tags": null}},
            ]
        })
    }

    fn ids(doc: &serde_json::Value) -> Vec<String> {
        doc["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["id"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn filter_by_tags_single_tag() {
        let mut doc = sample_catalog();
        super::filter_by_tags(&mut doc, &["docker".to_string()]);
        assert_eq!(ids(&doc), vec!["agent-install-docker"]);
    }

    #[test]
    fn filter_by_tags_requires_all_tags() {
        let mut doc = sample_catalog();
        super::filter_by_tags(&mut doc, &["agent".to_string(), "container".to_string()]);
        assert_eq!(ids(&doc), vec!["agent-install-docker"]);

        let mut doc = sample_catalog();
        // "agent" matches docker skill, "aws" matches the integration skill, but no
        // single skill has both — AND semantics yield nothing.
        super::filter_by_tags(&mut doc, &["agent".to_string(), "aws".to_string()]);
        assert!(ids(&doc).is_empty());
    }

    #[test]
    fn filter_by_tags_excludes_skills_without_tags() {
        let mut doc = sample_catalog();
        super::filter_by_tags(&mut doc, &["orchestrator".to_string()]);
        assert_eq!(ids(&doc), vec!["orchestrator"]);
        // The null-tags skill is never matched.
        let mut doc = sample_catalog();
        super::filter_by_tags(&mut doc, &["agent".to_string()]);
        assert!(!ids(&doc).contains(&"untagged".to_string()));
    }

    #[tokio::test]
    async fn skills_get_ok_with_intent() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let mock = server
            .mock("GET", "/api/v2/onboarding/skills/aws-integration-setup")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("intent".into(), "install".into()),
                mockito::Matcher::UrlEncoded("format".into(), "md".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "text/markdown; charset=utf-8")
            .with_body("---\nid: aws-integration-setup\n---\n# Setup\n")
            .create_async()
            .await;
        let result = super::get(&cfg, "aws-integration-setup", Some("install"), None, None).await;
        assert!(result.is_ok(), "skills get failed: {:?}", result.err());
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn skills_get_sends_session_id_as_onboarding_run_id_query() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let mock = server
            .mock("GET", "/api/v2/onboarding/skills/orchestrator")
            .match_query(mockito::Matcher::UrlEncoded(
                "onboarding_run_id".into(),
                "run-abc".into(),
            ))
            .with_status(200)
            .with_header("content-type", "text/markdown; charset=utf-8")
            .with_body("# orchestrator\n")
            .create_async()
            .await;
        let result = super::get(&cfg, "orchestrator", None, Some("run-abc"), None).await;
        assert!(result.is_ok(), "skills get failed: {:?}", result.err());
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn skills_get_percent_encodes_skill_id() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let mock = server
            .mock("GET", "/api/v2/onboarding/skills/weird%2Fid%3Fx")
            .match_query(mockito::Matcher::UrlEncoded("format".into(), "md".into()))
            .with_status(200)
            .with_header("content-type", "text/markdown; charset=utf-8")
            .with_body("# weird\n")
            .create_async()
            .await;
        let result = super::get(&cfg, "weird/id?x", None, None, None).await;
        assert!(result.is_ok(), "skills get failed: {:?}", result.err());
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn skills_get_rejects_empty_skill_id() {
        let _lock = lock_env().await;
        let server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let result = super::get(&cfg, "   ", None, None, None).await;
        let err = result.unwrap_err().to_string();
        assert!(err.contains("skill_id must not be empty"), "got: {err}");
        cleanup_env();
    }

    #[tokio::test]
    async fn skills_get_rejects_invalid_intent() {
        let _lock = lock_env().await;
        let server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let result = super::get(&cfg, "some-skill", Some("bogus"), None, None).await;
        let err = result.unwrap_err().to_string();
        assert!(err.contains("invalid intent"), "got: {err}");
        cleanup_env();
    }

    #[tokio::test]
    async fn skills_get_surfaces_not_found() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(404)
            .with_header("content-type", "application/vnd.api+json")
            .with_body(r#"{"errors":[{"detail":"Skill not found: nope"}]}"#)
            .create_async()
            .await;
        let result = super::get(&cfg, "nope", None, None, None).await;
        let err = result.unwrap_err().to_string();
        assert!(err.contains("404"), "expected 404 in error, got: {err}");
        cleanup_env();
    }

    #[tokio::test]
    async fn sessions_create_ok() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let mock = server
            .mock("POST", "/api/v2/onboarding/sessions")
            .match_body(mockito::Matcher::PartialJson(serde_json::json!({
                "data": {
                    "type": "onboarding_session",
                    "id": "run-123",
                    "attributes": {
                        "status": "completed",
                        "skill_ids": ["aws-integration-setup"],
                    },
                },
            })))
            .with_status(201)
            .with_header("content-type", "application/vnd.api+json")
            .with_body(r#"{"data":{"id":"run-123"}}"#)
            .create_async()
            .await;
        let result = super::sessions_create(
            &cfg,
            "run-123",
            &["aws-integration-setup".to_string()],
            "Set up the AWS integration",
            "completed",
        )
        .await;
        assert!(result.is_ok(), "sessions create failed: {:?}", result.err());
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn sessions_create_rejects_invalid_status() {
        let _lock = lock_env().await;
        let server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let result = super::sessions_create(
            &cfg,
            "run-123",
            &["aws-integration-setup".to_string()],
            "summary",
            "done",
        )
        .await;
        let err = result.unwrap_err().to_string();
        assert!(err.contains("invalid status"), "got: {err}");
        cleanup_env();
    }

    #[tokio::test]
    async fn sessions_create_rejects_empty_session_id() {
        let _lock = lock_env().await;
        let server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let result = super::sessions_create(
            &cfg,
            "  ",
            &["aws-integration-setup".to_string()],
            "summary",
            "completed",
        )
        .await;
        let err = result.unwrap_err().to_string();
        assert!(err.contains("--session-id must not be empty"), "got: {err}");
        cleanup_env();
    }

    #[tokio::test]
    async fn sessions_create_rejects_empty_skill_id() {
        let _lock = lock_env().await;
        let server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let result =
            super::sessions_create(&cfg, "run-123", &["".to_string()], "summary", "completed")
                .await;
        let err = result.unwrap_err().to_string();
        assert!(err.contains("--skill-id must not be empty"), "got: {err}");
        cleanup_env();
    }

    #[tokio::test]
    async fn sessions_create_terminal_requires_skill_id() {
        let _lock = lock_env().await;
        let server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let result = super::sessions_create(&cfg, "run-123", &[], "summary", "completed").await;
        let err = result.unwrap_err().to_string();
        assert!(err.contains("at least one --skill-id"), "got: {err}");
        cleanup_env();
    }

    #[tokio::test]
    async fn sessions_create_in_progress_allows_empty_skill_ids() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let mock = server
            .mock("POST", "/api/v2/onboarding/sessions")
            .match_body(mockito::Matcher::PartialJson(serde_json::json!({
                "data": {
                    "type": "onboarding_session",
                    "id": "run-123",
                    "attributes": { "status": "in_progress", "skill_ids": [] },
                },
            })))
            .with_status(201)
            .with_header("content-type", "application/vnd.api+json")
            .with_body(r#"{"data":{"id":"run-123"}}"#)
            .create_async()
            .await;
        let result =
            super::sessions_create(&cfg, "run-123", &[], "session started", "in_progress").await;
        assert!(
            result.is_ok(),
            "in_progress create failed: {:?}",
            result.err()
        );
        mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn sessions_create_requires_auth() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let server = mockito::Server::new_async().await;
        let mut cfg = test_config(&server.url());
        // Recording a session requires authentication (unlike list/get).
        cfg.api_key = None;
        cfg.app_key = None;
        cfg.access_token = None;
        let result = super::sessions_create(
            &cfg,
            "run-123",
            &["aws-integration-setup".to_string()],
            "summary",
            "completed",
        )
        .await;
        let err = result.unwrap_err().to_string();
        assert!(err.contains("authentication"), "got: {err}");
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }
}
