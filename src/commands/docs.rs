use anyhow::Result;
use std::io::Write;

const DOCS_AI_URL: &str = "https://app.datadoghq.com/api/unstable/docs-ai/chat";
// Public key embedded in the Datadog docs site JS — not a user credential.
const DOCS_AI_API_KEY: &str = "ddpub_docsai_nkbIDfPWw4pKuRlLef8aDs2onVqdimFI";

// Allows test overrides without changing the function signature (mirrors PUP_MOCK_SERVER).
fn endpoint_url() -> String {
    std::env::var("PUP_DOCS_AI_URL").unwrap_or_else(|_| DOCS_AI_URL.to_string())
}

/// Ask the Datadog Docs AI a question and stream the response to `out`.
///
/// `out` is injected so callers can redirect output (pass `&mut std::io::stdout()`
/// for normal use) and tests can capture or control write failures.
#[cfg(not(target_arch = "wasm32"))]
pub async fn ask(question: &str, out: &mut impl Write) -> Result<()> {
    use futures::StreamExt;

    if question.trim().is_empty() {
        anyhow::bail!("question cannot be empty");
    }

    let conversation_id = format!("dd_docsai_{}", uuid::Uuid::new_v4());

    let body = serde_json::json!({
        "data": {
            "attributes": {
                "query": question,
                "conversation_id": conversation_id,
                "anchor_url": "https://docs.datadoghq.com/",
                "rewrite_query": true
            }
        }
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build HTTP client: {e}"))?;

    let resp = client
        .post(endpoint_url())
        .header("Content-Type", "application/json")
        .header("Accept", "text/event-stream")
        .header("x-docs-ai-api-key", DOCS_AI_API_KEY)
        .header("User-Agent", crate::useragent::get())
        .json(&body)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Docs AI request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let err_body = match resp.text().await {
            Ok(b) => b,
            Err(e) => format!("<failed to read error body: {e}>"),
        };
        anyhow::bail!("Docs AI error (HTTP {status}): {err_body}");
    }

    let mut buffer = String::new();
    let mut bytes_stream = resp.bytes_stream();
    let mut saw_done = false;
    let mut writer = AnswerWriter::new(out);

    // SSE framing follows the same pattern as bits.rs: accumulate chunks,
    // split on \n\n event boundaries, strip "data: " prefix per line.
    'outer: while let Some(chunk_result) = bytes_stream.next().await {
        let chunk = chunk_result.map_err(|e| anyhow::anyhow!("Stream read error: {e}"))?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(end) = buffer.find("\n\n") {
            let event_block = buffer[..end].to_string();
            buffer = buffer[end + 2..].to_string();

            for line in event_block.lines() {
                let Some(data_str) = line.strip_prefix("data: ") else {
                    continue;
                };
                if data_str.trim() == "[DONE]" {
                    saw_done = true;
                    break 'outer;
                }
                if let Some(event) = extract_sse_event(data_str) {
                    if !writer.write_event(&event)? {
                        return Ok(());
                    }
                }
            }
        }
    }

    // Drain any remaining buffer when the stream ends without [DONE].
    for line in buffer.lines() {
        let Some(data_str) = line.strip_prefix("data: ") else {
            continue;
        };
        if data_str.trim() == "[DONE]" {
            saw_done = true;
        } else if let Some(event) = extract_sse_event(data_str) {
            if !writer.write_event(&event)? {
                return Ok(());
            }
        }
    }

    if !saw_done {
        eprintln!(
            "Warning: Docs AI stream ended without a completion signal — the response may be truncated."
        );
    }
    writer.finish()?;
    Ok(())
}

/// A single renderable event from the Docs AI stream.
///
/// The distinction matters for line breaks: `Status` events are self-contained
/// progress messages that each occupy a line, while `Fragment` events are
/// sub-word pieces of one continuous answer and must be concatenated verbatim.
/// Terminating every event with a newline splits the answer mid-word at
/// whatever boundary the network happened to chunk on.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum DocsEvent {
    /// Progress message (e.g. "Searching documentation..."); stands on its own line.
    Status(String),
    /// A piece of the streamed answer; concatenate with its neighbors, no newline.
    Fragment(String),
}

