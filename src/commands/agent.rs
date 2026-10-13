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

/// Describes the default JSON envelope shared by every command. Mirrors
/// `output::build_envelope`; keep the two in sync.
pub fn envelope_contract() -> serde_json::Value {
    serde_json::json!({
        "default": {
            "type": "object",
            "required": ["status", "data", "metadata"],
            "properties": {
                "status": {"const": "success"},
                "data": {"description": "Command payload; see each command's returns.data"},
                "metadata": {
                    "type": "object",
                    "required": ["truncated"],
                    "properties": {
                        "count": {"type": "integer", "description": "Items in data when it is a list"},
                        "truncated": {"type": "boolean", "description": "More results exist; false means none known"},
                        "command": {"type": "string"},
                        "next_cursor": {"type": "string", "description": "Pass back with the cursor flag named in next_action"},
                        "next_offset": {"type": "integer", "description": "Pass back with --offset"},
                        "next_page": {"type": "integer", "description": "Pass back with the page flag named in next_action"},
                        "next_action": {"type": "string"},
                        "warnings": {"type": "array", "items": {"type": "string"}}
                    }
                }
            }
        },
        "siblings": "A body with top-level `data` keeps it and its siblings (meta, links, included), plus `status` and `metadata`. Any other body, or one with its own `status`/`metadata` key (e.g. monitors search pagination), becomes envelope `data` whole.",
        "cursors": "metadata.next_cursor / next_offset / next_page give the next page when pup recognizes the paging fields, even after --jq. Absent means none known.",
        "raw": "--no-envelope (PUP_NO_ENVELOPE=1, legacy --no-agent) prints the raw body.",
        "jq": "--jq runs on the raw API body; its result becomes envelope `data` and metadata is kept. See each command's returns.jq_root.",
        "formats": "JSON and YAML are enveloped. Table/CSV/TSV print only rows on stdout and the metadata on stderr: a `metadata: key=value ...` line, then `next_action:` and `warning:` lines when set (only `warning:` with --no-envelope).",
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
        "setup" => setup_returns(),
        _ => generic_returns(),
    }
}

fn setup_returns() -> serde_json::Value {
    serde_json::json!({
        "documented": false,
        "data": {"description": "No envelope and no API body: AI Setup's output is passed through to stdout and stderr."},
        "notes": [
            "Headless runs (an AI agent is detected, or there is no terminal) print AI_SETUP_PROGRESS lines and end with one AI_SETUP_RESULT line: `status=success product=<product> code=0 url=<next step>` or `status=error product=<product> code=<n> reason=\"<message>\"`.",
            "The exit code is AI Setup's: 0 on success, non-zero on failure.",
            "Needs Node.js 22+ and npx; without them pup prints how to install them and exits 1."
        ]
    })
}

fn generic_returns() -> serde_json::Value {
    serde_json::json!({
        "documented": false,
        "data": {"description": "Datadog API response payload inside the envelope; shape not published"}
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
        "jq_root": ".data.buckets[]",
        "example": {
            "invocation": "pup logs aggregate --query='service:web' --from=1h --compute=count --group-by=status",
            "response": {
                "status": "success",
                "data": {"buckets": [{"by": {"status": "error"}, "computes": {"c0": 42}}]},
                "meta": {"status": "done"},
                "metadata": {"command": "logs aggregate", "truncated": false}
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
        "jq_root": ".data[]",
        "notes": ["When more results exist, metadata.next_cursor (also the response's meta.page.after) holds the cursor; pass it back with --cursor."],
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
                "meta": {"page": {"after": "eyJ..."}},
                "metadata": {"command": "logs search", "count": 1, "truncated": true, "next_cursor": "eyJ...", "next_action": "More results available. Use --cursor=\"eyJ...\" to fetch the next page in the current --sort order, change the limit with --limit, or narrow the --query"}
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
        "jq_root": ".data[].attributes",
        "notes": ["Durations (@duration) are in nanoseconds."],
        "example": {
            "invocation": "pup traces aggregate --query='service:web' --from=1h --compute=count --group-by=resource_name",
            "response": {
                "status": "success",
                "data": [{"id": "f8527b82-...", "type": "bucket", "attributes": {"by": {"resource_name": "GET /"}, "compute": {"c0": 2378}}}],
                "meta": {"status": "done"},
                "metadata": {"command": "traces aggregate", "count": 1, "truncated": false}
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
        "jq_root": ".data[]",
        "notes": [
            "Field names differ from logs search: start_timestamp (not timestamp), resource_name.",
            "When more results exist, metadata.next_cursor (also the response's meta.page.after) holds the cursor; pass it back with --cursor."
        ]
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
            "The v1 body has no top-level `data`, so it becomes envelope data: the envelope path is .data.series[] while --jq uses .series[].",
            "An invalid query exits 0 with data.status == \"error\" and the reason in data.error; check data.status."
        ],
        "example": {
            "invocation": "pup metrics query --query='avg:system.cpu.user{env:prod} by {host}' --from=1h",
            "response": {
                "status": "success",
                "data": {"status": "ok", "query": "avg:system.cpu.user{env:prod} by {host}", "series": [{"metric": "system.cpu.user", "scope": "env:prod,host:web-1", "pointlist": [[1767225600000.0, 12.5]]}]},
                "metadata": {"command": "metrics query", "truncated": false}
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
        "jq_root": ".[]",
        "notes": [
            "Without --search or --section this command prints the full reference (~200 KB) as plain markdown on stdout, not JSON. Pass a filter to get the structured shape above."
        ],
        "example": {
            "invocation": "pup security findings schema --search cve",
            "response": {
                "status": "success",
                "data": [{"path": "@advisory.cve", "type": "string", "section": "Advisory", "description": "Primary globally recognized identifier for a security vulnerability"}],
                "metadata": {"command": "security findings schema", "count": 1, "truncated": false}
            }
        }
    })
}
