use anyhow::Result;
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::time::{Duration, Instant};

use super::template;
use super::{Runbook, Step};
use crate::config::Config;

// ── Formatting helpers ────────────────────────────────────────────────────────

/// Current time as a full UTC datetime string.
fn now_str() -> String {
    chrono::Utc::now()
        .format("%Y-%m-%d %H:%M:%S UTC")
        .to_string()
}

/// Human-readable elapsed time: "8ms", "1.2s", "2m 5s".
fn fmt_elapsed(d: Duration) -> String {
    let ms = d.as_millis();
    if ms < 1000 {
        format!("{ms}ms")
    } else if ms < 60_000 {
        format!("{:.1}s", d.as_secs_f64())
    } else {
        let m = d.as_secs() / 60;
        let s = d.as_secs() % 60;
        format!("{m}m {s}s")
    }
}

/// One-line preview of what a step will execute (after template rendering).
fn step_preview(step: &Step, vars: &HashMap<String, String>) -> String {
    match step.kind.as_str() {
        "pup" | "shell" => {
            if let Some(run) = &step.run {
                let rendered = template::render(run, vars);
                // Truncate long commands
                let preview = if rendered.len() > 72 {
                    format!("{}…", &rendered[..71])
                } else {
                    rendered
                };
                return format!("$ {preview}");
            }
        }
        "datadog-workflow" => {
            if let Some(wid) = &step.workflow_id {
                return format!("workflow: {}", template::render(wid, vars));
            }
        }
        "http" => {
            let method = step.method.as_deref().unwrap_or("GET");
            if let Some(url) = &step.url {
                return format!("{method} {}", template::render(url, vars));
            }
        }
        "confirm" => {
            if let Some(msg) = &step.message {
                return format!("confirm: {}", template::render(msg, vars));
            }
        }
        _ => {}
    }
    String::new()
}

// ── Main executor ─────────────────────────────────────────────────────────────

/// Execute a runbook sequentially, with variable substitution and status updates.
pub async fn run(cfg: &Config, runbook: &Runbook, vars: HashMap<String, String>) -> Result<()> {
    let total = runbook.steps.len();
    let mut step_vars = vars;

    // Pre-fill defaults for vars not provided by --arg
    if let Some(var_defs) = &runbook.vars {
        for (k, def) in var_defs {
            if !step_vars.contains_key(k) {
                if let Some(default) = &def.default {
                    step_vars.insert(k.clone(), default.clone());
                }
            }
        }
    }

    let runbook_start = Instant::now();
    eprintln!(
        "runbook: {}  ({} steps)  {}",
        runbook.name,
        total,
        now_str()
    );
    if let Some(desc) = &runbook.description {
        eprintln!("  {desc}");
    }

    let mut last_failed = false;
    let mut steps_ok: usize = 0;

    for (i, step) in runbook.steps.iter().enumerate() {
        let step_num = i + 1;
        let step_start = Instant::now();

        // ── Step header ──────────────────────────────────────────────────────
        let next_display = if step_num < total {
            let next = &runbook.steps[i + 1];
            format!(
                "next: step {}/{} — {} ({})",
                step_num + 1,
                total,
                next.name,
                next.kind
            )
        } else {
            "last step".to_string()
        };

        let preview = step_preview(step, &step_vars);
        eprintln!();
        eprintln!(
            "[{step_num}/{total}] {}  ({})  {}",
            step.name,
            step.kind,
            now_str()
        );
        if !preview.is_empty() {
            eprintln!("  {preview}");
        }

        // ── when condition check ─────────────────────────────────────────────
        let when = step.when.as_deref().unwrap_or("on_success");
        if when == "on_success" && last_failed {
            eprintln!("  ⊘ skipped — previous step failed  →  {next_display}");
            continue;
        }

        // ── Execute ──────────────────────────────────────────────────────────
        let result = execute_step(cfg, step, &step_vars).await;
        let elapsed = step_start.elapsed();

        match result {
            Ok(output) => {
                // Print step output, labeled
                if !output.trim().is_empty() {
                    eprintln!("  stdout:");
                    print!("{}", output);
                    if !output.ends_with('\n') {
                        println!();
                    }
                }

                if let Some(capture_var) = &step.capture {
                    step_vars.insert(capture_var.clone(), output.trim().to_string());
                }
                last_failed = false;
                steps_ok += 1;
                eprintln!("  ✓ done  {}  ·  {next_display}", fmt_elapsed(elapsed));
            }
            Err(e) => {
                let optional = step.optional.unwrap_or(false);
                let on_failure = step.on_failure.as_deref().unwrap_or("fail");

                if optional {
                    eprintln!("  ⊘ skipped (optional): {e}");
                    eprintln!("    {}  ·  {next_display}", fmt_elapsed(elapsed));
                    last_failed = false;
                    steps_ok += 1;
                    continue;
                }

                // Show error
                eprintln!("  stderr:");
                eprintln!("    {e}");

                match on_failure {
                    "warn" => {
                        eprintln!("  ⚠ warning  {}  ·  {next_display}", fmt_elapsed(elapsed));
                        last_failed = true;
                    }
                    "confirm" => {
                        eprintln!("  ✗ failed  {}", fmt_elapsed(elapsed));
                        if !prompt_continue(cfg)? {
                            anyhow::bail!("runbook aborted by user at step {step_num}");
                        }
                        last_failed = true;
                    }
                    _ => {
                        eprintln!("  ✗ failed  {}", fmt_elapsed(elapsed));
                        return Err(e.context(format!("step {step_num}/{total}: {}", step.name)));
                    }
                }
            }
        }
    }

    // ── Runbook summary ──────────────────────────────────────────────────────
    let total_elapsed = runbook_start.elapsed();
    let status = if last_failed || steps_ok < total {
        "⚠ done with warnings"
    } else {
        "✓ done"
    };
    eprintln!();
    eprintln!(
        "{status}  {}  {steps_ok}/{total} steps  {}  {}",
        runbook.name,
        fmt_elapsed(total_elapsed),
        now_str()
    );
    Ok(())
}