/// Streams Docs AI events to `out`, inserting line breaks only where they belong.
///
/// Tracks whether the last byte written was a newline so that a status message
/// never lands mid-answer and the output ends with exactly one trailing newline.
struct AnswerWriter<'a, W: Write> {
    out: &'a mut W,
    wrote_any: bool,
    ends_with_newline: bool,
}

impl<'a, W: Write> AnswerWriter<'a, W> {
    fn new(out: &'a mut W) -> Self {
        Self {
            out,
            wrote_any: false,
            ends_with_newline: true,
        }
    }

    /// Returns `Ok(false)` on `BrokenPipe` (caller should stop and return
    /// `Ok(())`), `Ok(true)` on success, `Err` on other I/O errors.
    fn write_event(&mut self, event: &DocsEvent) -> Result<bool> {
        match event {
            DocsEvent::Status(text) => {
                if !self.ends_with_newline && !self.write_raw("\n")? {
                    return Ok(false);
                }
                if !self.write_raw(text)? || !self.write_raw("\n")? {
                    return Ok(false);
                }
                self.ends_with_newline = true;
            }
            DocsEvent::Fragment(text) => {
                if text.is_empty() {
                    return Ok(true);
                }
                if !self.write_raw(text)? {
                    return Ok(false);
                }
                self.ends_with_newline = text.ends_with('\n');
            }
        }
        self.wrote_any = true;
        Ok(true)
    }

    /// Terminate the output with exactly one newline, if anything was written
    /// and it did not already end with one.
    fn finish(&mut self) -> Result<()> {
        if self.wrote_any && !self.ends_with_newline {
            self.write_raw("\n")?;
        }
        Ok(())
    }

    fn write_raw(&mut self, text: &str) -> Result<bool> {
        if let Err(e) = write!(self.out, "{text}") {
            if e.kind() == std::io::ErrorKind::BrokenPipe {
                return Ok(false);
            }
            return Err(anyhow::anyhow!("Failed to write Docs AI response: {e}"));
        }
        if let Err(e) = self.out.flush() {
            if e.kind() == std::io::ErrorKind::BrokenPipe {
                return Ok(false);
            }
            return Err(anyhow::anyhow!("Failed to write Docs AI response: {e}"));
        }
        Ok(true)
    }
}

/// Extract text content from a single SSE data line.
///
/// Tries multiple envelope shapes in priority order. Returns None for non-JSON
/// frames, metadata events, or shapes that carry no printable content.
pub(crate) fn extract_sse_content(data_str: &str) -> Option<String> {
    // Non-JSON lines are intentionally skipped — SSE streams include comment lines,
    // heartbeat pings, and control frames (e.g. "event: ping") that are not JSON.
    let val = serde_json::from_str::<serde_json::Value>(data_str).ok()?;
    let text = val
        .pointer("/data/attributes/content")
        .or_else(|| val.pointer("/attributes/content"))
        .or_else(|| val.get("content"))
        .or_else(|| val.get("text"))
        .and_then(|v| v.as_str())?;
    Some(text.to_string())
}

/// Classify a single SSE data line into a renderable event.
///
/// The live stream tags events with a `type`: `thinking` carries progress
/// messages and `markdown_fragment` carries answer text. Untyped or
/// unrecognized shapes are treated as answer text, since concatenation is the
/// right default for content and a stray status line is the rarer case.
/// Frames with no printable content (e.g. `feedback_token`) yield `None`.
pub(crate) fn extract_sse_event(data_str: &str) -> Option<DocsEvent> {
    let text = extract_sse_content(data_str)?;
    let event_type = serde_json::from_str::<serde_json::Value>(data_str)
        .ok()
        .and_then(|v| {
            v.pointer("/data/attributes/type")
                .or_else(|| v.pointer("/attributes/type"))
                .or_else(|| v.get("type"))
                .and_then(|t| t.as_str())
                .map(str::to_string)
        });

    match event_type.as_deref() {
        Some("thinking") => Some(DocsEvent::Status(text)),
        _ => Some(DocsEvent::Fragment(text)),
    }
}

