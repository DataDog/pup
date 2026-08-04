# Usage Examples

Common workflows and usage patterns for Pup CLI.

## Authentication

### OAuth2 Login (Recommended)
```bash
# Login with default site (datadoghq.com)
pup auth login

# Login with specific site
pup --site=datadoghq.eu auth login

# Check authentication status
pup auth status

# Logout
pup auth logout
```

### API Key Authentication (Legacy)
```bash
export DD_API_KEY="your-api-key"
export DD_APP_KEY="your-app-key"
export DD_SITE="datadoghq.com"
```

## Metrics

### List Metrics
```bash
# List all metrics
pup metrics list

# Filter by pattern
pup metrics list --filter="system.*"
pup metrics list --filter="custom.app.*"
```

### Search Metrics (v1 API)
```bash
# Classic query syntax
pup metrics search --query="avg:system.cpu.user{*}" --from="1h"

# Search with aggregation and grouping
pup metrics search --query="sum:app.requests{env:prod} by {service}" --from="4h"
```

### Query Metrics (v2 API)
```bash
# Timeseries formula query
pup metrics query --query="avg:system.cpu.user{*}" --from="1h" --to="now"

# Query with aggregation
pup metrics query --query="sum:app.requests{env:prod} by {service}" --from="4h"
```

## Monitors

### List Monitors
```bash
# List all monitors
pup monitors list

# Filter by tag
pup monitors list --tag="env:production"
pup monitors list --tag="team:backend"

# Multiple tags
pup monitors list --tag="env:prod" --tag="service:api"
```

### Get Monitor Details
```bash
# Get specific monitor by ID
pup monitors get 12345678
```

### Delete Monitor
```bash
# Delete monitor (prompts for confirmation)
pup monitors delete 12345678

# Skip confirmation
pup monitors delete 12345678 --yes
```

### Diff Monitor (preview changes before update)
```bash
# Compare a candidate JSON file against the live monitor
pup monitors diff 12345678 candidate.json

# Scope the diff to a specific field subtree
pup monitors diff 12345678 candidate.json --only options.thresholds

# Exclude noisy fields from the diff
pup monitors diff 12345678 candidate.json --ignore message

# Combine --only and --ignore; both accept comma-separated or repeated flags
pup monitors diff 12345678 candidate.json --only options.thresholds --ignore options.thresholds.warning
```

> **"removed" entries:** `pup monitors update` is a partial/merge update — fields absent
> from the candidate are left unchanged on the live monitor, not deleted. `"removed"` entries
> in the diff show fields the candidate does not specify; they will **not** be deleted by
> `update`. Use `--ignore` to hide specific live-only fields from the output.
> Example: `pup monitors diff 12345678 candidate.json --ignore options`

## Logs

### Search Logs
```bash
# Search for errors in last hour
pup logs search --query="status:error" --from="1h" --to="now"

# Search by service
pup logs search --query="service:web-app status:warn" --from="30m"

# Complex query with attributes
pup logs search --query="@user.id:12345 status:error" --limit=100

# Search with time range
pup logs search \
  --query="service:api" \
  --from="2024-02-04T10:00:00Z" \
  --to="2024-02-04T11:00:00Z"

# Search specific indexes (comma-separated or repeated)
pup logs query --query="service:api" --index="main,security" --from="1h"
pup logs query --query="service:api" --index="main" --index="security" --from="1h"
```

### Aggregate Logs
```bash
# Count logs by status
pup logs aggregate \
  --query="service:web-app" \
  --from="1h" \
  --compute="count" \
  --group-by="status"

# Average duration by service
pup logs aggregate \
  --query="service:web-app" \
  --from="1h" \
  --compute="avg(@duration)" \
  --group-by="service"

# 99th percentile latency by service
pup logs aggregate \
  --query="env:prod" \
  --from="30m" \
  --compute="percentile(@duration, 99)" \
  --group-by="service"

# Multiple metrics in one query (comma-separated)
pup logs aggregate \
  --query="service:web-app" \
  --from="1h" \
  --compute="count,avg(@duration),percentile(@duration, 95)" \
  --group-by="service,status"

# Aggregate a specific index
pup logs aggregate \
  --query="service:web-app" \
  --index="main" \
  --from="1h" \
  --compute="count" \
  --group-by="status"
```

