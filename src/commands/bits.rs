use anyhow::Result;

use crate::config::Config;

const LASSIE_BASE: &str = "/api/unstable/lassie-ng/v1";

/// Ask Datadog Bits AI a natural-language question.
///
/// If `interactive` is true, enters a conversation loop after the optional
/// initial `query`, maintaining session context across turns via the
/// `session_id` returned by the API.
#[cfg(not(target_arch = "wasm32"))]
pub async fn ask(
    cfg: &Config,
    query: Option<&str>,
    agent_id: Option<String>,
    stream: bool,
    interactive: bool,
    auto_create: bool,
) -> Result<()> {
    cfg.validate_auth()?;

    if !interactive && query.is_none() {
        anyhow::bail!("a query is required (or use --interactive for a conversation)");
    }

    let app_base = format!("https://{}", cfg.auth_host());

    let agent_id = match agent_id {
        Some(id) if !id.is_empty() => id,
        _ => {
            resolve_agent_id(
                &app_base,
                cfg.access_token.as_deref(),
                cfg.api_key.as_deref(),
                cfg.app_key.as_deref(),
                auto_create,
            )
            .await?
        }
    };

    let mut session_id: Option<String> = None;

    // Send the initial query if provided.
    if let Some(q) = query {
        session_id = send_turn(cfg, &app_base, &agent_id, q, session_id, stream).await?;
    }

    if !interactive {
        return Ok(());
    }

    // Interactive conversation loop.
    eprintln!("Bits AI interactive session (Ctrl+D or 'exit' to quit)");
    eprintln!("─────────────────────────────────────────────────────");

    use std::io::{BufRead, Write};
    let stdin = std::io::stdin();

    loop {
        eprint!("> ");
        let _ = std::io::stderr().flush();

        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => break, // EOF (Ctrl+D)
            Ok(_) => {}
            Err(e) => anyhow::bail!("Failed to read input: {e}"),
        }

        let input = line.trim();
        if input.is_empty() {
            continue;
        }
        if input == "exit" || input == "quit" {
            break;
        }

        session_id = send_turn(cfg, &app_base, &agent_id, input, session_id, stream).await?;
        eprintln!(); // blank line between turns for readability
    }

    Ok(())
}

/// Send one message to Bits AI, print the response, and return the session ID
/// for the next turn.
#[cfg(not(target_arch = "wasm32"))]
async fn send_turn(
    cfg: &Config,
    app_base: &str,
    agent_id: &str,
    query: &str,
    session_id: Option<String>,
    stream: bool,
) -> Result<Option<String>> {
    let url = format!("{app_base}{LASSIE_BASE}/agents/{agent_id}/messages");

    let mut body = serde_json::json!({ "input": query, "stream": stream });
    if let Some(ref sid) = session_id {
        body["session_id"] = serde_json::Value::String(sid.clone());
    }

    let client = reqwest::Client::new();
    let req = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header(
            "Accept",
            if stream {
                "text/event-stream"
            } else {
                "application/json"
            },
        );
    let req = add_auth(req, cfg)?;

    let resp = req
        .json(&body)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Request to Bits AI failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let err_body = resp.text().await.unwrap_or_default();
        anyhow::bail!("Bits AI error (HTTP {status}): {err_body}");
    }

    if stream {
        stream_response(resp).await
    } else {
        collect_response(resp).await
    }
}

/// Stream SSE chunks from Bits AI to stdout. Returns the captured session ID.
#[cfg(not(target_arch = "wasm32"))]
async fn stream_response(resp: reqwest::Response) -> Result<Option<String>> {
    use futures::StreamExt;

    let mut buffer = String::new();
    let mut session_id: Option<String> = None;
    let mut bytes_stream = resp.bytes_stream();

    while let Some(chunk_result) = bytes_stream.next().await {
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
                    println!();
                    return Ok(session_id);
                }

                let Ok(val) = serde_json::from_str::<serde_json::Value>(data_str) else {
                    continue;
                };

                // Capture run_id for session continuity across turns.
                if session_id.is_none() {
                    if let Some(rid) = val.get("run_id").and_then(|v| v.as_str()) {
                        session_id = Some(rid.to_string());
                    }
                }

                if val
                    .get("message_type")
                    .and_then(|v| v.as_str())
                    .is_some_and(|t| t == "assistant_message")
                {
                    if let Some(content) = val.get("content").and_then(|v| v.as_str()) {
                        print!("{content}");
                        use std::io::Write;
                        let _ = std::io::stdout().flush();
                    }
                }
            }
        }
    }

    println!();
    Ok(session_id)
}

