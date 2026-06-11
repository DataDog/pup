use crate::config::Config;
/// ACP (Agent Communication Protocol) server that proxies to Datadog Bits AI.
///
/// Starts a local HTTP server implementing ACP and delegates requests to
/// the Datadog Bits AI agent endpoint (/api/unstable/lassie-ng/v1/agents/{id}/messages).
/// Requires OAuth2 with notebooks_read + notebooks_write scopes, or API key auth.
///
/// Protocol:  https://agentcommunicationprotocol.dev/
/// Endpoint:  GET  /agent.json       — agent card
///            POST /runs             — synchronous run
///            POST /runs/stream      — streaming run (SSE)
use anyhow::Result;

#[cfg(not(target_arch = "wasm32"))]
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{tcp::OwnedWriteHalf, TcpListener},
};

pub const DEFAULT_PORT: u16 = 9099;
pub const DEFAULT_HOST: &str = "127.0.0.1";

const LASSIE_BASE: &str = "/api/unstable/lassie-ng/v1";

/// Starts the ACP server on the given host and port.
#[cfg(not(target_arch = "wasm32"))]
pub async fn serve(cfg: &Config, port: u16, host: &str, agent_id: Option<String>) -> Result<()> {
    cfg.validate_auth()?;

    let app_base = format!("https://app.{}", cfg.site);
    let access_token = cfg.access_token.clone();
    let api_key = cfg.api_key.clone();
    let app_key = cfg.app_key.clone();

    // Resolve agent ID: use provided value or auto-discover from GET /agents.
    let agent_id = match agent_id {
        Some(id) if !id.is_empty() => id,
        _ => {
            resolve_agent_id(
                &app_base,
                access_token.as_deref(),
                api_key.as_deref(),
                app_key.as_deref(),
            )
            .await?
        }
    };

    let addr = format!("{host}:{port}");
    let listener = TcpListener::bind(&addr)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to bind to {addr}: {e}"))?;

    let base_url = format!("http://{addr}");
    eprintln!("ACP server running at {base_url}");
    eprintln!("  Agent card:  GET  {base_url}/agent.json");
    eprintln!("  Sync run:    POST {base_url}/runs");
    eprintln!("  Stream run:  POST {base_url}/runs/stream");
    eprintln!("  Datadog Bits AI agent: {app_base}{LASSIE_BASE}/agents/{agent_id}");
    eprintln!("Press Ctrl+C to stop.");

    loop {
        let (stream, peer_addr) = listener.accept().await?;
        let app_base = app_base.clone();
        let agent_id = agent_id.clone();
        let access_token = access_token.clone();
        let api_key = api_key.clone();
        let app_key = app_key.clone();

        tokio::spawn(async move {
            if let Err(e) = handle_connection(
                stream,
                peer_addr,
                &app_base,
                &agent_id,
                access_token.as_deref(),
                api_key.as_deref(),
                app_key.as_deref(),
            )
            .await
            {
                eprintln!("[{peer_addr}] Error: {e}");
            }
        });
    }
}

/// Fetches the first available Datadog Bits AI agent via GET /agents.
#[cfg(not(target_arch = "wasm32"))]
async fn resolve_agent_id(
    app_base: &str,
    access_token: Option<&str>,
    api_key: Option<&str>,
    app_key: Option<&str>,
) -> Result<String> {
    let url = format!("{app_base}{LASSIE_BASE}/agents?limit=1");
    let client = reqwest::Client::new();
    let mut req = client.get(&url).header("Accept", "application/json");
    req = add_auth(req, access_token, api_key, app_key)?;

    let resp = req
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to list agents: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("GET /agents failed (HTTP {status}): {body}");
    }

    let val: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to parse agents response: {e}"))?;

    // Response is an array of agents
    let agents = val
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Unexpected agents response format"))?;

    if agents.is_empty() {
        anyhow::bail!(
            "No Datadog Bits AI agents found. Create one first or pass --agent-id.\n\
             Hint: use the Datadog UI at app.datadoghq.com to create an agent."
        );
    }

    let id = agents[0]
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Agent missing 'id' field"))?;

    eprintln!("Auto-discovered agent: {id}");
    Ok(id.to_string())
}

