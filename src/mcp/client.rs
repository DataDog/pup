use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::Config;

const MCP_PATH: &str = "/api/unstable/mcp-server/mcp";

#[derive(Serialize)]
struct JsonRpcRequest {
    jsonrpc: &'static str,
    id: u64,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
}

#[derive(Deserialize, Debug)]
struct JsonRpcResponse {
    id: Option<u64>,
    result: Option<Value>,
    #[allow(dead_code)]
    error: Option<JsonRpcError>,
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
struct JsonRpcError {
    code: i64,
    message: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct McpTool {
    pub name: String,
    pub description: Option<String>,
    #[serde(rename = "inputSchema")]
    #[allow(dead_code)]
    pub input_schema: Option<Value>,
}

#[derive(Deserialize, Debug)]
pub struct McpContentItem {
    #[serde(rename = "type")]
    #[allow(dead_code)]
    pub content_type: String,
    pub text: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct McpToolResult {
    pub content: Vec<McpContentItem>,
    #[serde(rename = "isError")]
    pub is_error: Option<bool>,
}

struct McpSession {
    url: String,
    session_id: String,
    token: String,
    client: reqwest::Client,
    next_id: u64,
}

fn mcp_url(cfg: &Config) -> String {
    #[cfg(not(feature = "browser"))]
    {
        if let Ok(endpoint) = std::env::var("PUP_MCP_ENDPOINT") {
            return endpoint;
        }
    }
    let toolsets =
        std::env::var("PUP_MCP_TOOLSETS").unwrap_or_else(|_| "core,security".to_string());
    format!("https://mcp.{}{}?toolsets={}", cfg.site, MCP_PATH, toolsets)
}

impl McpSession {
    fn auth_post(&self, url: &str) -> reqwest::RequestBuilder {
        self.client
            .post(url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("User-Agent", crate::useragent::get())
            .header("Mcp-Session-Id", &self.session_id)
    }

    fn next_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    async fn rpc_call(&mut self, method: &str, params: Option<Value>) -> Result<JsonRpcResponse> {
        let id = self.next_id();
        let body = JsonRpcRequest {
            jsonrpc: "2.0",
            id,
            method: method.to_string(),
            params,
        };

        let resp = self
            .auth_post(&self.url.clone())
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("MCP request failed: {e}"))?;

        let status = resp.status();
        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            anyhow::bail!("MCP server returned HTTP {status}: {body_text}");
        }

        let body_text = resp.text().await?;
        parse_rpc_response(&body_text)
    }
}

fn parse_rpc_response(body: &str) -> Result<JsonRpcResponse> {
    if let Ok(resp) = serde_json::from_str::<JsonRpcResponse>(body) {
        return Ok(resp);
    }

    for line in body.lines() {
        let line = line.trim();
        if let Some(data) = line.strip_prefix("data: ") {
            if let Ok(resp) = serde_json::from_str::<JsonRpcResponse>(data) {
                if resp.id.is_some() {
                    return Ok(resp);
                }
            }
        }
    }

    anyhow::bail!(
        "failed to parse MCP response: {}",
        &body[..body.len().min(500)]
    )
}

async fn connect(cfg: &Config) -> Result<McpSession> {
    let url = mcp_url(cfg);
    let client = reqwest::Client::new();

    // Get MCP OAuth token (will prompt for browser login if needed)
    let token = super::oauth::get_mcp_token(cfg).await?;

    // Initialize MCP session
    let init_body = JsonRpcRequest {
        jsonrpc: "2.0",
        id: 1,
        method: "initialize".to_string(),
        params: Some(serde_json::json!({
            "protocolVersion": "2025-03-26",
            "capabilities": {},
            "clientInfo": {
                "name": "pup",
                "version": crate::version::VERSION,
            }
        })),
    };

    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("User-Agent", crate::useragent::get())
        .json(&init_body)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("MCP initialize failed: {e}"))?;

    let status = resp.status();
    let session_id = resp
        .headers()
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    if !status.is_success() {
        let body_text = resp.text().await.unwrap_or_default();
        anyhow::bail!("MCP initialize returned HTTP {status}: {body_text}");
    }

    let _ = resp.text().await;

    let session_id =
        session_id.ok_or_else(|| anyhow::anyhow!("MCP server did not return a session ID"))?;

    let mut session = McpSession {
        url,
        session_id,
        token,
        client,
        next_id: 1,
    };

    // Send initialized notification
    let notif = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized",
    });
    let _ = session
        .auth_post(&session.url.clone())
        .json(&notif)
        .send()
        .await;

    // Poll for tools to become available
    for _ in 0..10 {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        let resp = session.rpc_call("tools/list", None).await?;
        if let Some(result) = &resp.result {
            let count = result
                .get("tools")
                .and_then(|t| t.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            if count > 0 {
                break;
            }
        }
    }

    Ok(session)
}

pub async fn list_tools(cfg: &Config) -> Result<Vec<McpTool>> {
    let mut session = connect(cfg).await?;
    let resp = session.rpc_call("tools/list", None).await?;
    let result = resp
        .result
        .ok_or_else(|| anyhow::anyhow!("MCP tools/list returned no result"))?;

    let tools: Vec<McpTool> = serde_json::from_value(
        result
            .get("tools")
            .cloned()
            .unwrap_or_else(|| serde_json::json!([])),
    )
    .map_err(|e| anyhow::anyhow!("failed to parse tools list: {e}"))?;

    Ok(tools)
}

pub async fn call_tool(cfg: &Config, tool_name: &str, arguments: Value) -> Result<McpToolResult> {
    let mut session = connect(cfg).await?;
    let params = serde_json::json!({
        "name": tool_name,
        "arguments": arguments,
    });

    let resp = session.rpc_call("tools/call", Some(params)).await?;
    let result = resp
        .result
        .ok_or_else(|| anyhow::anyhow!("MCP tools/call returned no result"))?;

    let tool_result: McpToolResult = serde_json::from_value(result)
        .map_err(|e| anyhow::anyhow!("failed to parse tool result: {e}"))?;

    if tool_result.is_error == Some(true) {
        let error_text = tool_result
            .content
            .iter()
            .filter_map(|c| c.text.as_deref())
            .collect::<Vec<_>>()
            .join("\n");
        anyhow::bail!("MCP tool error: {error_text}");
    }

    Ok(tool_result)
}