// ---------------------------------------------------------------------------
// WASM stub
// ---------------------------------------------------------------------------

#[cfg(target_arch = "wasm32")]
pub async fn ask(_question: &str, _out: &mut impl Write) -> Result<()> {
    anyhow::bail!("docs ask is not supported in WASM builds")
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- extract_sse_content unit tests ---

    #[test]
    fn extract_nested_data_attributes_content() {
        let json = r#"{"data":{"attributes":{"content":"hello world"}}}"#;
        assert_eq!(extract_sse_content(json).as_deref(), Some("hello world"));
    }

    #[test]
    fn extract_attributes_content() {
        let json = r#"{"attributes":{"content":"from attributes"}}"#;
        assert_eq!(
            extract_sse_content(json).as_deref(),
            Some("from attributes")
        );
    }

    #[test]
    fn extract_flat_content_field() {
        let json = r#"{"content":"flat content"}"#;
        assert_eq!(extract_sse_content(json).as_deref(), Some("flat content"));
    }

    #[test]
    fn extract_flat_text_field() {
        let json = r#"{"text":"flat text"}"#;
        assert_eq!(extract_sse_content(json).as_deref(), Some("flat text"));
    }

    #[test]
    fn extract_returns_none_for_non_json() {
        assert_eq!(extract_sse_content("not json"), None);
        assert_eq!(extract_sse_content(""), None);
    }

    #[test]
    fn extract_returns_none_when_no_content_field() {
        assert_eq!(extract_sse_content(r#"{"data":{"attributes":{}}}"#), None);
        assert_eq!(extract_sse_content(r#"{"other":"value"}"#), None);
    }

    // --- extract_sse_event classification tests ---

    #[test]
    fn classify_thinking_as_status() {
        let json = r#"{"type":"thinking","content":"Searching documentation..."}"#;
        assert_eq!(
            extract_sse_event(json),
            Some(DocsEvent::Status("Searching documentation...".to_string()))
        );
    }

    #[test]
    fn classify_markdown_fragment_as_fragment() {
        let json = r#"{"type":"markdown_fragment","content":"est introduction to nav"}"#;
        assert_eq!(
            extract_sse_event(json),
            Some(DocsEvent::Fragment("est introduction to nav".to_string()))
        );
    }

    #[test]
    fn classify_untyped_content_as_fragment() {
        assert_eq!(
            extract_sse_event(r#"{"content":"flat content"}"#),
            Some(DocsEvent::Fragment("flat content".to_string()))
        );
        assert_eq!(
            extract_sse_event(r#"{"data":{"attributes":{"content":"nested"}}}"#),
            Some(DocsEvent::Fragment("nested".to_string()))
        );
    }

    #[test]
    fn classify_returns_none_without_printable_content() {
        // feedback_token carries no content/text field.
        assert_eq!(
            extract_sse_event(r#"{"type":"feedback_token","token":"abc"}"#),
            None
        );
        assert_eq!(extract_sse_event("not json"), None);
    }

    // --- ask() integration tests via mockito ---

    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn ask_streams_sse_content_to_out() {
        let _guard = crate::test_utils::ENV_LOCK.lock().await;

        let mut server = mockito::Server::new_async().await;
        let sse_body = concat!(
            "data: {\"content\":\"Hello\"}\n\n",
            "data: {\"content\":\", world\"}\n\n",
            "data: [DONE]\n\n",
        );
        let _mock = server
            .mock("POST", "/api/unstable/docs-ai/chat")
            .with_status(200)
            .with_header("content-type", "text/event-stream")
            .with_body(sse_body)
            .create_async()
            .await;

        std::env::set_var(
            "PUP_DOCS_AI_URL",
            format!("{}/api/unstable/docs-ai/chat", server.url()),
        );
        let mut buf = Vec::new();
        let result = ask("what is a monitor?", &mut buf).await;
        std::env::remove_var("PUP_DOCS_AI_URL");

        result.expect("ask() should succeed");
        assert_eq!(
            String::from_utf8(buf).unwrap(),
            "Hello, world\n",
            "answer fragments must be concatenated verbatim, not one per line"
        );
    }

    /// Regression test for the newline-per-delta bug: the Docs AI stream splits
    /// the answer on sub-word token boundaries, so terminating each event with a
    /// newline broke words and URLs at whatever point the network chunked on.
    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn ask_does_not_split_words_or_urls_across_fragments() {
        let _guard = crate::test_utils::ENV_LOCK.lock().await;

        let mut server = mockito::Server::new_async().await;
        let sse_body = concat!(
            "data: {\"type\":\"markdown_fragment\",\"content\":\"Monitoring a Kub\"}\n\n",
            "data: {\"type\":\"markdown_fragment\",\"content\":\"ernetes Cluster: https://docs.datadog\"}\n\n",
            "data: {\"type\":\"markdown_fragment\",\"content\":\"hq.com/containers/\"}\n\n",
            "data: [DONE]\n\n",
        );
        let _mock = server
            .mock("POST", "/api/unstable/docs-ai/chat")
            .with_status(200)
            .with_header("content-type", "text/event-stream")
            .with_body(sse_body)
            .create_async()
            .await;

        std::env::set_var(
            "PUP_DOCS_AI_URL",
            format!("{}/api/unstable/docs-ai/chat", server.url()),
        );
        let mut buf = Vec::new();
        let result = ask("what is a monitor?", &mut buf).await;
        std::env::remove_var("PUP_DOCS_AI_URL");

        result.expect("ask() should succeed");
        let out = String::from_utf8(buf).unwrap();
        assert_eq!(
            out,
            "Monitoring a Kubernetes Cluster: https://docs.datadoghq.com/containers/\n"
        );
        assert!(out.contains("Kubernetes"), "word must not be split");
        assert!(
            out.contains("https://docs.datadoghq.com/containers/"),
            "URL must not be split"
        );
    }

    /// Progress events are self-contained messages, so they keep their own line
    /// even though answer fragments do not.
    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn ask_puts_each_thinking_event_on_its_own_line() {
        let _guard = crate::test_utils::ENV_LOCK.lock().await;

        let mut server = mockito::Server::new_async().await;
        let sse_body = concat!(
            "data: {\"type\":\"thinking\",\"content\":\"Searching documentation...\"}\n\n",
            "data: {\"type\":\"thinking\",\"content\":\"Generating answer...\"}\n\n",
            "data: {\"type\":\"markdown_fragment\",\"content\":\"An ans\"}\n\n",
            "data: {\"type\":\"markdown_fragment\",\"content\":\"wer.\"}\n\n",
            "data: {\"type\":\"feedback_token\",\"token\":\"abc123\"}\n\n",
            "data: [DONE]\n\n",
        );
        let _mock = server
            .mock("POST", "/api/unstable/docs-ai/chat")
            .with_status(200)
            .with_header("content-type", "text/event-stream")
            .with_body(sse_body)
            .create_async()
            .await;

        std::env::set_var(
            "PUP_DOCS_AI_URL",
            format!("{}/api/unstable/docs-ai/chat", server.url()),
        );
        let mut buf = Vec::new();
        let result = ask("what is a monitor?", &mut buf).await;
        std::env::remove_var("PUP_DOCS_AI_URL");

        result.expect("ask() should succeed");
        assert_eq!(
            String::from_utf8(buf).unwrap(),
            "Searching documentation...\nGenerating answer...\nAn answer.\n",
            "status lines stay separate; the feedback token is not printed"
        );
    }

    /// A status event arriving after answer text must not be appended mid-line.
    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn ask_breaks_line_before_status_following_a_fragment() {
        let _guard = crate::test_utils::ENV_LOCK.lock().await;

        let mut server = mockito::Server::new_async().await;
        let sse_body = concat!(
            "data: {\"type\":\"markdown_fragment\",\"content\":\"partial\"}\n\n",
            "data: {\"type\":\"thinking\",\"content\":\"Still working...\"}\n\n",
            "data: [DONE]\n\n",
        );
        let _mock = server
            .mock("POST", "/api/unstable/docs-ai/chat")
            .with_status(200)
            .with_header("content-type", "text/event-stream")
            .with_body(sse_body)
            .create_async()
            .await;

        std::env::set_var(
            "PUP_DOCS_AI_URL",
            format!("{}/api/unstable/docs-ai/chat", server.url()),
        );
        let mut buf = Vec::new();
        let result = ask("what is a monitor?", &mut buf).await;
        std::env::remove_var("PUP_DOCS_AI_URL");

        result.expect("ask() should succeed");
        assert_eq!(
            String::from_utf8(buf).unwrap(),
            "partial\nStill working...\n"
        );
    }

    /// Output ends with exactly one newline whether or not the last fragment
    /// already supplied one.
    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn ask_ends_with_exactly_one_newline() {
        for (last_fragment, label) in [
            ("tail", "no trailing newline"),
            ("tail\\n", "trailing newline"),
        ] {
            let _guard = crate::test_utils::ENV_LOCK.lock().await;

            let mut server = mockito::Server::new_async().await;
            let sse_body = format!(
                "data: {{\"type\":\"markdown_fragment\",\"content\":\"{last_fragment}\"}}\n\ndata: [DONE]\n\n"
            );
            let _mock = server
                .mock("POST", "/api/unstable/docs-ai/chat")
                .with_status(200)
                .with_header("content-type", "text/event-stream")
                .with_body(sse_body)
                .create_async()
                .await;

            std::env::set_var(
                "PUP_DOCS_AI_URL",
                format!("{}/api/unstable/docs-ai/chat", server.url()),
            );
            let mut buf = Vec::new();
            let result = ask("what is a monitor?", &mut buf).await;
            std::env::remove_var("PUP_DOCS_AI_URL");

            result.expect("ask() should succeed");
            assert_eq!(String::from_utf8(buf).unwrap(), "tail\n", "case: {label}");
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn ask_returns_error_on_non_200() {
        let _guard = crate::test_utils::ENV_LOCK.lock().await;

        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("POST", "/api/unstable/docs-ai/chat")
            .with_status(429)
            .with_header("content-type", "application/json")
            .with_body(r#"{"error":"rate limited"}"#)
            .create_async()
            .await;

        std::env::set_var(
            "PUP_DOCS_AI_URL",
            format!("{}/api/unstable/docs-ai/chat", server.url()),
        );
        let result = ask("what is a monitor?", &mut std::io::sink()).await;
        std::env::remove_var("PUP_DOCS_AI_URL");

        let err = result.expect_err("ask() should fail on HTTP 429");
        let msg = err.to_string();
        assert!(msg.contains("429"), "error should mention HTTP status");
        assert!(
            msg.contains("rate limited"),
            "error should include the response body"
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn ask_rejects_empty_question() {
        let result = ask("", &mut std::io::sink()).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("question cannot be empty"));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn ask_rejects_whitespace_only_question() {
        let result = ask("   ", &mut std::io::sink()).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("question cannot be empty"));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn ask_handles_broken_pipe_gracefully() {
        let _guard = crate::test_utils::ENV_LOCK.lock().await;

        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("POST", "/api/unstable/docs-ai/chat")
            .with_status(200)
            .with_header("content-type", "text/event-stream")
            .with_body("data: {\"content\":\"Hello\"}\n\ndata: [DONE]\n\n")
            .create_async()
            .await;

        std::env::set_var(
            "PUP_DOCS_AI_URL",
            format!("{}/api/unstable/docs-ai/chat", server.url()),
        );
        let result = ask("what is a monitor?", &mut BrokenPipeWriter).await;
        std::env::remove_var("PUP_DOCS_AI_URL");

        assert!(result.is_ok(), "broken pipe should be silently handled");
    }

    /// A writer whose flush always returns BrokenPipe, simulating a closed pipe.
    struct BrokenPipeWriter;

    impl Write for BrokenPipeWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "broken pipe",
            ))
        }
    }
}