#[cfg(not(target_arch = "wasm32"))]
async fn handle_connection(
    stream: tokio::net::TcpStream,
    peer_addr: std::net::SocketAddr,
    app_base: &str,
    agent_id: &str,
    access_token: Option<&str>,
    api_key: Option<&str>,
    app_key: Option<&str>,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    // Read request line
    let mut request_line = String::new();
    reader.read_line(&mut request_line).await?;
    let request_line = request_line.trim();
    let parts: Vec<&str> = request_line.splitn(3, ' ').collect();
    if parts.len() < 2 {
        return Ok(());
    }
    let method = parts[0];
    let path = parts[1];

    eprintln!("[{peer_addr}] {method} {path}");

    // Read headers, capture Content-Length
    let mut content_length: usize = 0;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).await?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }
        if let Some(val) = trimmed.to_lowercase().strip_prefix("content-length:") {
            content_length = val.trim().parse().unwrap_or(0);
        }
    }

    // Read body
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body).await?;
    }

    match (method, path) {
        ("GET", "/agent.json") | ("GET", "/.well-known/agent.json") => {
            write_agent_card(&mut writer, agent_id).await
        }
        // ACP endpoints
        ("POST", "/runs") => {
            handle_run(
                &mut writer,
                app_base,
                agent_id,
                access_token,
                api_key,
                app_key,
                &body,
                false,
            )
            .await
        }
        ("POST", "/runs/stream") => {
            handle_run(
                &mut writer,
                app_base,
                agent_id,
                access_token,
                api_key,
                app_key,
                &body,
                true,
            )
            .await
        }
        // OpenAI-compatible endpoints (for opencode / ai-sdk clients)
        ("GET", "/models") | ("GET", "/v1/models") => {
            write_json_response(
                &mut writer,
                200,
                serde_json::json!({
                    "object": "list",
                    "data": [{"id": "datadog-ai", "object": "model", "owned_by": "datadog"}]
                }),
            )
            .await
        }
        ("POST", "/chat/completions") | ("POST", "/v1/chat/completions") => {
            handle_openai_completions(
                &mut writer,
                app_base,
                agent_id,
                access_token,
                api_key,
                app_key,
                &body,
            )
            .await
        }
        ("OPTIONS", _) => {
            writer
                .write_all(
                    b"HTTP/1.1 204 No Content\r\n\
                      Access-Control-Allow-Origin: *\r\n\
                      Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
                      Access-Control-Allow-Headers: Content-Type\r\n\
                      \r\n",
                )
                .await?;
            Ok(())
        }
        _ => write_json_response(&mut writer, 404, serde_json::json!({"error": "not found"})).await,
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn write_agent_card(writer: &mut OwnedWriteHalf, agent_id: &str) -> Result<()> {
    let card = serde_json::json!({
        "name": "Datadog AI Agent",
        "description": "Datadog Bits AI Agent — answers questions about your Datadog environment, metrics, logs, monitors, and more.",
        "version": "1.0.0",
        "url": "",
        "capabilities": {
            "streaming": true
        },
        "metadata": {
            "provider": "Datadog",
            "backend": "datadog-bits-ai",
            "agent_id": agent_id,
            "endpoint": format!("{LASSIE_BASE}/agents/{agent_id}/messages")
        }
    });
    write_json_response(writer, 200, card).await
}

/// Handles both sync (`POST /runs`) and streaming (`POST /runs/stream`) ACP requests.
#[cfg(not(target_arch = "wasm32"))]
#[allow(clippy::too_many_arguments)]
async fn handle_run(
    writer: &mut OwnedWriteHalf,
    app_base: &str,
    agent_id: &str,
    access_token: Option<&str>,
    api_key: Option<&str>,
    app_key: Option<&str>,
    body: &[u8],
    streaming: bool,
) -> Result<()> {
    let acp_req: serde_json::Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(e) => {
            return write_json_response(
                writer,
                400,
                serde_json::json!({"error": format!("invalid JSON: {e}")}),
            )
            .await;
        }
    };

    let message = match extract_acp_message(&acp_req) {
        Some(m) => m,
        None => {
            return write_json_response(
                writer,
                400,
                serde_json::json!({"error": "missing user message in input"}),
            )
            .await;
        }
    };

    let acp_run_id = uuid::Uuid::new_v4().to_string();

    // Bits AI request: {"input": "...", "stream": bool}
    let lassie_body = serde_json::json!({
        "input": message,
        "stream": streaming,
    });

    let url = format!("{app_base}{LASSIE_BASE}/agents/{agent_id}/messages");
    let client = reqwest::Client::new();
    let mut req = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header(
            "Accept",
            if streaming {
                "text/event-stream"
            } else {
                "application/json"
            },
        );

    req = match add_auth(req, access_token, api_key, app_key) {
        Ok(r) => r,
        Err(_) => {
            return write_json_response(
                writer,
                401,
                serde_json::json!({"error": "no authentication configured"}),
            )
            .await;
        }
    };

    let resp = match req.json(&lassie_body).send().await {
        Ok(r) => r,
        Err(e) => {
            return write_json_response(
                writer,
                502,
                serde_json::json!({"error": format!("upstream request failed: {e}")}),
            )
            .await;
        }
    };

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let err_body = resp.text().await.unwrap_or_default();
        return write_json_response(
            writer,
            status,
            serde_json::json!({"error": format!("Datadog Bits AI error (HTTP {status}): {err_body}")}),
        )
        .await;
    }

    if streaming {
        stream_lassie_to_acp(writer, resp, &acp_run_id, agent_id).await
    } else {
        collect_lassie_to_acp(writer, resp, &acp_run_id, agent_id).await
    }
}