### Search Logs in Specific Storage Tier
```bash
# Search Flex logs (cost-optimized storage tier)
pup logs search --query="service:api" --from="7d" --storage="flex"

# Search online archives (long-term storage)
pup logs search --query="status:error" --from="30d" --storage="online-archives"

# Search standard indexes (default, fastest tier)
pup logs search --query="service:web-app" --from="1h" --storage="indexes"

# Use Datadog's default storage behavior
pup logs search --query="status:warn" --from="1h"
```

## Dashboards

### List Dashboards
```bash
# List all dashboards
pup dashboards list

# Output as table
pup dashboards list --output=table
```

### Get Dashboard
```bash
# Get dashboard details
pup dashboards get "abc-123-def"

# Get public URL for sharing
pup dashboards url "abc-123-def"

# Open with a live 1 week time window
pup dashboards url "abc-123-def" --from=now-1w --to=now --live=true
```

### Delete Dashboard
```bash
pup dashboards delete "abc-123-def" --yes
```

## Database Monitoring

### Search DBM Query Samples
```bash
# Search recent DBM activity samples
pup dbm samples search --query="dbm_type:activity service:orders env:prod" --from="1h" --limit=10

# Search a specific database host
pup dbm samples search --query="db.hostname:primary-db service:checkout" --from="30m" --sort="asc"

# Search an explicit time window
pup dbm samples search \
  --query="service:payments @db.statement_type:select" \
  --from="2024-02-04T10:00:00Z" \
  --to="2024-02-04T11:00:00Z" \
  --limit=25
```

## Change Stories

### List Change Stories for a Service
```bash
# All change events in the last hour (relative time)
pup change-stories list --service api-gateway --from 1h --to now

# All change events using an absolute window with unix timestamps
pup change-stories list --service api-gateway --from 1713132000 --to 1713135600

# Narrow to deployments in production using an absolute window
pup change-stories list \
  --service api-gateway --env prod \
  --from 2024-01-15T00:00:00Z --to 2024-01-15T01:00:00Z \
  --story-types deployment

# Multiple story types
pup change-stories list \
  --service api-gateway \
  --from 2024-01-15T00:00:00Z --to 2024-01-15T01:00:00Z \
  --story-types deployment --story-types kubernetes

# Region-scoped filter (--filter-tags is key:value)
pup change-stories list \
  --service api-gateway \
  --from 2024-01-15T00:00:00Z --to 2024-01-15T01:00:00Z \
  --filter-tags datacenter:us1.prod.dog

# Trim response to ~4000 tokens (server default is 10000)
pup change-stories list \
  --service api-gateway --from 30min --to now \
  --token-limit 4000
```

## SLOs

### List SLOs
```bash
# List all SLOs
pup slos list

# Filter by name or API-supported query string
pup slos list --query="monitor-history-reader"

# Filter by a single SLO tag
pup slos list --tags-query="team:slo-app"

# Filter by metric query and paginate results
pup slos list --metrics-query="sum:requests.error{service:api}" --limit=25 --offset=50
```

### Get SLO
```bash
pup slos get "abc-123-def"
```

### Create SLO
```bash
pup slos create \
  --name="API Availability" \
  --type="metric" \
  --target=99.9 \
  --timeframe="30d"
```

### Manage SLO Corrections
```bash
# List corrections for SLO
pup slos corrections list "abc-123-def"

# Create correction
pup slos corrections create "abc-123-def" \
  --start="2024-02-04T10:00:00Z" \
  --end="2024-02-04T11:00:00Z" \
  --category="deployment"
```

## Incidents

### List Incidents
```bash
# List all incidents
pup incidents list

# Filter by status
pup incidents list --status="active"
```

### Get Incident
```bash
pup incidents get "abc-123"
```

