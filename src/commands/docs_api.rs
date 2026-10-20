#[cfg(not(target_arch = "wasm32"))]
use anyhow::Context;
use anyhow::Result;
#[cfg(not(target_arch = "wasm32"))]
use reqwest::{Client, Url};
#[cfg(not(target_arch = "wasm32"))]
use std::collections::{BTreeMap, HashSet};
use std::io::Write;

#[cfg(not(target_arch = "wasm32"))]
const INDEX_URL: &str = "https://docs.datadoghq.com/api_reference/llms.txt";
#[cfg(not(target_arch = "wasm32"))]
const MAX_INDEX_PAGES: usize = 64;

#[cfg(not(target_arch = "wasm32"))]
struct DocPage {
    title: String,
    url: Url,
}

/// Discover API documentation live, or print a topic's original Markdown.
#[cfg(not(target_arch = "wasm32"))]
pub async fn run(
    topic: Option<&str>,
    operation: Option<&str>,
    operations: bool,
    out: &mut impl Write,
) -> Result<()> {
    let index = Url::parse(INDEX_URL)?;
    run_selected(
        &client_for(&index)?,
        &index,
        topic,
        operation,
        operations,
        out,
    )
    .await
}

#[cfg(target_arch = "wasm32")]
pub async fn run(
    _topic: Option<&str>,
    _operation: Option<&str>,
    _operations: bool,
    _out: &mut impl Write,
) -> Result<()> {
    anyhow::bail!("docs api is not supported in WASM builds")
}

#[cfg(not(target_arch = "wasm32"))]
fn client_for(origin: &Url) -> Result<Client> {
    let origin = origin.clone();
    Client::builder()
        .user_agent(crate::useragent::get())
        .timeout(std::time::Duration::from_secs(30))
        .connect_timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() >= 5 || !same_origin(attempt.url(), &origin) {
                attempt.error(
                    "API docs redirect left the documentation host or exceeded five redirects",
                )
            } else {
                attempt.follow()
            }
        }))
        .build()
        .context("Failed to build API docs HTTP client")
}

#[cfg(not(target_arch = "wasm32"))]
fn same_origin(url: &Url, origin: &Url) -> bool {
    url.origin() == origin.origin()
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
}

#[cfg(not(target_arch = "wasm32"))]
async fn fetch(client: &Client, url: &Url) -> Result<Vec<u8>> {
    let response = client
        .get(url.clone())
        .send()
        .await
        .with_context(|| format!("Failed to fetch API docs from {url}"))?;
    anyhow::ensure!(
        response.status().is_success(),
        "API docs request to {url} failed (HTTP {})",
        response.status()
    );
    Ok(response
        .bytes()
        .await
        .context("Failed to read API docs response")?
        .to_vec())
}

#[cfg(not(target_arch = "wasm32"))]
fn page_from_link(title: &str, url: &Url, origin: &Url) -> Option<(String, DocPage)> {
    if !same_origin(url, origin) {
        return None;
    }
    let slug = url
        .path()
        .strip_prefix("/api/latest/")?
        .strip_suffix(".md")?;
    if slug.split('/').count() > 2
        || !slug.split('/').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        })
    {
        return None;
    }
    Some((
        slug.to_string(),
        DocPage {
            title: title.to_string(),
            url: url.clone(),
        },
    ))
}