/// Handles OpenAI-compatible POST /chat/completions, proxying to Datadog Bits AI.
#[cfg(not(target_arch = "wasm32"))]
#[allow(clippy::too_many_arguments)]
async fn handle_openai_completions(
    writer: &mut OwnedWriteHalf,
    app_base: &str,
    agent_id: &str,
    access_token: Option<&str>,
    api_key: Option<&str>,
    app_key: Option<&str>,
    body: &[u8],
) -> Result<()> {
    let req: serde_json::Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(e) => {
            return write_json_response(
                writer,
                400,
                serde_json::json!({"error": {"message": format!("invalid JSON: {e}"), "type": "invalid_request_error"}}),
            )
            .await;
        }
    };

    let streaming = req.get("stream").and_then(|v| v.as_bool()).unwrap_or(false);
    let model = req
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("datadog-ai")
        .to_string();

    // Extract the last user message from the OpenAI messages array
    let message = match extract_openai_message(&req) {
        Some(m) => m,
        None => {
            return write_json_response(
                writer,
                400,
                serde_json::json!({"error": {"message": "no user message found", "type": "invalid_request_error"}}),
            )
            .await;
        }
    };

    let lassie_body = serde_json::json!({
        "input": message,
        "stream": streaming,
    });

    let url = format!("{app_base}{LASSIE_BASE}/agents/{agent_id}/messages");
    let client = reqwest::Client::new();
    let mut lreq = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header(
            "Accept",
            if streaming {
                "text/event-stream"
            } else {
                "application/json"
            },
        );

    lreq = match add_auth(lreq, access_token, api_key, app_key) {
        Ok(r) => r,
        Err(_) => {
            return write_json_response(
                writer,
                401,
                serde_json::json!({"error": {"message": "no authentication configured", "type": "authentication_error"}}),
            )
            .await;
        }
    };

    let resp = match lreq.json(&lassie_body).send().await {
        Ok(r) => r,
        Err(e) => {
            return write_json_response(
                writer,
                502,
                serde_json::json!({"error": {"message": format!("upstream error: {e}"), "type": "api_error"}}),
            )
            .await;
        }
    };

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let err_body = resp.text().await.unwrap_or_default();
        return write_json_response(
            writer,
            status,
            serde_json::json!({"error": {"message": format!("Datadog Bits AI error (HTTP {status}): {err_body}"), "type": "api_error"}}),
        )
        .await;
    }

    if streaming {
        stream_lassie_as_openai(writer, resp, &model).await
    } else {
        collect_lassie_as_openai(writer, resp, &model).await
    }
}