### Create Incident
```bash
pup incidents create \
  --title="High Error Rate in API" \
  --severity="SEV-2" \
  --customer-impacted=true
```

### Update Incident
```bash
pup incidents update "abc-123" --status="resolved"
```

## RUM (Real User Monitoring)

### List RUM Applications
```bash
pup rum apps list
```

### Get RUM Application
```bash
pup rum apps get "abc-123"
```

### Search RUM Sessions
```bash
pup rum sessions search \
  --query="@application.id:abc-123" \
  --from="1h"
```

## Security

### List Security Rules
```bash
pup security rules list
```

### Get Security Rule
```bash
pup security rules get "abc-123"
```

### List Security Signals
```bash
pup security signals list --from="1h"
```

### Search Security Findings
```bash
pup security findings search \
  --query="@severity:high"
```

## APM Service Config

### Get Service Instance Configuration
```bash
# Get service instance metadata (IDs, hostnames, config IDs)
pup apm service-config get --service-name my-service

# Filter by environment
pup apm service-config get --service-name my-service --env prod

# Filter by specific instance IDs
pup apm service-config get --service-name my-service --service-instance-ids "id-1,id-2"
```

## APM Service Library Config

### Get Service Library Configuration
```bash
# Get tracer configuration for a service across all instances
pup apm service-library-config get --service-name my-service

# Filter by environment
pup apm service-library-config get --service-name my-service --env prod

# Filter by language
pup apm service-library-config get --service-name my-service --env prod --language python

# Only show configs where instances disagree (useful for finding config drift)
pup apm service-library-config get --service-name my-service --mixed
```

## APM Troubleshooting

### List Instrumentation Errors for a Host
```bash
# Show APM instrumentation errors for a specific host
pup apm troubleshooting list --hostname my-host

# Narrow results to a specific time window
pup apm troubleshooting list --hostname my-host --timeframe 4h
```

## Fleet Tracers

### List Tracers Across the Fleet
```bash
# List all tracers (telemetry-derived service names, language, runtime IDs)
pup fleet tracers list

# Filter tracers by environment
pup fleet tracers list --filter "env:prod"

# Filter tracers by hostname
pup fleet tracers list --filter "hostname:my-host"

# Paginate results
pup fleet tracers list --filter "env:prod" --page-size 50 --page-number 0
```

### List Tracers for a Specific Agent
```bash
# Get tracers running on a specific agent
pup fleet agents tracers <agent-key>

# With pagination
pup fleet agents tracers <agent-key> --page-size 20
```

### List Instrumented Pods (K8s)
```bash
# List pods instrumented by SSI in a cluster
pup fleet instrumented-pods list <cluster-name>
```

## Fleet Clusters

### List Kubernetes Clusters in the Fleet
```bash
# List all clusters
pup fleet clusters list

# Filter by cluster name
pup fleet clusters list --filter "cluster_name:production"

# Paginate
pup fleet clusters list --filter "env:prod" --page-size 50 --page-number 0
```

## Live Debugger

### Service Context
```bash
# Get full service context as JSON (environments, instances, probe support)
pup debugger context my-service

# Filter to a specific environment
pup debugger context my-service --env staging

# Select specific fields for compact output: service, language, envs, repo
pup debugger context my-service --fields service,language,envs
```

### List Log Probes
```bash
# List all log probes
pup debugger probes list

# Filter by service
pup debugger probes list --service my-service
```

### Get Probe Details
```bash
pup debugger probes get "probe-id"
```