/// Collect a non-streaming Bits AI response, print it, and return the session ID.
#[cfg(not(target_arch = "wasm32"))]
async fn collect_response(resp: reqwest::Response) -> Result<Option<String>> {
    let val: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to parse Bits AI response: {e}"))?;

    let session_id = val
        .get("run_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let text = extract_text(&val);
    if text.is_empty() {
        println!("{}", serde_json::to_string_pretty(&val)?);
    } else {
        println!("{text}");
    }

    Ok(session_id)
}

/// Extract assistant text from a non-streaming response.
fn extract_text(val: &serde_json::Value) -> String {
    let Some(messages) = val.get("messages").and_then(|v| v.as_array()) else {
        return String::new();
    };
    let mut parts = Vec::new();
    for msg in messages {
        if msg.get("role").and_then(|v| v.as_str()) == Some("assistant") {
            if let Some(content) = msg.get("content").and_then(|v| v.as_str()) {
                if !content.is_empty() {
                    parts.push(content.to_string());
                }
            }
        }
        if msg
            .get("message_type")
            .and_then(|v| v.as_str())
            .is_some_and(|t| t == "assistant_message")
        {
            if let Some(content) = msg
                .pointer("/assistant_message/content")
                .and_then(|v| v.as_str())
            {
                if !content.is_empty() {
                    parts.push(content.to_string());
                }
            }
        }
    }
    parts.join("\n")
}

/// Resolve the first available Bits AI agent ID from the API.
///
/// If `auto_create` is true and no agents are found, creates a new agent.
#[cfg(not(target_arch = "wasm32"))]
async fn resolve_agent_id(
    app_base: &str,
    access_token: Option<&str>,
    api_key: Option<&str>,
    app_key: Option<&str>,
    auto_create: bool,
) -> Result<String> {
    let url = format!("{app_base}{LASSIE_BASE}/agents?limit=1");
    let client = reqwest::Client::new();
    let req = client.get(&url).header("Accept", "application/json");

    let req = req.header("User-Agent", crate::useragent::get());
    let req = if let Some(token) = access_token {
        req.header("Authorization", format!("Bearer {token}"))
    } else if let (Some(ak), Some(apk)) = (api_key, app_key) {
        req.header("DD-API-KEY", ak)
            .header("DD-APPLICATION-KEY", apk)
    } else {
        anyhow::bail!("no authentication configured");
    };

    let resp = req
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to list Bits AI agents: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("GET /agents failed (HTTP {status}): {body}");
    }

    let val: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to parse agents response: {e}"))?;

    let agents = val.as_array().ok_or_else(|| {
        anyhow::anyhow!(
            "Unexpected response format from Bits AI agents API.\n\
                 Expected a JSON array but got: {}\n\
                 This may indicate an API version mismatch — please report this at\n\
                 https://github.com/DataDog/pup/issues if the issue persists.",
            serde_json::to_string(&val).unwrap_or_else(|_| "<unparseable>".to_string())
        )
    })?;

    if agents.is_empty() {
        if auto_create {
            eprintln!("No Bits AI agents found — creating one automatically...");
            return create_agent(app_base, access_token, api_key, app_key).await;
        }
        anyhow::bail!(
            "No Bits AI agents found in your Datadog organization.\n\
             \n\
             To fix this, create a Bits AI agent at:\n\
               https://app.datadoghq.com/actions/agents/create\n\
             \n\
             Once created, `pup bits ask` will auto-discover it.\n\
             Alternatively, pass --auto-create to have pup create one for you,\n\
             or pass an existing agent ID with --agent-id."
        );
    }

    let id = agents[0]
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Agent missing 'id' field"))?;

    Ok(id.to_string())
}

/// Create a new Bits AI agent and return its ID.
#[cfg(not(target_arch = "wasm32"))]
async fn create_agent(
    app_base: &str,
    access_token: Option<&str>,
    api_key: Option<&str>,
    app_key: Option<&str>,
) -> Result<String> {
    let url = format!("{app_base}{LASSIE_BASE}/agents");
    let body = serde_json::json!({
        "name": "Pup CLI Agent",
        "description": "Auto-created by pup CLI for bits ask",
    });

    let client = reqwest::Client::new();
    let req = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .header("User-Agent", crate::useragent::get());

    let req = if let Some(token) = access_token {
        req.header("Authorization", format!("Bearer {token}"))
    } else if let (Some(ak), Some(apk)) = (api_key, app_key) {
        req.header("DD-API-KEY", ak)
            .header("DD-APPLICATION-KEY", apk)
    } else {
        anyhow::bail!("no authentication configured");
    };

    let resp = req
        .json(&body)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create Bits AI agent: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let err_body = resp.text().await.unwrap_or_default();
        anyhow::bail!("Failed to create Bits AI agent (HTTP {status}): {err_body}");
    }

    let val: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to parse create-agent response: {e}"))?;

    let id = val
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Create-agent response missing 'id' field"))?;

    eprintln!("Created Bits AI agent: {id}");
    Ok(id.to_string())
}

/// Attach auth headers to a request builder.
#[cfg(not(target_arch = "wasm32"))]
fn add_auth(req: reqwest::RequestBuilder, cfg: &Config) -> Result<reqwest::RequestBuilder> {
    let req = req.header("User-Agent", crate::useragent::get());
    if let Some(token) = &cfg.access_token {
        return Ok(req.header("Authorization", format!("Bearer {token}")));
    }
    if let (Some(ak), Some(apk)) = (&cfg.api_key, &cfg.app_key) {
        return Ok(req
            .header("DD-API-KEY", ak.as_str())
            .header("DD-APPLICATION-KEY", apk.as_str()));
    }
    anyhow::bail!("no authentication configured")
}

// ---------------------------------------------------------------------------
// WASM stub
// ---------------------------------------------------------------------------

#[cfg(target_arch = "wasm32")]
pub async fn ask(
    _cfg: &Config,
    _query: Option<&str>,
    _agent_id: Option<String>,
    _stream: bool,
    _interactive: bool,
    _auto_create: bool,
) -> Result<()> {
    anyhow::bail!("bits ask is not supported in WASM builds")
}