/// Collects Datadog Bits AI response and returns OpenAI chat completion format.
#[cfg(not(target_arch = "wasm32"))]
async fn collect_lassie_as_openai(
    writer: &mut OwnedWriteHalf,
    resp: reqwest::Response,
    model: &str,
) -> Result<()> {
    let val: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to parse Datadog Bits AI response: {e}"))?;

    let text = extract_lassie_text(&val);
    let completion_id = format!("chatcmpl-{}", uuid::Uuid::new_v4());
    let created = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let body = serde_json::json!({
        "id": completion_id,
        "object": "chat.completion",
        "created": created,
        "model": model,
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": text},
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 0,
            "completion_tokens": 0,
            "total_tokens": 0
        }
    });
    write_json_response(writer, 200, body).await
}

/// Streams Datadog Bits AI SSE as OpenAI chat completion chunks.
#[cfg(not(target_arch = "wasm32"))]
async fn stream_lassie_as_openai(
    writer: &mut OwnedWriteHalf,
    resp: reqwest::Response,
    model: &str,
) -> Result<()> {
    use futures::StreamExt;

    writer
        .write_all(
            b"HTTP/1.1 200 OK\r\n\
              Content-Type: text/event-stream\r\n\
              Cache-Control: no-cache\r\n\
              X-Accel-Buffering: no\r\n\
              Access-Control-Allow-Origin: *\r\n\
              \r\n",
        )
        .await?;

    let completion_id = format!("chatcmpl-{}", uuid::Uuid::new_v4());
    let created = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Send role chunk first
    let role_chunk = serde_json::json!({
        "id": &completion_id,
        "object": "chat.completion.chunk",
        "created": created,
        "model": model,
        "choices": [{"index": 0, "delta": {"role": "assistant", "content": ""}, "finish_reason": null}]
    });
    writer
        .write_all(format!("data: {}\n\n", role_chunk).as_bytes())
        .await?;

    let mut buffer = String::new();
    let mut bytes_stream = resp.bytes_stream();

    while let Some(chunk_result) = bytes_stream.next().await {
        let chunk = chunk_result.map_err(|e| anyhow::anyhow!("stream read error: {e}"))?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(end) = buffer.find("\n\n") {
            let event_block = buffer[..end].to_string();
            buffer = buffer[end + 2..].to_string();

            for line in event_block.lines() {
                let Some(data_str) = line.strip_prefix("data: ") else {
                    continue;
                };

                if data_str.trim() == "[DONE]" {
                    // Send finish chunk then [DONE]
                    let finish = serde_json::json!({
                        "id": &completion_id,
                        "object": "chat.completion.chunk",
                        "created": created,
                        "model": model,
                        "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]
                    });
                    writer
                        .write_all(format!("data: {finish}\n\ndata: [DONE]\n\n").as_bytes())
                        .await?;
                    return Ok(());
                }

                let Ok(val) = serde_json::from_str::<serde_json::Value>(data_str) else {
                    continue;
                };

                if val
                    .get("message_type")
                    .and_then(|v| v.as_str())
                    .is_some_and(|t| t == "assistant_message")
                {
                    let content = val.get("content").and_then(|v| v.as_str()).unwrap_or("");
                    if !content.is_empty() {
                        let oai_chunk = serde_json::json!({
                            "id": &completion_id,
                            "object": "chat.completion.chunk",
                            "created": created,
                            "model": model,
                            "choices": [{"index": 0, "delta": {"content": content}, "finish_reason": null}]
                        });
                        writer
                            .write_all(format!("data: {oai_chunk}\n\n").as_bytes())
                            .await?;
                    }
                }
            }
        }
    }

    // Fallback close
    let finish = serde_json::json!({
        "id": &completion_id,
        "object": "chat.completion.chunk",
        "created": created,
        "model": model,
        "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]
    });
    writer
        .write_all(format!("data: {finish}\n\ndata: [DONE]\n\n").as_bytes())
        .await?;
    Ok(())
}