### Create a Log Probe
```bash
# Create a probe with capture expressions (recommended)
pup debugger probes create \
  --service my-service \
  --env staging \
  --probe-location com.example.MyClass:myMethod \
  --capture "request.id" --capture "user.name"

# Use a method signature from --view probe-locations output
pup debugger probes create \
  --service my-service \
  --env staging \
  --probe-location "com.example.MyClass:myMethod(String, int)" \
  --capture "request.id"

# Increase capture depth for nested objects (default: 1)
pup debugger probes create \
  --service my-service \
  --env staging \
  --probe-location com.example.MyClass:myMethod \
  --capture "response.body" --depth 3

# Create with full snapshot capture
pup debugger probes create \
  --service my-service \
  --env staging \
  --probe-location com.example.MyClass:myMethod \
  --capture

# Create with a custom template
pup debugger probes create \
  --service my-service \
  --env staging \
  --probe-location com.example.MyClass:myMethod \
  --capture "userId" \
  --template "User {userId} called, took {@duration}ms"

# Create with a condition
pup debugger probes create \
  --service my-service \
  --env staging \
  --probe-location com.example.MyClass:myMethod \
  --capture "userId" \
  --condition "userId != null"

# Create with custom budget and TTL
pup debugger probes create \
  --service my-service \
  --env staging \
  --probe-location com.example.MyClass:myMethod \
  --budget 500 --ttl 2h
```

### Delete a Log Probe
```bash
pup debugger probes delete "probe-id"
```

### Watch Probe Events
```bash
# Stream probe events — compact output (message, captures, timestamp)
pup debugger probes watch "probe-id" --fields "message,captures,timestamp" --wait 10

# Template message only (one line per event)
pup debugger probes watch "probe-id" --fields "message" --limit 5

# Full debugger payload (default — trimmed to just the debugger field)
pup debugger probes watch "probe-id"

# Limit to 5 events
pup debugger probes watch "probe-id" --limit 5

# Custom timeout
pup debugger probes watch "probe-id" --timeout 300

# Start from a specific time
pup debugger probes watch "probe-id" --from 1h

# Wait for a newly created probe to become available (useful in pipelines)
pup debugger probes watch "probe-id" --wait 30
```

### Pipeline: Create and Watch
```bash
# Search for a method, create a probe, and watch events
# Note: --view probe-locations may output signatures like TYPE:METHOD(args)
pup symdb search --service my-service --query MyController --view probe-locations \
  | head -1 \
  | xargs -I{} pup debugger probes create --service my-service --env staging --probe-location {} --capture --ttl 1h \
  | jq -r .data.id \
  | xargs -I{} pup debugger probes watch {} --fields "message,captures,timestamp" --wait 30 --limit 5
```

## SymDB (Symbol Database)

### Search Scopes
```bash
# Search for classes/methods by name
pup symdb search --service my-service --query MyController

# List all scopes in a service
pup symdb search --service my-service

# Filter by service version
pup symdb search --service my-service --query MyController --version 1.2.3
```

### Output Views
```bash
# Full JSON response (default)
pup symdb search --service my-service --query MyController --view full

# Scope names only
pup symdb search --service my-service --query MyController --view names

# Probe locations (type:method or type:method(args) format)
pup symdb search --service my-service --query MyController --view probe-locations
```

## Containers

### List Containers
```bash
# List all containers
pup containers list

# Filter by tags
pup containers list --filter-tags="env:production"

# Group by image
pup containers list --group-by="image_name"
```

### List Container Images
```bash
# List all container images
pup containers images list

# Filter images by tags
pup containers images list --filter-tags="env:production"
```

## Infrastructure

### List Hosts
```bash
# List all hosts
pup infrastructure hosts list

# Filter by tag
pup infrastructure hosts list --filter="env:production"
```

## Fleet

### List Fleet Agents
```bash
# Filter agents by hostname
pup fleet agents list --filter "hostname:my-host"

# Filter by IP address
pup fleet agents list --filter "ip_address:1.2.3.4"

# Boolean filter expression
pup fleet agents list --filter "(hostname:host-a OR hostname:host-b) AND env:prod"
```

### Get Host
```bash
pup infrastructure hosts get "host-name"
```

## Tags

### List Host Tags
```bash
# List all host tags
pup tags list
```

### Get Tags for Host
```bash
pup tags get "host-name"
```

### Add Tags to Host
```bash
pup tags add "host-name" \
  --tag="env:production" \
  --tag="team:backend"
```

### Update Host Tags
```bash
pup tags update "host-name" \
  --tag="env:prod" \
  --tag="service:api"
```

## Users & Organizations

### List Users
```bash
pup users list
```

### Get User
```bash
pup users get "user-id"
```

