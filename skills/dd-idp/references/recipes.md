# IDP Query Recipes

Use these as starting shapes, not a static schema. Replace every `<placeholder>` with an organization value. Run `pup --read-only idp kinds describe <kind>` before using a recipe and adjust fields or relations to the live response.

## One-step service and dependency context

```bash
pup --read-only idp entities query 'kind:service AND name:"<service-name>"' \
  --field name,display_name,description,owner,team,contacts,service_health_status,active_incidents_count,alert_monitors_count,breached_slos_count \
  --include owner_teams,systems,code_locations,current_oncalls,upstream_services,downstream_services \
  --relation-limit 3 \
  --timeseries-interval 24h \
  --limit 1
```

This is the default service-context workflow: ownership, system and code placement, declared dependencies, and health signals in one request. Relationship `count` can exceed the returned sample, so report truncation instead of presenting the first three dependencies as complete.

Describe `service` before adding a different relation family. Depending on the live schema and question, useful families may include runtime upstream/downstream services, datastores, queues, external providers, inferred services, incidents, monitors, SLOs, deployments, Kubernetes workloads, Terraform, and security findings. Select one relevant family rather than expanding every relation.

## Team portfolio and missing ownership

```bash
pup --read-only idp entities query 'kind:service AND owner:"<team-handle>"' \
  --field name,display_name,owner,team,additional_owners,contacts,links \
  --include owner_teams \
  --include-total-count \
  --limit 25
```

For missing ownership, `_missing_:owner` applies to an attribute, not a relation:

```bash
pup --read-only idp entities query 'kind:service AND _missing_:owner' \
  --field name,display_name,owner \
  --limit 25
```

## Team portfolio health

Describe `service` and select the aggregate fields currently available for incidents, SLOs, monitors, scorecards, vulnerabilities, or health. Keep alternatives beneath the shared kind:

```bash
pup --read-only idp entities query \
  'kind:service AND owner:"<team-handle>" AND (active_incidents_count:>0 OR breached_slos_count:>0 OR critical_vulnerabilities_count:>0 OR high_vulnerabilities_count:>0)' \
  --field name,display_name,owner,active_incidents_count,breached_slos_count,critical_vulnerabilities_count,high_vulnerabilities_count,service_health_status \
  --include systems,code_locations \
  --timeseries-interval 24h \
  --limit 25
```

Report explicit nonzero values, null/unknown fields, the time window, result pagination, and truncated relations separately. Use product-specific commands for incident, SLO, monitor, or vulnerability detail after identifying the relevant entities.

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

## On-call

For service ownership context, include the declared `current_oncalls` relation:

```bash
pup --read-only idp entities query 'kind:service AND name:"<service-name>"' \
  --field name,owner,team \
  --include current_oncalls \
  --relation-limit 25
```

For a provider inventory, describe and query `current_oncall` directly. Treat a truncated relation as a sample, not a full roster.

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

## Public API posture

Scope by a service or team relation, then inspect the explicit returned values:

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

## Cursor continuation

When `page.next_cursor` is present, repeat the exact query, selection, includes, limits, and time window with:

```bash
pup --read-only idp entities query '<same-query>' \
  --field '<same-fields>' \
  --cursor '<next_cursor>'
```

Continue only as far as the user's completeness requirement warrants, and state when results or relationship samples remain truncated.