// ── Step dispatching ──────────────────────────────────────────────────────────

async fn execute_step(cfg: &Config, step: &Step, vars: &HashMap<String, String>) -> Result<String> {
    match step.kind.as_str() {
        "pup" => execute_pup(cfg, step, vars).await,
        "shell" => execute_shell(step, vars).await,
        "datadog-workflow" => execute_datadog_workflow(cfg, step, vars).await,
        "confirm" => execute_confirm(cfg, step, vars),
        "http" => execute_http(cfg, step, vars).await,
        other => anyhow::bail!(
            "unknown step kind '{}' (expected pup, shell, datadog-workflow, confirm, http)",
            other
        ),
    }
}

// ── Kind implementations ──────────────────────────────────────────────────────

async fn execute_pup(cfg: &Config, step: &Step, vars: &HashMap<String, String>) -> Result<String> {
    let run = step
        .run
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("pup step '{}' missing 'run' field", step.name))?;

    let rendered = template::render(run, vars);
    let exe = std::env::current_exe()
        .map_err(|e| anyhow::anyhow!("cannot find current executable: {e}"))?;
    let parts: Vec<String> = rendered.split_whitespace().map(String::from).collect();

    if let Some(poll) = &step.poll {
        let interval = template::parse_duration(&poll.interval)?;
        let timeout = template::parse_duration(&poll.timeout)?;
        let until = poll.until.clone();
        let start = Instant::now();
        let mut baseline: Option<f64> = None;

        loop {
            if start.elapsed() >= timeout {
                anyhow::bail!("poll timeout after {}s", timeout.as_secs());
            }

            let out = tokio::process::Command::new(&exe)
                .args(&parts)
                .args(["--output", "json"])
                .output()
                .await
                .map_err(|e| anyhow::anyhow!("failed to run pup: {e}"))?;

            let stdout = String::from_utf8_lossy(&out.stdout).into_owned();

            if out.status.success() && eval_condition(&stdout, &until, &mut baseline)? {
                return Ok(stdout);
            }

            let elapsed = fmt_elapsed(start.elapsed());
            eprintln!("    ↻ polling ({elapsed} elapsed)  until: {}", poll.until);
            tokio::time::sleep(interval).await;
        }
    } else {
        let out = tokio::process::Command::new(&exe)
            .args(&parts)
            .args(["--output", &cfg.output_format.to_string()])
            .output()
            .await
            .map_err(|e| anyhow::anyhow!("failed to run pup: {e}"))?;

        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            anyhow::bail!("{}", stderr.trim());
        }

        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }
}