### List Roles
```bash
pup users roles list
```

### Get Organization
```bash
pup organizations get
```

## API Keys

### List API Keys
```bash
pup api-keys list
```

### Get API Key
```bash
pup api-keys get "key-id"
```

### Create API Key
```bash
pup api-keys create --name="CI/CD Key"
```

### Delete API Key
```bash
pup api-keys delete "key-id" --yes
```

## Synthetics

### List Synthetic Tests
```bash
pup synthetics tests list
```

### Get Synthetic Test
```bash
pup synthetics tests get "test-id"
```

### List Synthetic Locations
```bash
pup synthetics locations list
```

## Workflows

### Get a Workflow
```bash
pup workflows get <workflow-id>
```

### Create a Workflow
```bash
pup workflows create --file=workflow.json
```

### Update a Workflow
```bash
pup workflows update <workflow-id> --file=workflow.json
```

### Diff a Workflow
```bash
# Compare a candidate JSON file against the live workflow
pup workflows diff <workflow-id> workflow.json

# Scope or suppress specific field paths
pup workflows diff <workflow-id> workflow.json --only data.attributes.spec
pup workflows diff <workflow-id> workflow.json --ignore data.attributes.updatedAt
```

### Delete a Workflow
```bash
pup workflows delete <workflow-id>
```

### Execute a Workflow
```bash
# Run with inline payload (requires DD_API_KEY + DD_APP_KEY)
pup workflows run <workflow-id> --payload '{"key": "value"}'

# Run with payload from file
pup workflows run <workflow-id> --payload-file=params.json

# Run and wait for completion (default timeout: 5m)
pup workflows run <workflow-id> --wait

# Run with custom timeout
pup workflows run <workflow-id> --wait --timeout 2m
```

### Manage Workflow Instances
```bash
# List recent executions
pup workflows instances list <workflow-id>

# List with pagination
pup workflows instances list <workflow-id> --limit=20 --page=2

# Get instance details
pup workflows instances get <workflow-id> <instance-id>

# Cancel a running instance
pup workflows instances cancel <workflow-id> <instance-id>
```

## Runbooks

Runbooks are YAML files stored in `~/.config/pup/runbooks/` that encode multi-step operational procedures. Each step runs a `pup` command, shell command, HTTP request, Datadog Workflow, or an interactive confirmation.

### List Available Runbooks
```bash
pup runbooks list
pup runbooks list --tag=type:deployment
```

### Inspect a Runbook
```bash
pup runbooks describe deploy-service
```

### Run a Runbook
```bash
# Run with required variables via --arg flags
pup runbooks run deploy-service --arg SERVICE=payments --arg VERSION=1.2.3

# Dry-run (print steps without executing)
pup runbooks run incident-triage --dry-run
```

### Import a Runbook
```bash
# Copy a runbook YAML into ~/.config/pup/runbooks/
pup runbooks import ./my-runbook.yaml
```

### Validate a Runbook
```bash
pup runbooks validate ./my-runbook.yaml
```

### Example Runbook (YAML)
```yaml
name: restart-service
description: Safely restart a service after checking monitors
vars:
  SERVICE:
    description: Service name
    required: true

steps:
  - name: Check active monitors
    kind: pup
    run: monitors list --tags="service:{{SERVICE}}"
    capture: MONITORS_JSON

  - name: Confirm restart
    kind: confirm
    message: "Restart {{SERVICE}}? Review monitors above."

  - name: Trigger restart workflow
    kind: datadog-workflow
    workflow_id: "abc-123"
    inputs:
      service: "{{SERVICE}}"
    on_failure: fail

  - name: Notify via webhook
    kind: http
    url: https://hooks.example.com/notify
    method: POST
    body: '{"text": "Restarted {{SERVICE}}"}'
    content_type: application/json
    on_failure: warn
```

### Reusable Step Templates
Store shared step logic in `~/.config/pup/runbooks/_templates/<name>.yaml`:
```yaml
# _templates/slack-notify.yaml
kind: http
url: "{{SLACK_WEBHOOK}}"
method: POST
body: '{"text": "{{MESSAGE}}"}'
content_type: application/json
on_failure: warn
```