#[cfg(not(target_arch = "wasm32"))]
async fn discover(client: &Client, index: &Url) -> Result<BTreeMap<String, DocPage>> {
    let mut pending = vec![index.clone()];
    let mut visited = HashSet::new();
    let mut topics = BTreeMap::new();
    while let Some(url) = pending.pop() {
        if !visited.insert(url.clone()) {
            continue;
        }
        anyhow::ensure!(
            visited.len() <= MAX_INDEX_PAGES,
            "API docs index exceeds {MAX_INDEX_PAGES} pages"
        );
        let bytes = fetch(client, &url).await?;
        let text = std::str::from_utf8(&bytes).context("API docs index is not valid UTF-8")?;
        for line in text.lines() {
            let Some(link) = line.trim().strip_prefix("- [") else {
                continue;
            };
            let Some((title, rest)) = link.split_once("](") else {
                continue;
            };
            let Some((target, _)) = rest.split_once(')') else {
                continue;
            };
            let Ok(target) = url.join(target) else {
                continue;
            };
            if !same_origin(&target, index) {
                continue;
            }
            if target.path().starts_with("/api_reference/") && target.path().ends_with("/llms.txt")
            {
                pending.push(target);
            } else if let Some((slug, topic)) = page_from_link(title, &target, index) {
                topics.entry(slug).or_insert(topic);
            }
        }
    }
    anyhow::ensure!(
        topics.keys().any(|key| !key.contains('/')),
        "API docs index contained no API topics; its format may have changed"
    );
    Ok(topics)
}

#[cfg(not(target_arch = "wasm32"))]
async fn run_selected(
    client: &Client,
    index: &Url,
    topic: Option<&str>,
    operation: Option<&str>,
    operations: bool,
    out: &mut impl Write,
) -> Result<()> {
    let topics = discover(client, index).await?;
    let Some(name) = topic else {
        let mut listing = String::from("TOPIC\tTITLE\n");
        for (slug, topic) in topics.iter().filter(|(slug, _)| !slug.contains('/')) {
            listing.push_str(&format!("{slug}\t{}\n", topic.title));
        }
        return write_output(out, listing.as_bytes());
    };
    let name = name.to_ascii_lowercase();
    let Some(topic) = topics.get(&name).filter(|_| !name.contains('/')) else {
        let suggestions: Vec<_> = topics
            .keys()
            .filter(|slug| {
                !slug.contains('/')
                    && !name.is_empty()
                    && (slug.contains(&name) || name.contains(slug.as_str()))
            })
            .take(5)
            .map(String::as_str)
            .collect();
        let hint = if suggestions.is_empty() {
            String::new()
        } else {
            format!(" Did you mean {}?", suggestions.join(", "))
        };
        anyhow::bail!(
            "Unknown API docs topic {name:?}.{hint} Run `pup docs api` to list available topics."
        );
    };
    if operations {
        let prefix = format!("{name}/");
        let mut listing = String::from("OPERATION\tTITLE\n");
        for (key, page) in &topics {
            if let Some(slug) = key.strip_prefix(&prefix) {
                listing.push_str(&format!("{slug}\t{}\n", page.title));
            }
        }
        anyhow::ensure!(
            listing != "OPERATION\tTITLE\n",
            "No operations published for API docs topic {name:?}"
        );
        return write_output(out, listing.as_bytes());
    }
    let page = match operation {
        None => topic,
        Some(operation) => {
            let operation = operation.to_ascii_lowercase();
            topics.get(&format!("{name}/{operation}")).filter(|_| !operation.contains('/'))
                .ok_or_else(|| anyhow::anyhow!("Unknown API docs operation {operation:?} for topic {name:?}. Run `pup docs api {name} --operations` to list available operations."))?
        }
    };
    write_output(out, &fetch(client, &page.url).await?)
}