async fn execute_shell(step: &Step, vars: &HashMap<String, String>) -> Result<String> {
    let run = step
        .run
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("shell step '{}' missing 'run' field", step.name))?;

    let rendered = template::render(run, vars);
    let optional = step.optional.unwrap_or(false);

    let result = tokio::process::Command::new("sh")
        .args(["-c", &rendered])
        .output()
        .await;

    match result {
        Err(_e) if optional => Ok(String::new()),
        Err(e) => anyhow::bail!("failed to run shell command: {e}"),
        Ok(out) => {
            // Always surface stderr if non-empty (warnings, notices, etc.)
            let stderr = String::from_utf8_lossy(&out.stderr);
            if !stderr.trim().is_empty() {
                eprintln!("  stderr:");
                eprint!("{}", stderr);
                if !stderr.ends_with('\n') {
                    eprintln!();
                }
            }

            if !out.status.success() {
                anyhow::bail!("exited with status {}", out.status.code().unwrap_or(-1));
            }

            Ok(String::from_utf8_lossy(&out.stdout).into_owned())
        }
    }
}

async fn execute_datadog_workflow(
    cfg: &Config,
    step: &Step,
    vars: &HashMap<String, String>,
) -> Result<String> {
    let workflow_id = step.workflow_id.as_ref().ok_or_else(|| {
        anyhow::anyhow!(
            "datadog-workflow step '{}' missing 'workflow_id'",
            step.name
        )
    })?;

    let workflow_id = template::render(workflow_id, vars);

    // Build inputs payload
    let mut inputs_map = serde_json::Map::new();
    if let Some(inputs) = &step.inputs {
        for (k, v) in inputs {
            let rendered_v = template::render(v, vars);
            inputs_map.insert(k.clone(), serde_json::Value::String(rendered_v));
        }
    }
    let body = serde_json::json!({ "meta": { "payload": inputs_map } });

    // Trigger the workflow
    let path = format!("/api/v2/workflows/{workflow_id}/instances");
    let trigger_resp = crate::raw_client::raw_post(cfg, &path, body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to trigger workflow: {e}"))?;

    let instance_id = trigger_resp
        .pointer("/data/id")
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_default();

    // Emit agent-mode metadata hint to stderr
    if cfg.agent_mode || instance_id.is_empty() {
        let meta = serde_json::json!({
            "metadata": {
                "kind": "datadog-workflow",
                "workflow_id": workflow_id,
                "instance_id": instance_id,
                "watch_command": format!(
                    "pup workflows instances get --workflow-id={workflow_id} --instance-id={instance_id}"
                )
            }
        });
        eprintln!(
            "  [agent hint] {}",
            serde_json::to_string(&meta).unwrap_or_default()
        );
    }

    if instance_id.is_empty() {
        return Ok(serde_json::to_string_pretty(&trigger_resp).unwrap_or_default());
    }

    // Auto-poll until terminal state (15s interval)
    let poll_timeout = if let Some(poll) = &step.poll {
        template::parse_duration(&poll.timeout)?
    } else {
        Duration::from_secs(600)
    };

    let start = Instant::now();
    loop {
        if start.elapsed() >= poll_timeout {
            anyhow::bail!("workflow poll timeout after {}s", poll_timeout.as_secs());
        }

        let elapsed = fmt_elapsed(start.elapsed());
        eprintln!("    ↻ [{elapsed} elapsed]  waiting for workflow instance {instance_id}...");
        tokio::time::sleep(Duration::from_secs(15)).await;

        let status_path = format!("/api/v2/workflows/{workflow_id}/instances/{instance_id}");
        let status_resp = crate::raw_client::raw_get(cfg, &status_path, &[])
            .await
            .map_err(|e| anyhow::anyhow!("failed to poll workflow: {e}"))?;

        let state = status_resp
            .pointer("/data/attributes/status")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        eprintln!("    status: {state}");

        match state {
            "success" => {
                return Ok(serde_json::to_string_pretty(&status_resp).unwrap_or_default());
            }
            "failed" | "error" => {
                anyhow::bail!("workflow instance {instance_id} ended with status: {state}");
            }
            _ => {}
        }
    }
}

fn execute_confirm(cfg: &Config, step: &Step, vars: &HashMap<String, String>) -> Result<String> {
    let message = step
        .message
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("confirm step '{}' missing 'message' field", step.name))?;

    let rendered = template::render(message, vars);

    if cfg.auto_approve {
        eprintln!("  {rendered}");
        eprintln!("  [auto-approved via --yes]");
        return Ok(String::new());
    }

    eprint!("  {rendered}  [y/N] ");
    io::stderr().flush().ok();

    let stdin = io::stdin();
    let mut line = String::new();
    stdin.lock().read_line(&mut line)?;

    if matches!(line.trim().to_lowercase().as_str(), "y" | "yes") {
        Ok(String::new())
    } else {
        anyhow::bail!("user declined")
    }
}