Reference a template in any runbook step:
```yaml
steps:
  - name: Notify Slack
    template: slack-notify
    # Override any template field as needed
```

See `docs/examples/runbooks/` for complete examples.

## IDP (Service Catalog)

### Get Full Service Context
```bash
# Get owner, on-call, health, dependencies, and metadata gaps in one call
pup idp assist my-service

# Useful as a starting point for incident response or code review
pup idp assist payments-api
```

### Find Entities
```bash
# Search services by name (fuzzy match)
pup idp find payments

# Use kind: prefix to search other entity types
pup idp find "kind:team AND name:backend"
```

### Get Ownership and On-Call
```bash
# Show owning team and current on-call responders
pup idp owner my-service
```

### Show Service Dependencies
```bash
# List upstream (callers) and downstream (callees) services
pup idp deps my-service
```

### Register a Service Definition
```bash
# POST a service.datadog.yaml to the Service Definitions API
pup idp register service.datadog.yaml

# Verify after registration
pup idp assist my-service
```

### Incident Response Workflow with IDP
```bash
# Get full service context immediately
pup idp assist payments-api

# Investigate alerts for the service
pup monitors list --tag="service:payments-api"

# Check who is on-call
pup idp owner payments-api

# Review upstream services that may be affected
pup idp deps payments-api
```

## Costs

### Datadog Cost (Usage Metering)
```bash
# Projected cost for current month
pup costs datadog projected

# Cost breakdown by org for a date range
pup costs datadog by-org --start-month="2024-01-01T00:00:00Z" --end-month="2024-03-01T00:00:00Z"

# Monthly cost attribution
pup costs datadog attribution --start="2024-01-01T00:00:00Z"

# AWS CUR config management
pup costs datadog aws-config list
pup costs datadog aws-config get 42
pup costs datadog aws-config create --file=aws-cur.json
pup costs datadog aws-config delete 42

# Azure UC config management
pup costs datadog azure-config list

# GCP usage cost config management
pup costs datadog gcp-config list
```

### Cloud Cost Management (CCM)

#### Custom Costs
```bash
# List uploaded custom cost files
pup costs ccm custom-costs list

# Filter by status or sort
pup costs ccm custom-costs list --status=SUCCESS --sort=created_at

# Get a specific custom cost file
pup costs ccm custom-costs get <file-id>

# Upload a custom cost CSV (multipart PUT)
pup costs ccm custom-costs upload --file=costs.csv

# Upload with a schema version
pup costs ccm custom-costs upload --file=costs.csv --version=2

# Delete a custom cost file
pup costs ccm custom-costs delete <file-id>
```

#### Tag Descriptions
```bash
# List all tag descriptions
pup costs ccm tag-descriptions list

# Filter by cloud provider
pup costs ccm tag-descriptions list --cloud=aws

# Get description for a specific tag key
pup costs ccm tag-descriptions get --tag-key=env

# Generate an AI-suggested description for a tag
pup costs ccm tag-descriptions generate --tag-key=team

# Create or update a tag description
pup costs ccm tag-descriptions upsert --tag-key=env --description="Deployment environment"

# Delete a tag description
pup costs ccm tag-descriptions delete --tag-key=env
```

#### Tag Metadata
```bash
# List all tag metadata for a month
pup costs ccm tag-metadata list --month=2024-01

# Filter by provider and metric
pup costs ccm tag-metadata list --month=2024-01 --provider=aws --metric=aws.cost

# List tag sources
pup costs ccm tag-metadata tag-sources --month=2024-01

# List available metrics
pup costs ccm tag-metadata metrics --month=2024-01

# List orchestrators
pup costs ccm tag-metadata orchestrators --month=2024-01

# List supported currencies
pup costs ccm tag-metadata currency --month=2024-01
```

#### Tags and Tag Keys
```bash
# List all CCM tags
pup costs ccm tags list

# Filter by metric
pup costs ccm tags list --metric=aws.cost

# List tag keys
pup costs ccm tag-keys list

# Get details for a specific tag key
pup costs ccm tag-keys get env
```