#[cfg(not(target_arch = "wasm32"))]
fn write_output(out: &mut impl Write, bytes: &[u8]) -> Result<()> {
    match out.write_all(bytes).and_then(|()| out.flush()) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        Err(error) => Err(error).context("Failed to write API docs"),
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    async fn run_with(
        client: &Client,
        index: &Url,
        topic: Option<&str>,
        out: &mut impl Write,
    ) -> Result<()> {
        run_selected(client, index, topic, None, false, out).await
    }

    #[tokio::test]
    async fn lists_and_fetches_live_operations_scoped_to_topic() {
        let mut server = mockito::Server::new_async().await;
        let base = server.url();
        let index = Url::parse(&format!("{base}/api_reference/llms.txt")).unwrap();
        let client = client_for(&index).unwrap();
        let _root = server.mock("GET", "/api_reference/llms.txt")
            .with_body("- [Monitors](/api/latest/monitors.md)\n- [Metrics](/api/latest/metrics.md)\n- [Empty](/api/latest/empty.md)\n- [Part](/api_reference/part_1/llms.txt)")
            .create_async().await;
        let _shard = server.mock("GET", "/api_reference/part_1/llms.txt")
            .with_body("- [Create a monitor](/api/latest/monitors/create-a-monitor.md)\n- [New operation](/api/latest/monitors/new-operation.md)\n- [Other](/api/latest/metrics/other.md)")
            .create_async().await;
        let page = server
            .mock("GET", "/api/latest/monitors/create-a-monitor.md")
            .match_header("authorization", mockito::Matcher::Missing)
            .with_body("### Request\nrequired field\n### Response\nschema")
            .create_async()
            .await;
        let mut out = Vec::new();
        run_selected(&client, &index, Some("monitors"), None, true, &mut out)
            .await
            .unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "OPERATION\tTITLE\ncreate-a-monitor\tCreate a monitor\nnew-operation\tNew operation\n"
        );
        let mut out = Vec::new();
        run_selected(
            &client,
            &index,
            Some("MONITORS"),
            Some("CREATE-A-MONITOR"),
            false,
            &mut out,
        )
        .await
        .unwrap();
        assert_eq!(out, b"### Request\nrequired field\n### Response\nschema");
        page.assert_async().await;
        for operation in ["other", "../metrics/other", "missing"] {
            let error = run_selected(
                &client,
                &index,
                Some("monitors"),
                Some(operation),
                false,
                &mut Vec::new(),
            )
            .await
            .unwrap_err();
            assert!(error
                .to_string()
                .contains("pup docs api monitors --operations"));
        }
        let error = run_selected(&client, &index, Some("empty"), None, true, &mut Vec::new()).await;
        assert!(error
            .unwrap_err()
            .to_string()
            .contains("No operations published"));
        page.remove_async().await;
        let _missing = server
            .mock("GET", "/api/latest/monitors/create-a-monitor.md")
            .with_status(404)
            .create_async()
            .await;
        let error = run_selected(
            &client,
            &index,
            Some("monitors"),
            Some("create-a-monitor"),
            false,
            &mut Vec::new(),
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("404"));
    }

    #[tokio::test]
    async fn discovers_new_topics_through_shards_and_ignores_cycles_and_endpoints() {
        let mut server = mockito::Server::new_async().await;
        let base = server.url();
        let root = server.mock("GET", "/api_reference/llms.txt").with_body(format!(
            "- [Part]({base}/api_reference/part_1/llms.txt)\n- [Endpoint]({base}/api/latest/monitors/create.md)\n- [Overview]({base}/api/latest.md)"
        )).expect(1).create_async().await;
        let shard = server.mock("GET", "/api_reference/part_1/llms.txt").with_body(format!(
            "- [Root]({base}/api_reference/llms.txt)\n- [New API]({base}/api/latest/new-api.md)\n- [Monitors]({base}/api/latest/monitors.md)"
        )).expect(1).create_async().await;
        let mut out = Vec::new();
        run_with(
            &client_for(&reqwest::Url::parse(&base).unwrap()).unwrap(),
            &reqwest::Url::parse(&format!("{base}/api_reference/llms.txt")).unwrap(),
            None,
            &mut out,
        )
        .await
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("new-api\tNew API"));
        assert!(!text.contains("Endpoint"));
        assert!(!text.contains("Overview"));
        assert!(text.find("monitors").unwrap() < text.find("new-api").unwrap());
        root.assert_async().await;
        shard.assert_async().await;
    }

    #[tokio::test]
    async fn fetches_exact_markdown_and_suggests_unknown_topic() {
        let mut server = mockito::Server::new_async().await;
        let base = server.url();
        let index = reqwest::Url::parse(&format!("{base}/api_reference/llms.txt")).unwrap();
        let client = client_for(&index).unwrap();
        let _root = server
            .mock("GET", "/api_reference/llms.txt")
            .match_header("authorization", mockito::Matcher::Missing)
            .match_header("dd-api-key", mockito::Matcher::Missing)
            .match_header("dd-application-key", mockito::Matcher::Missing)
            .with_body(format!("- [Monitors]({base}/api/latest/monitors.md)"))
            .create_async()
            .await;
        let page = server
            .mock("GET", "/api/latest/monitors.md")
            .match_header("authorization", mockito::Matcher::Missing)
            .match_header("dd-api-key", mockito::Matcher::Missing)
            .match_header("dd-application-key", mockito::Matcher::Missing)
            .with_body("# Monitors\r\n\nno final newline")
            .create_async()
            .await;
        let mut out = Vec::new();
        run_with(&client, &index, Some("MONITORS"), &mut out)
            .await
            .unwrap();
        assert_eq!(out, b"# Monitors\r\n\nno final newline");
        page.assert_async().await;
        let err = run_with(&client, &index, Some("monitor"), &mut out)
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("monitors"));
        assert!(err.contains("pup docs api"));
    }

    #[tokio::test]
    async fn rejects_empty_indexes_http_errors_and_external_redirects() {
        for (status, body, location) in [
            (200, "malformed", ""),
            (503, "unavailable", ""),
            (302, "", "https://example.com/"),
        ] {
            let mut server = mockito::Server::new_async().await;
            let index =
                reqwest::Url::parse(&format!("{}/api_reference/llms.txt", server.url())).unwrap();
            let _mock = server
                .mock("GET", "/api_reference/llms.txt")
                .with_status(status)
                .with_body(body)
                .with_header("location", location)
                .create_async()
                .await;
            assert!(
                run_with(&client_for(&index).unwrap(), &index, None, &mut Vec::new())
                    .await
                    .is_err()
            );
        }
    }

    #[test]
    fn only_accepts_safe_topic_links() {
        let base = reqwest::Url::parse(INDEX_URL).unwrap();
        for link in [
            "https://evil.com/api/latest/monitors.md",
            "http://docs.datadoghq.com/api/latest/monitors.md",
            "https://user@docs.datadoghq.com/api/latest/monitors.md",
            "https://docs.datadoghq.com/api/latest/monitors.md?x=1",
            "https://docs.datadoghq.com/api/latest/monitors/get/extra.md",
        ] {
            assert!(page_from_link("Title", &reqwest::Url::parse(link).unwrap(), &base).is_none());
        }
    }

    #[tokio::test]
    async fn reports_topic_fetch_failure_and_invalid_utf8_index() {
        let mut server = mockito::Server::new_async().await;
        let base = server.url();
        let index = Url::parse(&format!("{base}/api_reference/llms.txt")).unwrap();
        let client = client_for(&index).unwrap();
        let root = server
            .mock("GET", "/api_reference/llms.txt")
            .with_body(format!("- [Monitors]({base}/api/latest/monitors.md)"))
            .create_async()
            .await;
        let _page = server
            .mock("GET", "/api/latest/monitors.md")
            .with_status(404)
            .create_async()
            .await;
        let error = run_with(&client, &index, Some("monitors"), &mut Vec::new())
            .await
            .unwrap_err();
        assert!(error.to_string().contains("404"));
        root.remove_async().await;
        let _invalid = server
            .mock("GET", "/api_reference/llms.txt")
            .with_body(vec![255])
            .create_async()
            .await;
        let error = run_with(&client, &index, None, &mut Vec::new())
            .await
            .unwrap_err();
        assert!(error.to_string().contains("UTF-8"));
    }

    struct FailingWriter(std::io::ErrorKind);
    impl Write for FailingWriter {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(self.0.into())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Err(self.0.into())
        }
    }

    #[test]
    fn handles_closed_pipe_but_propagates_other_write_errors() {
        assert!(write_output(&mut FailingWriter(std::io::ErrorKind::BrokenPipe), b"docs").is_ok());
        assert!(write_output(
            &mut FailingWriter(std::io::ErrorKind::PermissionDenied),
            b"docs"
        )
        .is_err());
    }
}