async fn execute_http(cfg: &Config, step: &Step, vars: &HashMap<String, String>) -> Result<String> {
    let url = step
        .url
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("http step '{}' missing 'url' field", step.name))?;

    let rendered_url = template::render(url, vars);
    let method = step.method.as_deref().unwrap_or("GET").to_uppercase();
    let accept = step.accept.as_deref().unwrap_or("application/json");

    // Build the request body bytes and effective Content-Type.
    let (body_bytes, effective_ct): (Option<Vec<u8>>, Option<String>) =
        if let Some(file_tmpl) = &step.body_file {
            // body_file takes precedence: read raw bytes from the rendered path.
            let path = template::render(file_tmpl, vars);
            let bytes = std::fs::read(&path).map_err(|e| {
                anyhow::anyhow!(
                    "http step '{}' could not read body_file {path:?}: {e}",
                    step.name
                )
            })?;
            let ct = step
                .content_type
                .clone()
                .unwrap_or_else(|| "application/octet-stream".to_string());
            (Some(bytes), Some(ct))
        } else if let Some(body_tmpl) = &step.body {
            let rendered = template::render(body_tmpl, vars);
            let ct = step
                .content_type
                .clone()
                .unwrap_or_else(|| "application/json".to_string());
            (Some(rendered.into_bytes()), Some(ct))
        } else {
            (None, step.content_type.clone())
        };

    // Render header value templates.
    let rendered_headers: Vec<(String, String)> = step
        .headers
        .as_ref()
        .map(|h| {
            h.iter()
                .map(|(k, v)| (k.clone(), template::render(v, vars)))
                .collect()
        })
        .unwrap_or_default();
    let header_refs: Vec<(&str, &str)> = rendered_headers
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    let http_resp = if rendered_url.starts_with('/') {
        // Datadog API path — use authenticated client helper.
        crate::raw_client::raw_request(
            cfg,
            &method,
            &rendered_url,
            &[],
            body_bytes,
            effective_ct.as_deref(),
            accept,
            &header_refs,
        )
        .await?
    } else {
        // External URL — plain reqwest, no DD auth.
        let m = reqwest::Method::from_bytes(method.as_bytes())
            .map_err(|_| anyhow::anyhow!("unsupported HTTP method: {method}"))?;
        let mut req = reqwest::Client::new()
            .request(m, &rendered_url)
            .header("Accept", accept);
        for (k, v) in &header_refs {
            req = req.header(*k, *v);
        }
        if let Some(b) = body_bytes {
            if let Some(ct) = effective_ct.as_deref() {
                req = req.header("Content-Type", ct);
            }
            req = req.body(b);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("HTTP {method} failed: {e}"))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("HTTP {status}: {text}");
        }
        let resp_ct = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let bytes = if resp.status() == reqwest::StatusCode::NO_CONTENT {
            vec![]
        } else {
            resp.bytes().await?.to_vec()
        };
        crate::raw_client::HttpResponse {
            content_type: resp_ct,
            bytes,
        }
    };

    decode_http_response(http_resp, step, vars)
}