#### Budgets
```bash
# List all budgets
pup costs ccm budgets list

# Get a specific budget
pup costs ccm budgets get <budget-id>

# Get with actual and forecasted spend for a time range
pup costs ccm budgets get <budget-id> \
  --start="2024-01-01T00:00:00Z" \
  --end="2024-03-01T00:00:00Z" \
  --actual --forecast

# Create or update a budget (upsert)
pup costs ccm budgets upsert --file=budget.json

# Delete a budget
pup costs ccm budgets delete <budget-id>

# Validate a budget payload without saving
pup costs ccm budgets validate --file=budget.json
```

#### Commitment Programs (Reserved Instances & Savings Plans)
```bash
# Utilization of RIs over a time range
pup costs ccm commitments utilization \
  --provider=aws --product=EC2 \
  --from=2024-01-01T00:00:00Z --to=2024-02-01T00:00:00Z

# Coverage by Savings Plans
pup costs ccm commitments coverage \
  --provider=aws --product=EC2 \
  --from=2024-01-01T00:00:00Z --to=2024-02-01T00:00:00Z \
  --commitment-type=SP

# Savings summary
pup costs ccm commitments savings \
  --provider=aws --product=RDS \
  --from=2024-01-01T00:00:00Z --to=2024-02-01T00:00:00Z

# On-demand hot spots (unreserved spend)
pup costs ccm commitments hotspots \
  --provider=aws --product=EC2 \
  --from=2024-01-01T00:00:00Z --to=2024-02-01T00:00:00Z

# Utilization time series
pup costs ccm commitments utilization-ts \
  --provider=aws --product=EC2 \
  --from=2024-01-01T00:00:00Z --to=2024-02-01T00:00:00Z

# Coverage time series
pup costs ccm commitments coverage-ts \
  --provider=aws --product=EC2 \
  --from=2024-01-01T00:00:00Z --to=2024-02-01T00:00:00Z

# Savings time series
pup costs ccm commitments savings-ts \
  --provider=aws --product=EC2 \
  --from=2024-01-01T00:00:00Z --to=2024-02-01T00:00:00Z

# List all commitment programs
pup costs ccm commitments list \
  --provider=aws --product=EC2 \
  --from=2024-01-01T00:00:00Z --to=2024-02-01T00:00:00Z
```

## Output Formatting

### JSON Output (Default)
```bash
pup monitors list --output=json
```

### YAML Output
```bash
pup monitors list --output=yaml
```

### Table Output
```bash
pup monitors list --output=table
```

### Custom Fields
```bash
pup monitors list --fields="id,name,type,status"
```

## Advanced Usage

### Custom Config File
```bash
pup --config=/path/to/config.yaml monitors list
```

### Specify Datadog Site
```bash
pup --site=datadoghq.eu monitors list
```

### Verbose Output (Debug)
```bash
pup --verbose monitors list
```

### Skip Confirmation Prompts
```bash
pup --yes monitors delete 12345678
```

### Read-Only Mode
```bash
# Block all write operations (create, update, delete)
pup --read-only monitors list
pup --read-only dashboards list

# Also available via env var or config file
DD_READ_ONLY=true pup monitors list
```

## Common Workflows

### Monitoring Dashboard
```bash
# List monitors for a service
pup monitors list --tag="service:api" --output=table

# Check recent logs
pup logs search --query="service:api" --from="1h" --output=table

# Query metrics
pup metrics query --query="avg:api.latency{*}" --from="1h"
```

### Incident Response
```bash
# Create incident
pup incidents create --title="API Down" --severity="SEV-1"

# Search related logs
pup logs search --query="status:error service:api" --from="1h"

# Check monitors
pup monitors list --tag="service:api"

# Update incident status
pup incidents update "incident-id" --status="investigating"
```

### Security Audit
```bash
# List recent security signals
pup security signals list --from="24h"

# Check security rules
pup security rules list

# Search security findings
pup security findings search --query="@severity:critical"

# Review audit logs
pup audit-logs list --from="7d"
```

