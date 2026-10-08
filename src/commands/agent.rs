use anyhow::Result;

pub fn guide() -> Result<()> {
    println!("datadog-agent (Datadog-Agent) — Operational Reference");
    println!("======================================================");
    println!();
    println!("NOTE: This guide is about the datadog-agent host daemon that collects");
    println!("metrics, traces, and logs from your infrastructure — NOT an AI agent.");
    println!();
    println!("The datadog-agent binary runs on each monitored host and ships data to");
    println!("Datadog. Manage it at scale with 'pup fleet' commands.");
    println!();
    println!("COMMON datadog-agent OPERATIONS:");
    println!("  Install:    https://docs.datadoghq.com/agent/");
    println!("  Start:      sudo datadog-agent start");
    println!("  Stop:       sudo datadog-agent stop");
    println!("  Restart:    sudo datadog-agent restart");
    println!("  Status:     datadog-agent status");
    println!("  Config:     /etc/datadog-agent/datadog.yaml");
    println!();
    println!("FLEET MANAGEMENT (via pup):");
    println!("  pup fleet agents list        List all datadog-agent instances");
    println!("  pup fleet agents versions    Show available datadog-agent versions");
    println!("  pup fleet deployments list   List agent deployment tasks");
    println!("  pup fleet schedules list     List agent schedule tasks");
    println!();
    println!("DOCUMENTATION:");
    println!("  https://docs.datadoghq.com/agent/");
    Ok(())
}

// ---- Response contracts for `pup agent schema <command>` ----

/// Describes how every command writes its output. JSON output is the raw API
/// response body, unchanged; each command's `returns.body` gives its shape.
pub fn output_contract() -> serde_json::Value {
    serde_json::json!({
        "json": "stdout is the Datadog API response body, unchanged (no wrapper). See each command's returns.body.",
        "jq": "--jq runs on that same body; see each command's returns.jq_root.",
        "errors": "Failed calls print `Error: <message>` to stderr, print nothing to stdout, and exit non-zero. A few APIs report errors in the response body with exit 0; see the command's returns.notes.",
        "non_json": "A few commands print plain text instead of JSON in some modes; their returns.notes say so."
    })
}

/// Response contract for a leaf command, keyed by its canonical full path.
/// Commands without a hand-verified shape get a generic passthrough contract.
pub fn returns_for(full_path: &str) -> serde_json::Value {
    match full_path {
        "logs aggregate" => logs_aggregate_returns(),
        "logs search" | "logs list" | "logs query" => logs_search_returns(),
        "traces aggregate" => traces_aggregate_returns(),
        "traces search" => traces_search_returns(),
        "metrics query" | "metrics search" => metrics_query_returns(),
        "security findings schema" => findings_schema_returns(),
        _ => generic_returns(),
    }
}

/// Wrap a JSON:API `data` schema in the response object that carries it.
fn jsonapi_body(data: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "required": ["data"],
        "properties": {
            "data": data,
            "meta": {"type": "object", "description": "Pagination and request status, e.g. meta.page.after"}
        }
    })
}

fn generic_returns() -> serde_json::Value {
    serde_json::json!({
        "documented": false,
        "body": {"description": "Datadog API response body; shape not published"}
    })
}

fn logs_aggregate_returns() -> serde_json::Value {
    serde_json::json!({
        "documented": true,
        "body": jsonapi_body(serde_json::json!({
            "type": "object",
            "properties": {
                "buckets": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "required": ["by", "computes"],
                        "properties": {
                            "by": {"type": "object", "description": "group-by facet -> value"},
                            "computes": {"type": "object", "description": "c0, c1, ... in --compute order; values are numbers, or arrays of {time, value} with --interval"}
                        }
                    }
                }
            }
        })),
        "jq_root": ".data.buckets[]",
        "example": {
            "invocation": "pup logs aggregate --query='service:web' --from=1h --compute=count --group-by=status",
            "response": {
                "data": {"buckets": [{"by": {"status": "error"}, "computes": {"c0": 42}}]},
                "meta": {"status": "done"}
            }
        }
    })
}

fn logs_search_returns() -> serde_json::Value {
    serde_json::json!({
        "documented": true,
        "body": jsonapi_body(serde_json::json!({
            "type": "array",
            "items": {
                "type": "object",
                "required": ["attributes", "id", "type"],
                "properties": {
                    "id": {"type": "string"},
                    "type": {"const": "log"},
                    "attributes": {
                        "type": "object",
                        "properties": {
                            "timestamp": {"type": "string"},
                            "message": {"type": "string"},
                            "service": {"type": "string"},
                            "host": {"type": "string"},
                            "status": {"type": "string"},
                            "tags": {"type": "array", "items": {"type": "string"}},
                            "attributes": {"type": "object", "description": "Custom log attributes (the @-prefixed facets), nested one level deeper"}
                        }
                    }
                }
            }
        })),
        "jq_root": ".data[]",
        "notes": ["When more results exist, meta.page.after holds the cursor; pass it back with --cursor."],
        "example": {
            "invocation": "pup logs search --query='status:error' --from=1h --limit=1",
            "response": {
                "data": [{
                    "id": "AQAAAY...",
                    "type": "log",
                    "attributes": {
                        "timestamp": "2026-01-01T00:00:00Z",
                        "message": "upstream timeout",
                        "service": "web",
                        "status": "error",
                        "attributes": {"http": {"status_code": 504}}
                    }
                }],
                "meta": {"page": {"after": "eyJ..."}}
            }
        }
    })
}