/// Decode an [`HttpResponse`] into a string suitable for capture / display.
///
/// - If `output_file` is set on the step, the raw bytes are written there and a
///   summary string is returned.
/// - JSON responses are pretty-printed.
/// - YAML, CSV, and plain-text responses are returned as-is.
/// - Unrecognised binary responses that cannot be decoded as UTF-8 require
///   `output_file` to be set; otherwise an error is returned.
fn decode_http_response(
    resp: crate::raw_client::HttpResponse,
    step: &Step,
    vars: &HashMap<String, String>,
) -> Result<String> {
    // Write to output_file if specified.
    if let Some(file_tmpl) = &step.output_file {
        let path = template::render(file_tmpl, vars);
        std::fs::write(&path, &resp.bytes).map_err(|e| {
            anyhow::anyhow!(
                "http step '{}' could not write output_file {path:?}: {e}",
                step.name
            )
        })?;
        return Ok(format!("written {} bytes to {}", resp.bytes.len(), path));
    }

    if resp.bytes.is_empty() {
        return Ok(String::new());
    }

    let ct = resp.content_type.to_lowercase();
    let ct = ct.split(';').next().unwrap_or("").trim(); // strip e.g. "; charset=utf-8"

    // JSON — pretty-print.
    if ct.is_empty() || ct.contains("json") {
        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&resp.bytes) {
            return Ok(serde_json::to_string_pretty(&v).unwrap_or_default());
        }
    }

    // Text-based formats — return as UTF-8 string.
    if ct.starts_with("text/")
        || ct.contains("yaml")
        || ct.contains("csv")
        || ct.contains("xml")
        || ct.contains("html")
    {
        return String::from_utf8(resp.bytes).map_err(|e| {
            anyhow::anyhow!(
                "http step '{}' response claimed text content-type ({ct}) but is not valid UTF-8: {e}",
                step.name
            )
        });
    }

    // Binary or unknown — try UTF-8, otherwise tell the user to use output_file.
    String::from_utf8(resp.bytes).map_err(|_| {
        anyhow::anyhow!(
            "http step '{}' received binary response (content-type: {ct}); \
             add 'output_file: /path/to/save' to the step to write it to disk",
            step.name
        )
    })
}

// ── Poll condition evaluator ──────────────────────────────────────────────────

fn eval_condition(output: &str, condition: &str, baseline: &mut Option<f64>) -> Result<bool> {
    let condition = condition.trim();

    if condition == "empty" {
        let v: serde_json::Value = serde_json::from_str(output).unwrap_or(serde_json::Value::Null);
        return Ok(match &v {
            serde_json::Value::Array(arr) => arr.is_empty(),
            serde_json::Value::Null => true,
            serde_json::Value::String(s) => s.is_empty(),
            _ => false,
        });
    }

    if condition == "decreasing" {
        let v: serde_json::Value = serde_json::from_str(output).unwrap_or(serde_json::Value::Null);
        let current = extract_numeric(&v);
        if let Some(current) = current {
            let result = baseline.map(|b| current < b).unwrap_or(false);
            *baseline = Some(current);
            return Ok(result);
        }
        return Ok(false);
    }

    if let Some(rest) = condition.strip_prefix("status ==") {
        let expected = rest.trim().trim_matches('"');
        let v: serde_json::Value = serde_json::from_str(output).unwrap_or(serde_json::Value::Null);
        let status = v.get("status").and_then(|s| s.as_str()).unwrap_or("");
        return Ok(status == expected);
    }

    if let Some(rest) = condition.strip_prefix("value <") {
        let threshold: f64 = rest
            .trim()
            .parse()
            .map_err(|_| anyhow::anyhow!("invalid threshold in condition: {condition}"))?;
        let v: serde_json::Value = serde_json::from_str(output).unwrap_or(serde_json::Value::Null);
        if let Some(n) = extract_numeric(&v) {
            return Ok(n < threshold);
        }
        return Ok(false);
    }

    Ok(true)
}

fn extract_numeric(v: &serde_json::Value) -> Option<f64> {
    match v {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::Object(map) => map.get("value").and_then(|v| v.as_f64()),
        _ => None,
    }
}

// ── Prompt ────────────────────────────────────────────────────────────────────

fn prompt_continue(cfg: &Config) -> Result<bool> {
    if cfg.auto_approve {
        return Ok(true);
    }
    eprint!("  Continue despite failure? [y/N] ");
    io::stderr().flush().ok();
    let stdin = io::stdin();
    let mut line = String::new();
    stdin.lock().read_line(&mut line)?;
    Ok(matches!(line.trim().to_lowercase().as_str(), "y" | "yes"))
}