### Infrastructure Review
```bash
# List all hosts
pup infrastructure hosts list --output=table

# Get host details
pup infrastructure hosts get "host-name"

# Review host tags
pup tags list --output=table
```

## Time Range Formats

### Relative Times
```bash
--from="1h"    # 1 hour ago
--from="30m"   # 30 minutes ago
--from="7d"    # 7 days ago
--from="now"   # Current time
```

### Absolute Times
```bash
--from="2024-02-04T10:00:00Z"
--to="2024-02-04T11:00:00Z"
```

### Unix Timestamps
```bash
--from="1707048000"  # Unix timestamp in seconds
```

## Environment Variables

```bash
# Authentication
export DD_API_KEY="your-api-key"
export DD_APP_KEY="your-app-key"
export DD_SITE="datadoghq.com"

# Configuration
export PUP_CONFIG="/path/to/config.yaml"
export PUP_OUTPUT="json"
export PUP_LOG_LEVEL="debug"
```

## ACP Server (AI Agent Integration)

`pup acp serve` starts a local HTTP server that lets AI coding assistants and agents
talk directly to Datadog Bits AI. It speaks two protocols:

- **ACP** ([Agent Communication Protocol](https://agentcommunicationprotocol.dev/)) — for ACP-native clients
- **OpenAI-compatible** — for tools like [opencode](https://opencode.ai), Cursor, or any `@ai-sdk/openai-compatible` client

### Quick Start

```bash
# Authenticate first (notebooks_read + notebooks_write scopes required)
pup auth login

# Start the server (auto-discovers your first Datadog Bits AI agent)
pup acp serve

# Specify a particular agent
pup acp serve --agent-id <uuid>

# Custom port or bind address
pup acp serve --port 8080
pup acp serve --host 0.0.0.0 --port 9099
```

### Endpoints

| Method | Path | Protocol | Description |
|--------|------|----------|-------------|
| GET | `/agent.json` | ACP | Agent card / capability discovery |
| POST | `/runs` | ACP | Synchronous run — returns full response |
| POST | `/runs/stream` | ACP | Streaming run — SSE events |
| GET | `/models` or `/v1/models` | OpenAI | Model list |
| POST | `/chat/completions` or `/v1/chat/completions` | OpenAI | Chat completions (streaming or sync) |

### Testing with curl

```bash
# ACP sync
curl -s -X POST http://127.0.0.1:9099/runs \
  -H "Content-Type: application/json" \
  -d '{"input": [{"role": "user", "content": [{"type": "text", "text": "list my monitors with status alert"}]}]}' \
  | jq .output[0].content[0].text

# ACP streaming
curl -X POST http://127.0.0.1:9099/runs/stream \
  -H "Content-Type: application/json" \
  -d '{"input": [{"role": "user", "content": [{"type": "text", "text": "what services have errors in the last hour?"}]}]}'

# OpenAI-compatible
curl -s -X POST http://127.0.0.1:9099/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model": "datadog-ai", "messages": [{"role": "user", "content": "how many monitors are currently alerting?"}]}' \
  | jq .choices[0].message.content
```

### opencode Setup

Add to `~/Library/Application Support/opencode/opencode.jsonc` (macOS) or
`~/.config/opencode/opencode.jsonc` (Linux):

```jsonc
{
  "$schema": "https://opencode.ai/config.json",
  "provider": {
    "datadog": {
      "name": "Datadog AI",
      "npm": "@ai-sdk/openai-compatible",
      "models": {
        "datadog-ai": {
          "name": "Datadog AI Agent"
        }
      },
      "options": {
        "baseURL": "http://127.0.0.1:9099"
      }
    }
  }
}
```

Then start the server (`pup acp serve`) and select the **Datadog AI** provider in opencode.

## Configuration File

Create `~/.config/pup/config.yaml`:

```yaml
site: datadoghq.com
output: json
verbose: false

# Default time ranges
default_from: 1h
default_to: now

# Output preferences
output_format: json
table_max_width: 120
```
