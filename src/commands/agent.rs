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

/// Describes the agent-mode output envelope shared by every command. Mirrors
/// `output::build_agent_envelope_with_order`; keep the two in sync.
pub fn envelope_contract() -> serde_json::Value {
    serde_json::json!({
        "agent_mode": {
            "type": "object",
            "required": ["status", "data", "metadata"],
            "properties": {
                "status": {"const": "success"},
                "data": {"description": "Command payload; see each command's returns.data"},
                "metadata": {
                    "type": "object",
                    "required": ["note"],
                    "properties": {
                        "note": {"type": "string"},
                        "command": {"type": "string"},
                        "count": {"type": "integer"},
                        "truncated": {"type": "boolean", "description": "Omitted when false"},
                        "next_action": {"type": "string"}
                    }
                }
            }
        },
        "hoisting": "When the API body has a top-level `data` key, envelope `data` is that inner value and sibling keys (meta, links, included) are dropped.",
        "no_agent": "With --no-agent the raw API body is printed with no envelope and no hoisting.",
        "jq": "--jq runs on the raw API body (before hoisting); the filtered result becomes envelope `data`. See each command's returns.jq_root.",
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

fn generic_returns() -> serde_json::Value {
    serde_json::json!({
        "documented": false,
        "data": {"description": "Datadog API response body (see envelope.hoisting); shape not published"}
    })
}

fn logs_aggregate_returns() -> serde_json::Value {
    serde_json::json!({
        "documented": true,
        "data": {
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
        },
        "metadata": ["note"],
        "jq_root": ".data.buckets[]",
        "example": {
            "invocation": "pup logs aggregate --query='service:web' --from=1h --compute=count --group-by=status",
            "response": {
                "status": "success",
                "data": {"buckets": [{"by": {"status": "error"}, "computes": {"c0": 42}}]},
                "metadata": {"note": "..."}
            }
        }
    })
}

fn logs_search_returns() -> serde_json::Value {
    serde_json::json!({
        "documented": true,
        "data": {
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
        },
        "metadata": ["note", "command", "count", "truncated", "next_action"],
        "jq_root": ".data[]",
        "example": {
            "invocation": "pup logs search --query='status:error' --from=1h --limit=1",
            "response": {
                "status": "success",
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
                "metadata": {"command": "logs search", "count": 1, "note": "..."}
            }
        }
    })
}

fn traces_aggregate_returns() -> serde_json::Value {
    serde_json::json!({
        "documented": true,
        "data": {
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
        },
        "metadata": ["note", "command"],
        "jq_root": ".data[].attributes",
        "notes": ["Durations (@duration) are in nanoseconds."],
        "example": {
            "invocation": "pup traces aggregate --query='service:web' --from=1h --compute=count --group-by=resource_name",
            "response": {
                "status": "success",
                "data": [{"id": "f8527b82-...", "type": "bucket", "attributes": {"by": {"resource_name": "GET /"}, "compute": {"c0": 2378}}}],
                "metadata": {"command": "traces aggregate", "note": "..."}
            }
        }
    })
}

fn traces_search_returns() -> serde_json::Value {
    serde_json::json!({
        "documented": true,
        "data": {
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
        },
        "metadata": ["note", "command", "count", "truncated", "next_action"],
        "jq_root": ".data[]",
        "notes": ["Field names differ from logs search: start_timestamp (not timestamp), resource_name."]
    })
}

fn metrics_query_returns() -> serde_json::Value {
    serde_json::json!({
        "documented": true,
        "data": {
            "type": "object",
            "properties": {
                "status": {"type": "string", "description": "\"ok\" or \"error\""},
                "error": {"type": "string"},
                "query": {"type": "string"},
                "from_date": {"type": "integer", "description": "seconds since epoch"},
                "to_date": {"type": "integer", "description": "seconds since epoch"},
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
        "metadata": ["note"],
        "jq_root": ".series[]",
        "notes": [
            "Not hoisted: the v1 body has no top-level `data`, so the envelope path is .data.series[] while --jq uses .series[].",
            "An invalid query exits 0 with data.status == \"error\" and the reason in data.error; check data.status."
        ],
        "example": {
            "invocation": "pup metrics query --query='avg:system.cpu.user{env:prod} by {host}' --from=1h",
            "response": {
                "status": "success",
                "data": {"status": "ok", "query": "avg:system.cpu.user{env:prod} by {host}", "series": [{"metric": "system.cpu.user", "scope": "env:prod,host:web-1", "pointlist": [[1767225600000.0, 12.5]]}]},
                "metadata": {"note": "..."}
            }
        }
    })
}

fn findings_schema_returns() -> serde_json::Value {
    serde_json::json!({
        "documented": true,
        "data": {
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
        "metadata": ["note"],
        "jq_root": ".[]",
        "notes": [
            "Without --search or --section this command prints the full reference (~200 KB) as plain markdown on stdout: not JSON and no envelope. Pass a filter to get the structured shape above."
        ],
        "example": {
            "invocation": "pup security findings schema --search cve",
            "response": {
                "status": "success",
                "data": [{"path": "@advisory.cve", "type": "string", "section": "Advisory", "description": "Primary globally recognized identifier for a security vulnerability"}],
                "metadata": {"note": "..."}
            }
        }
    })
}
