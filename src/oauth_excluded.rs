//! Datadog endpoints that do not accept OAuth2 bearer tokens.
//!
//! Auth on these endpoints must use either DD_API_KEY + DD_APP_KEY, or a
//! Personal/Service Access Token in the `DD-APPLICATION-KEY` slot (Datadog's
//! PAT migration form). The list is consulted by `client::apply_auth`,
//! `commands::api::run`, and the WASM `api::apply_auth` so the routing is
//! consistent across paths.
//!
//! This module is intentionally dependency-free so the browser/WASM build can
//! import it alongside `src/api.rs` without dragging in the rest of
//! `src/client.rs`.

pub struct EndpointRequirement {
    pub path: &'static str,
    pub method: &'static str,
}

/// Returns true if the endpoint doesn't support OAuth and requires API key fallback.
pub fn requires_api_key_fallback(method: &str, path: &str) -> bool {
    find_endpoint_requirement(method, path).is_some()
}

fn find_endpoint_requirement(method: &str, path: &str) -> Option<&'static EndpointRequirement> {
    OAUTH_EXCLUDED_ENDPOINTS.iter().find(|req| {
        if req.method != method {
            return false;
        }
        // Trailing "/" means prefix match (for ID-parameterized paths)
        if req.path.ends_with('/') {
            path.starts_with(&req.path[..req.path.len() - 1])
        } else {
            req.path == path
        }
    })
}

/// Endpoints that don't support OAuth.
/// Trailing "/" means prefix match for ID-parameterized paths.
pub static OAUTH_EXCLUDED_ENDPOINTS: &[EndpointRequirement] = &[
    // API/App Keys (8)
    EndpointRequirement {
        path: "/api/v2/api_keys",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/v2/api_keys/",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/v2/api_keys",
        method: "POST",
    },
    EndpointRequirement {
        path: "/api/v2/api_keys/",
        method: "DELETE",
    },
    EndpointRequirement {
        path: "/api/v2/application_keys",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/v2/application_keys/",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/v2/application_keys/",
        method: "POST",
    },
    EndpointRequirement {
        path: "/api/v2/application_keys/",
        method: "PATCH",
    },
    // DDSQL editor tools (3)
    EndpointRequirement {
        path: "/api/unstable/ddsql-editor/tools/ddsql-docs",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/unstable/ddsql-editor/tools/table-names",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/unstable/ddsql-editor/tools/table-data",
        method: "POST",
    },
    EndpointRequirement {
        path: "/api/v2/application_keys/",
        method: "DELETE",
    },
    // Fleet Automation (15)
    EndpointRequirement {
        path: "/api/v2/fleet/agents",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/v2/fleet/agents/",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/v2/fleet/agents/versions",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/v2/fleet/deployments",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/v2/fleet/deployments/",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/v2/fleet/deployments/configure",
        method: "POST",
    },
    EndpointRequirement {
        path: "/api/v2/fleet/deployments/upgrade",
        method: "POST",
    },
    EndpointRequirement {
        path: "/api/v2/fleet/deployments/",
        method: "POST",
    },
    EndpointRequirement {
        path: "/api/v2/fleet/deployments/",
        method: "DELETE",
    },
    EndpointRequirement {
        path: "/api/v2/fleet/schedules",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/v2/fleet/schedules/",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/v2/fleet/schedules",
        method: "POST",
    },
    EndpointRequirement {
        path: "/api/v2/fleet/schedules/",
        method: "PATCH",
    },
    EndpointRequirement {
        path: "/api/v2/fleet/schedules/",
        method: "DELETE",
    },
    EndpointRequirement {
        path: "/api/v2/fleet/schedules/",
        method: "POST",
    },
    // Observability Pipelines (6) — API key only, no OAuth support
    EndpointRequirement {
        path: "/api/v2/obs-pipelines/pipelines",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/v2/obs-pipelines/pipelines",
        method: "POST",
    },
    EndpointRequirement {
        path: "/api/v2/obs-pipelines/pipelines/",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/v2/obs-pipelines/pipelines/",
        method: "PUT",
    },
    EndpointRequirement {
        path: "/api/v2/obs-pipelines/pipelines/",
        method: "DELETE",
    },
    EndpointRequirement {
        path: "/api/v2/obs-pipelines/pipelines/validate",
        method: "POST",
    },
    // Cost / Billing (3) — API key only, no OAuth support
    EndpointRequirement {
        path: "/api/v2/usage/projected_cost",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/v2/usage/cost_by_org",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/v2/cost_by_tag/monthly_cost_attribution",
        method: "GET",
    },
    // Cloud Cost Management config (12)
    EndpointRequirement {
        path: "/api/v2/cost/aws_cur_config",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/v2/cost/aws_cur_config",
        method: "POST",
    },
    EndpointRequirement {
        path: "/api/v2/cost/aws_cur_config/",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/v2/cost/aws_cur_config/",
        method: "DELETE",
    },
    EndpointRequirement {
        path: "/api/v2/cost/azure_uc_config",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/v2/cost/azure_uc_config",
        method: "POST",
    },
    EndpointRequirement {
        path: "/api/v2/cost/azure_uc_config/",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/v2/cost/azure_uc_config/",
        method: "DELETE",
    },
    EndpointRequirement {
        path: "/api/v2/cost/gcp_uc_config",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/v2/cost/gcp_uc_config",
        method: "POST",
    },
    EndpointRequirement {
        path: "/api/v2/cost/gcp_uc_config/",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/v2/cost/gcp_uc_config/",
        method: "DELETE",
    },
    // Profiling (4)
    // No OAuth scope is declared for Continuous Profiler endpoints; force API-key auth.
    EndpointRequirement {
        path: "/profiling/api/v1/",
        method: "POST",
    },
    EndpointRequirement {
        path: "/profiling/api/v1/",
        method: "GET",
    },
    EndpointRequirement {
        path: "/api/unstable/profiles/",
        method: "POST",
    },
    EndpointRequirement {
        path: "/api/ui/profiling/",
        method: "GET",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_excluded_endpoints_count() {
        // Bumping this requires conscious thought -- it's the canonical list.
        assert_eq!(OAUTH_EXCLUDED_ENDPOINTS.len(), 52);
    }

    #[test]
    fn test_requires_fallback_api_keys_exact_match() {
        assert!(requires_api_key_fallback("GET", "/api/v2/api_keys"));
        assert!(!requires_api_key_fallback("PUT", "/api/v2/api_keys"));
    }

    #[test]
    fn test_requires_fallback_prefix_match() {
        assert!(requires_api_key_fallback(
            "DELETE",
            "/api/v2/api_keys/abc-123"
        ));
        assert!(requires_api_key_fallback(
            "GET",
            "/api/v2/fleet/agents/some-agent"
        ));
    }

    #[test]
    fn test_requires_fallback_unrelated_path() {
        assert!(!requires_api_key_fallback("GET", "/api/v1/monitors"));
        assert!(!requires_api_key_fallback(
            "POST",
            "/api/v2/logs/events/search"
        ));
    }
}
