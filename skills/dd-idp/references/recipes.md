# IDP Query Recipes

Choose the question closest to the user's task. Replace every `<placeholder>` with an organization value; `<service-ref>` is a full returned ref such as `ref:service:checkout`. Describe unfamiliar kinds with `pup --read-only idp kinds describe <kind>` and use the live fields and relations. Each recipe returns evidence for the stated question; follow the suggested next step only when the user needs it.

## Which service handles this endpoint?

For an on-call engineer starting with a route and HTTP method: resolve the serving service and its owner before looking for logs or paging a team.

```bash
pup --read-only idp entities query \
  'kind:api_endpoint AND http_route:"<route>" AND http_method:"<method>"' \
  --field http_route,http_method,service_name \
  --include service --fields service=name,owner,contacts \
  --relation-limit 5 --limit 10
```

Use the returned service ref with the service-context or on-call recipe below. A route can match several services; preserve those matches and narrow with a known service or host from the live schema. This establishes routing/ownership context, not the cause of an incident.

## Where is this service's code and who owns it?

For an engineer investigating unfamiliar code: return the service's description, contacts, owning team, system membership, and source locations together.

```bash
pup --read-only idp entities query 'ref:"<service-ref>"' \
  --field name,description,owner,contacts,links \
  --include owner_teams,systems,code_locations \
  --fields team=name,handle \
  --fields code_location=repository_id,path_pattern \
  --relation-limit 5 --limit 1
```