fn traces_aggregate_returns() -> serde_json::Value {
    serde_json::json!({
        "documented": true,
        "body": jsonapi_body(serde_json::json!({
            "type": "array",
            "items": {
                "type": "object",
                "required": ["attributes"],
                "properties": {
                    "id": {"type": "string"},
                    "type": {"const": "bucket"},
                    "attributes": {
                        "type": "object",
                        "properties": {
                            "by": {"type": "object", "description": "group-by facet -> value"},
                            "compute": {"type": "object", "description": "c0 -> number (singular key, unlike logs aggregate)"}
                        }
                    }
                }
            }
        })),
        "jq_root": ".data[].attributes",
        "notes": ["Durations (@duration) are in nanoseconds."],
        "example": {
            "invocation": "pup traces aggregate --query='service:web' --from=1h --compute=count --group-by=resource_name",
            "response": {
                "data": [{"id": "f8527b82-...", "type": "bucket", "attributes": {"by": {"resource_name": "GET /"}, "compute": {"c0": 2378}}}],
                "meta": {"status": "done"}
            }
        }
    })
}

fn traces_search_returns() -> serde_json::Value {
    serde_json::json!({
        "documented": true,
        "body": jsonapi_body(serde_json::json!({
            "type": "array",
            "items": {
                "type": "object",
                "required": ["attributes", "id", "type"],
                "properties": {
                    "id": {"type": "string"},
                    "type": {"const": "spans"},
                    "attributes": {
                        "type": "object",
                        "properties": {
                            "service": {"type": "string"},
                            "resource_name": {"type": "string"},
                            "env": {"type": "string"},
                            "host": {"type": "string"},
                            "trace_id": {"type": "string"},
                            "span_id": {"type": "string"},
                            "parent_id": {"type": "string"},
                            "start_timestamp": {"type": "string"},
                            "end_timestamp": {"type": "string"},
                            "tags": {"type": "array", "items": {"type": "string"}},
                            "attributes": {"type": "object"},
                            "custom": {"type": "object", "description": "Custom span attributes (the @-prefixed facets)"}
                        }
                    }
                }
            }
        })),
        "jq_root": ".data[]",
        "notes": [
            "Field names differ from logs search: start_timestamp (not timestamp), resource_name.",
            "When more results exist, meta.page.after holds the cursor; pass it back with --cursor."
        ]
    })
}

fn metrics_query_returns() -> serde_json::Value {
    serde_json::json!({
        "documented": true,
        "body": {
            "type": "object",
            "properties": {
                "status": {"type": "string", "description": "\"ok\" or \"error\""},
                "error": {"type": "string"},
                "query": {"type": "string"},
                "from_date": {"type": "integer", "description": "ms since epoch"},
                "to_date": {"type": "integer", "description": "ms since epoch"},
                "series": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "metric": {"type": "string"},
                            "scope": {"type": "string"},
                            "tag_set": {"type": "array", "items": {"type": "string"}},
                            "pointlist": {"type": "array", "items": {"type": "array", "description": "[timestamp_ms, value|null]"}}
                        }
                    }
                }
            }
        },
        "jq_root": ".series[]",
        "notes": [
            "An invalid query exits 0 with status == \"error\" and the reason in error; check status."
        ],
        "example": {
            "invocation": "pup metrics query --query='avg:system.cpu.user{env:prod} by {host}' --from=1h",
            "response": {"status": "ok", "query": "avg:system.cpu.user{env:prod} by {host}", "series": [{"metric": "system.cpu.user", "scope": "env:prod,host:web-1", "pointlist": [[1767225600000.0, 12.5]]}]}
        }
    })
}

fn findings_schema_returns() -> serde_json::Value {
    serde_json::json!({
        "documented": true,
        "body": {
            "type": "array",
            "items": {
                "type": "object",
                "required": ["path", "type", "section", "description"],
                "properties": {
                    "path": {"type": "string", "description": "Query path, e.g. @advisory.cve"},
                    "type": {"type": "string", "description": "e.g. string, integer, array (string)"},
                    "section": {"type": "string", "description": "Top-level namespace, e.g. Advisory"},
                    "description": {"type": "string"}
                }
            }
        },
        "jq_root": ".[]",
        "notes": [
            "Without --search or --section this command prints the full reference (~200 KB) as plain markdown on stdout, not JSON. Pass a filter to get the structured shape above."
        ],
        "example": {
            "invocation": "pup security findings schema --search cve",
            "response": [{"path": "@advisory.cve", "type": "string", "section": "Advisory", "description": "Primary globally recognized identifier for a security vulnerability"}]
        }
    })
}