/// Collects the full Datadog Bits AI response and returns a single ACP run response.
#[cfg(not(target_arch = "wasm32"))]
async fn collect_lassie_to_acp(
    writer: &mut OwnedWriteHalf,
    resp: reqwest::Response,
    run_id: &str,
    agent_id: &str,
) -> Result<()> {
    let val: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to parse Datadog Bits AI response: {e}"))?;

    let text = extract_lassie_text(&val);
    let lassie_run_id = val.get("run_id").and_then(|v| v.as_str()).unwrap_or(run_id);

    let body = serde_json::json!({
        "run_id": run_id,
        "agent_id": agent_id,
        "session_id": lassie_run_id,
        "status": "completed",
        "output": [{"role": "assistant", "content": [{"type": "text", "text": text}]}]
    });
    write_json_response(writer, 200, body).await
}

/// Streams Datadog Bits AI SSE output as ACP SSE events.
#[cfg(not(target_arch = "wasm32"))]
async fn stream_lassie_to_acp(
    writer: &mut OwnedWriteHalf,
    resp: reqwest::Response,
    acp_run_id: &str,
    agent_id: &str,
) -> Result<()> {
    use futures::StreamExt;

    writer
        .write_all(
            b"HTTP/1.1 200 OK\r\n\
              Content-Type: text/event-stream\r\n\
              Cache-Control: no-cache\r\n\
              X-Accel-Buffering: no\r\n\
              Access-Control-Allow-Origin: *\r\n\
              \r\n",
        )
        .await?;

    write_sse_event(
        writer,
        "run_created",
        &serde_json::json!({
            "type": "run_created",
            "run_id": acp_run_id,
            "agent_id": agent_id,
            "status": "running"
        }),
    )
    .await?;

    let mut buffer = String::new();
    let mut lassie_run_id = String::new();
    let mut message_started = false;
    let mut run_completed = false;
    let mut full_text = String::new();
    let mut bytes_stream = resp.bytes_stream();

    while let Some(chunk_result) = bytes_stream.next().await {
        let chunk = chunk_result.map_err(|e| anyhow::anyhow!("stream read error: {e}"))?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(end) = buffer.find("\n\n") {
            let event_block = buffer[..end].to_string();
            buffer = buffer[end + 2..].to_string();

            for line in event_block.lines() {
                let Some(data_str) = line.strip_prefix("data: ") else {
                    continue;
                };

                // [DONE] marks end of SSE stream
                if data_str.trim() == "[DONE]" {
                    // Only emit run_completed here if stop_reason didn't already send it
                    if message_started && !run_completed {
                        write_sse_event(
                            writer,
                            "run_completed",
                            &serde_json::json!({
                                "type": "run_completed",
                                "run_id": acp_run_id,
                                "agent_id": agent_id,
                                "session_id": &lassie_run_id,
                                "status": "completed",
                                "output": [{"role": "assistant", "content": [{"type": "text", "text": &full_text}]}]
                            }),
                        )
                        .await?;
                    }
                    return Ok(());
                }

                let Ok(val) = serde_json::from_str::<serde_json::Value>(data_str) else {
                    continue;
                };

                let msg_type = val
                    .get("message_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                // Capture run_id from any event that carries it
                if lassie_run_id.is_empty() {
                    if let Some(rid) = val.get("run_id").and_then(|v| v.as_str()) {
                        lassie_run_id = rid.to_string();
                    }
                }

                match msg_type {
                    "assistant_message" => {
                        let content = val.get("content").and_then(|v| v.as_str()).unwrap_or("");

                        if !message_started {
                            message_started = true;
                            write_sse_event(
                                writer,
                                "message_created",
                                &serde_json::json!({
                                    "type": "message_created",
                                    "run_id": acp_run_id,
                                    "message_id": val.get("id").and_then(|v| v.as_str()),
                                    "role": "assistant"
                                }),
                            )
                            .await?;
                        }

                        if !content.is_empty() {
                            full_text.push_str(content);
                            write_sse_event(
                                writer,
                                "message_chunk",
                                &serde_json::json!({
                                    "type": "message_chunk",
                                    "run_id": acp_run_id,
                                    "delta": {
                                        "role": "assistant",
                                        "content": [{"type": "text", "text": content}]
                                    }
                                }),
                            )
                            .await?;
                        }
                    }
                    "stop_reason" => {
                        let stop = val
                            .get("stop_reason")
                            .and_then(|v| v.as_str())
                            .unwrap_or("end_turn");

                        // Emit message_completed + run_completed on any terminal stop reason
                        let status = match stop {
                            "end_turn" | "max_steps" | "max_tokens_exceeded" => "completed",
                            "cancelled" => "cancelled",
                            "error" => "error",
                            _ => "completed",
                        };

                        if message_started {
                            write_sse_event(
                                writer,
                                "message_completed",
                                &serde_json::json!({
                                    "type": "message_completed",
                                    "run_id": acp_run_id,
                                    "message": {
                                        "role": "assistant",
                                        "content": [{"type": "text", "text": &full_text}]
                                    }
                                }),
                            )
                            .await?;
                        }

                        write_sse_event(
                            writer,
                            "run_completed",
                            &serde_json::json!({
                                "type": "run_completed",
                                "run_id": acp_run_id,
                                "agent_id": agent_id,
                                "session_id": &lassie_run_id,
                                "status": status,
                                "output": [{"role": "assistant", "content": [{"type": "text", "text": &full_text}]}]
                            }),
                        )
                        .await?;
                        run_completed = true;

                        // Don't return yet — wait for [DONE] to cleanly close
                    }
                    // Skip: thinking_chunk, ping, tool_call_message, usage_statistics
                    _ => {}
                }
            }
        }
    }

    // Stream closed without [DONE] — emit completion if we got any content
    if message_started && !run_completed {
        write_sse_event(
            writer,
            "run_completed",
            &serde_json::json!({
                "type": "run_completed",
                "run_id": acp_run_id,
                "agent_id": agent_id,
                "session_id": &lassie_run_id,
                "status": "completed",
                "output": [{"role": "assistant", "content": [{"type": "text", "text": &full_text}]}]
            }),
        )
        .await?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extracts assistant text content from a non-streaming LettaResponse.
fn extract_lassie_text(val: &serde_json::Value) -> String {
    let Some(messages) = val.get("messages").and_then(|v| v.as_array()) else {
        return String::new();
    };
    let mut parts = Vec::new();
    for msg in messages {
        // Simple format: {"role": "assistant", "content": "..."}
        if msg.get("role").and_then(|v| v.as_str()) == Some("assistant") {
            if let Some(content) = msg.get("content").and_then(|v| v.as_str()) {
                if !content.is_empty() {
                    parts.push(content.to_string());
                }
            }
        }
        // Nested format: {"message_type": "assistant_message", "assistant_message": {"content": "..."}}
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

/// Extracts the first user text message from an ACP input array.
fn extract_acp_message(req: &serde_json::Value) -> Option<String> {
    let input = req.get("input")?.as_array()?;
    for msg in input {
        if msg.get("role").and_then(|r| r.as_str()) != Some("user") {
            continue;
        }
        if let Some(content) = msg.get("content") {
            if let Some(s) = content.as_str() {
                if !s.is_empty() {
                    return Some(s.to_string());
                }
            } else if let Some(blocks) = content.as_array() {
                for block in blocks {
                    if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                        if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                            if !text.is_empty() {
                                return Some(text.to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// Extracts the last user message text from an OpenAI messages array.
fn extract_openai_message(req: &serde_json::Value) -> Option<String> {
    let messages = req.get("messages")?.as_array()?;
    // Walk in reverse to find the last user message
    for msg in messages.iter().rev() {
        if msg.get("role").and_then(|r| r.as_str()) != Some("user") {
            continue;
        }
        if let Some(content) = msg.get("content") {
            if let Some(s) = content.as_str() {
                if !s.is_empty() {
                    return Some(s.to_string());
                }
            } else if let Some(blocks) = content.as_array() {
                let text: String = blocks
                    .iter()
                    .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("text"))
                    .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("");
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }
    }
    None
}

/// Adds auth headers to a reqwest RequestBuilder.
fn add_auth(
    req: reqwest::RequestBuilder,
    access_token: Option<&str>,
    api_key: Option<&str>,
    app_key: Option<&str>,
) -> Result<reqwest::RequestBuilder> {
    let req = req.header("User-Agent", crate::useragent::get());
    if let Some(token) = access_token {
        return Ok(req.header("Authorization", format!("Bearer {token}")));
    }
    if let (Some(ak), Some(apk)) = (api_key, app_key) {
        return Ok(req
            .header("DD-API-KEY", ak)
            .header("DD-APPLICATION-KEY", apk));
    }
    anyhow::bail!("no authentication configured")
}

// ---------------------------------------------------------------------------
// HTTP helpers
// ---------------------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
async fn write_json_response(
    writer: &mut OwnedWriteHalf,
    status: u16,
    body: serde_json::Value,
) -> Result<()> {
    let body_bytes = body.to_string();
    let reason = http_reason(status);
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\
         Access-Control-Allow-Origin: *\r\n\
         \r\n\
         {body_bytes}",
        body_bytes.len()
    );
    writer.write_all(response.as_bytes()).await?;
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
async fn write_sse_event(
    writer: &mut OwnedWriteHalf,
    event_type: &str,
    data: &serde_json::Value,
) -> Result<()> {
    let data_str = data.to_string();
    let event = format!("event: {event_type}\ndata: {data_str}\n\n");
    writer.write_all(event.as_bytes()).await?;
    Ok(())
}

fn http_reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        502 => "Bad Gateway",
        _ => "Internal Server Error",
    }
}

// ---------------------------------------------------------------------------
// WASM stub
// ---------------------------------------------------------------------------

#[cfg(target_arch = "wasm32")]
pub async fn serve(
    _cfg: &Config,
    _port: u16,
    _host: &str,
    _agent_id: Option<String>,
) -> Result<()> {
    anyhow::bail!("acp serve is not supported in WASM")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_acp_message_text_block() {
        let req = serde_json::json!({
            "input": [{"role": "user", "content": [{"type": "text", "text": "hello world"}]}]
        });
        assert_eq!(extract_acp_message(&req), Some("hello world".to_string()));
    }

    #[test]
    fn test_extract_acp_message_string_content() {
        let req = serde_json::json!({
            "input": [{"role": "user", "content": "simple string message"}]
        });
        assert_eq!(
            extract_acp_message(&req),
            Some("simple string message".to_string())
        );
    }

    #[test]
    fn test_extract_acp_message_skips_non_user() {
        let req = serde_json::json!({
            "input": [
                {"role": "system", "content": [{"type": "text", "text": "system"}]},
                {"role": "user", "content": [{"type": "text", "text": "user msg"}]}
            ]
        });
        assert_eq!(extract_acp_message(&req), Some("user msg".to_string()));
    }

    #[test]
    fn test_extract_acp_message_missing_input() {
        let req = serde_json::json!({"agent_id": "bits-ai"});
        assert_eq!(extract_acp_message(&req), None);
    }

    #[test]
    fn test_extract_lassie_text_simple_role() {
        let val = serde_json::json!({
            "messages": [
                {"role": "user", "content": "hello"},
                {"role": "assistant", "content": "world"}
            ],
            "stop_reason": "end_turn"
        });
        assert_eq!(extract_lassie_text(&val), "world");
    }

    #[test]
    fn test_extract_lassie_text_nested() {
        let val = serde_json::json!({
            "messages": [{
                "message_type": "assistant_message",
                "assistant_message": {"content": "nested content"}
            }]
        });
        assert_eq!(extract_lassie_text(&val), "nested content");
    }

    #[test]
    fn test_extract_lassie_text_empty() {
        let val = serde_json::json!({"messages": []});
        assert_eq!(extract_lassie_text(&val), "");
    }

    #[test]
    fn test_http_reason() {
        assert_eq!(http_reason(200), "OK");
        assert_eq!(http_reason(400), "Bad Request");
        assert_eq!(http_reason(404), "Not Found");
        assert_eq!(http_reason(401), "Unauthorized");
        assert_eq!(http_reason(502), "Bad Gateway");
    }
}