Use service `contacts` for contact channels and returned code locations for repository paths. Follow a system or code-location ref when more detail is needed; do not infer a source-code connection from a similar service name. For responders, use [on-call](#who-is-on-call-for-this-service).

## Which callers could be affected by a change?

For an SRE or service owner planning a change: inspect observed callers and the request/error measurements on their connections to this service.

```bash
pup --read-only idp entities query 'ref:"<service-ref>"' \
  --field name,owner \
  --include runtime_upstream_services \
  --edge-fields runtime_upstream_services=requests_count,error_rate \
  --timeseries-interval 1h --relation-limit 5 --limit 1
```

`runtime_upstream_services` are callers of this service; `runtime_downstream_services` are services it calls. Follow returned caller refs for ownership or deeper investigation. Report the window and sampled coverage; this is observed dependency context, not a complete blast radius or proof of causality. See [measurement limitations](footguns.md#time-windows-change-meaning) before interpreting edge values or environment scope.

## Which services does our team own?

For an engineering lead: list primary-owned services with lifecycle, tier, and contact information. This query filters `owner`; selecting `additional_owners` does not include services where the team is only an additional owner.

```bash
pup --read-only idp entities query 'kind:service AND owner:"<team-handle>"' \
  --field name,owner,additional_owners,lifecycle,tier,contacts \
  --order-by name:asc --include-total-count \
  --limit 100 --max-results 500
```

Check `page.stop_reason` before calling this a complete inventory. If the result budget is reached, use the returned `next_request.args`; see [pagination](ueg-dsl.md#pagination-and-completeness).

## Which services are missing a primary owner?

For a platform engineer cleaning up ownership: find services without the primary `owner` attribute. This does not prove that other ownership metadata or contacts are absent.

```bash
pup --read-only idp entities query 'kind:service AND _missing_:owner' \
  --field name,display_name,owner \
  --limit 25
```

For a count of ownership gaps by lifecycle, use the server-side summary instead of downloading every service:

```bash
pup --read-only idp entities aggregate 'kind:service' \
  --group-by lifecycle --count 'unowned=_missing_:owner' \
  --order-by unowned:desc --limit 25
```

Discover observed values without fetching every service:

```bash
pup --read-only idp entities facets 'kind:service' --facet owner,lifecycle
```

See [portfolio summaries](ueg-dsl.md#portfolio-summaries) for result fields and pagination limits.

## Which services need reliability attention?

For an engineering lead reviewing a team's portfolio: find services with active incidents or breached SLOs and return the counts that explain why they matched.

```bash
pup --read-only idp entities query \
  'kind:service AND owner:"<team-handle>" AND (active_incidents_count:>0 OR breached_slos_count:>0)' \
  --field name,owner,active_incidents_count,breached_slos_count \
  --limit 25
```

Use the returned service names with `pup incidents` or `pup slos` for operational detail. Report null/unknown counts separately; a time flag does not turn these health summaries into historical state.

## Which services are below our scorecard target?

For a platform engineer prioritizing standards work: inspect primary-owned services below a chosen scorecard level. This example uses level 2; choose the threshold meaningful to the organization's scorecards.

```bash
pup --read-only idp entities query \
  'kind:service AND owner:"<team-handle>" AND highest_completed_scorecard_level:<2' \
  --field name,owner,highest_completed_scorecard_level \
  --order-by highest_completed_scorecard_level:asc,name:asc --limit 25
```

Keep the name tie-breaker when paging services with equal levels. Inspect the matching services' scorecard rules before deciding what to remediate. Missing levels are unknown, not level zero; the numeric filter does not include them. Use this service field rather than the deprecated `scorecard_outcome` kind.

## Systems and code locations

```bash
pup --read-only idp entities query 'kind:system AND name:"<system-name>"' \
  --field name,display_name,owner \
  --include services \
  --limit 10
```

```bash
pup --read-only idp entities query \
  'kind:code_location AND repository_id:"github.com/<org>/<repository>"' \
  --field name,repository_id,path_pattern,source \
  --include services,systems,repository \
  --limit 25
```

Use returned refs to walk from a service to a system or code location. Do not infer a code-to-service edge from similar names.

## Incidents, monitors, and SLOs

```bash
pup --read-only idp entities query \
  'kind:incident AND service_names:"<service-name>" AND state:active' \
  --field public_id,title,state,severity,service_names,teams \
  --timeseries-interval 24h \
  --limit 25
```

```bash
pup --read-only idp entities query \
  'kind:monitor AND service_tags:"<service-name>" AND status:"Alert"' \
  --field name,status,monitor_id,service_tags \
  --limit 25
```

```bash
pup --read-only idp entities query \
  'kind:slo AND service_names:"<service-name>" AND state:breached' \
  --field name,state,target_threshold,sli \
  --limit 25
```

After graph discovery, use `pup incidents`, `pup monitors`, or `pup slos` for authoritative product detail.

## Who is on call for this service?

For an incident lead: resolve the owning team and current responders together.

```bash
pup --read-only idp owner "<service-name>"
```

This helper uses the service's owner team to look up responders in the on-call API. Check `warnings` if enrichment fails; an absent responder is not proof that nobody is responsible. Prefer this path when the graph's `current_oncalls` relation is unavailable. Use the returned contacts for the escalation the user requested.

## GitHub pull requests

Scope every pull-request search by repository. Numeric lookups can collide across repositories, and broad author/reviewer searches can exceed backend project-fanout limits:

```bash
pup --read-only idp entities query \
  'kind:integration.github.pull_request AND repository.full_name:"<org>/<repository>" AND number:<number>' \
  --field title,state,number,updated_at,mergeable,changed_files,html_url \
  --include repository,author \
  --limit 10
```

Open work by reviewer:

```bash
pup --read-only idp entities query \
  'kind:integration.github.pull_request AND repository.full_name:"<org>/<repository>" AND reviewer_users.login:"<login>" AND state:open AND draft:false' \
  --field title,state,updated_at,mergeable,changed_files,html_url \
  --include repository,author \
  --limit 25
```

Do not assign PRs to services unless a declared relation in the live schema supports that join.

## Jira work

```bash
pup --read-only idp entities query \
  'kind:integration.jira.issue AND key:"<jira-key>"' \
  --field key,summary,status,status_category,priority,assignee_name,html_url \
  --limit 10
```

Prefer direct fields first; relation includes may fail. Do not infer a service link from project names or text.

## Which public endpoints need security attention?

For a security engineer: list a team's public endpoints with authentication, rate-limit, service, and ownership evidence. Inspect the returned protection values to identify the follow-up work.

```bash
pup --read-only idp entities query \
  'kind:api_endpoint AND service.owner:"<team-handle>" AND endpoint_is_public:true' \
  --field resource_name,http_method,http_route,endpoint_is_public,endpoint_authenticated,endpoint_is_rate_limited,service_name,team_names \
  --include service,teams \
  --limit 25
```

Do not trust the query predicate alone: verify `endpoint_is_public` on every returned row. Live schema-declared booleans may arrive as JSON booleans or as strings (`"true"` / `"false"`). Normalize only those explicit forms; treat null or absent authentication/rate-limit values as unknown, and report only explicit false values as missing controls.

## Repository-scoped security and code quality

Discover exact deployed kinds first. Representative shapes include:

```bash
pup --read-only idp entities query \
  'kind:secret AND repository_id:"github.com/<org>/<repository>"' \
  --field severity,repository_id \
  --limit 25
```

```bash
pup --read-only idp entities query \
  'kind:source_code_vulnerability_secfinding AND repository_id:"github.com/<org>/<repository>"' \
  --field severity,finding_type,repository_id,service_name,code_location_filename,package_decl_filename \
  --limit 25
```

```bash
pup --read-only idp entities query \
  'kind:code_violation AND associated_service:*<service-name>*' \
  --field status,k9_severity,associated_service,repository_id \
  --limit 25
```

Repository association is authoritative for the repository, not necessarily for each service in a monorepo. Label free-text service matching as inferred and route to `pup security` or `pup static-analysis` when the user needs product-specific detail.

## Bounded Kubernetes inventory

Never query all deployments unscoped:

```bash
pup --read-only idp entities query \
  'kind:integration.k8s.deployment AND team:"<team-handle>" AND unavailable_replicas:>0' \
  --field name,team,service,cluster_name,namespace,ready_replicas,replicas_desired,unavailable_replicas,status,html_url \
  --limit 25
```

## Change and deployment context

Start from the service so UEG can return declared change/deployment edges alongside ownership and dependencies:

```bash
pup --read-only idp entities query 'kind:service AND name:"<service-name>"' \
  --field name,display_name,owner,service_health_status \
  --include owner_teams,upstream_services,downstream_services,service_deployments \
  --relation-limit 3 \
  --timeseries-interval 24h \
  --limit 1
```

Use the returned refs to inspect a deployment kind directly. If `kinds describe service` does not declare `service_deployments` in the current tenant, omit it and use the deployment or Kubernetes kind named by the live schema. UEG establishes the declared graph context; product deployment and telemetry commands establish detailed rollout state.
