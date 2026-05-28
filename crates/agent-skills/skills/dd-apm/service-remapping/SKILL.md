---
name: service-remapping
description: Create and manage APM service remapping rules — rewrite service names at ingestion time to collapse noisy inferred entities, clean up auto-generated names, handle org renames, or normalize naming conventions. Use for any request involving service renaming, service mapping, inferred service cleanup, peer.service normalization, or collapsing fragmented service names.
metadata:
  version: "1.0.0"
  author: datadog-labs
  repository: https://github.com/datadog-labs/agent-skills
  tags: datadog,apm,service-remapping,service-naming,inferred-services,peer-service
  alwaysApply: "false"
  tools: pup
---

# APM Service Remapping

> **Before doing anything else:** Fully resolve all variables in `## Context to resolve before acting`. Do not begin Step 0 until every variable has a concrete value.

---

## How Service Remapping Works — Domain Knowledge

Read this before building any rule. It gives you the mental model to construct the right filter and catch edge cases.

**What remapping does:** A rule intercepts telemetry at ingestion time and rewrites the service name before indexing. A rule says: "for any entity matching this filter, replace its service name with this new value."

**Two entity types — pick the right one:**

| Entity type | `rule_type` integer | What it targets |
|---|---|---|
| **SERVICE** | `0` | Instrumented services — have spans with an explicit `service` tag set by a tracer |
| **INFERRED_ENTITY** | `1` | Auto-detected from outbound calls — named from `peer.service`. **Requires `peer.service` to be set on outbound spans** (see prerequisite below). |

**Prerequisite for inferred entity remapping — `peer.service` must be set:**

Inferred entity remapping only works when the tracer sets `peer.service` on outbound spans. Without it, entities are keyed by `peer.hostname` and remapping rules will not apply.

To enable this, set the following env var on the **instrumented service** (not the downstream dependency):

```bash
DD_TRACE_PEER_SERVICE_DEFAULTS_ENABLED=true
```

This makes the ddtrace tracer automatically propagate `peer.service` from `peer.hostname` on outbound HTTP, gRPC, and database calls. Without this, `pup traces search` will show spans with `peer.hostname` but no `peer.service`, and no service remapping rule will match.

To verify `peer.service` is being set before building a rule:

```bash
pup traces search --query "@peer.service:<ENTITY_NAME>" --from 15m --limit 5
```

If zero results — the tracer is not setting `peer.service`. Ask the user to add `DD_TRACE_PEER_SERVICE_DEFAULTS_ENABLED=true` to their service's environment and redeploy before continuing.

**Filter syntax** — a standard Datadog event-grammar query string:

| Goal | Filter |
|---|---|
| Exact service match | `service:payments` |
| All services with a prefix | `service:deploy-test*` |
| All services with a suffix | `service:*.tropos` |
| All services containing a string | `service:*payments*` |
| All inferred services under a domain | `peer.service:*.shopify.com` |
| Service in one environment only | `service:payments AND env:prod` |

**New name syntax** — the `value` field in `rewrite_tag_rules`:

| Form | Example | Use for |
|---|---|---|
| Static string | `my-service` | Every matched entity gets exactly this name |
| Tag interpolation | `{{service}}` | Substitute the full value of a tag |
| Tag + regex capture | `{{service\|^(.+?)\..*$}}` | Extract part of a tag value (non-greedy capture) |

**Regex constraints for `{{tag\|regex}}`:**
- Maximum **1 capture group** per expression
- **No greedy quantifiers inside capture groups** — use non-greedy variants: `(.+?)` not `(.+)`, `(.*?)` not `(.*)`
- Quantifiers on capture groups themselves (e.g. `(foo)+`) are not allowed

**Five remapping patterns:**

| Pattern | User says… | Filter example | New name example |
|---|---|---|---|
| **N:1 group** | "These N services are all the same thing" | `peer.service:*.shopify.com` | `shopify` |
| **Strip suffix/prefix** | "The name has junk at the end/start" | `service:*.tropos` | `{{service\|^(.+?)\..*$}}` |
| **1:1 rename** | "We renamed this service and Datadog needs to match" | `service:old-auth-service` | `auth-service` |
| **Env split** | "I want separate services per env but they all have the same name" | `service:my-service AND env:prod` | `my-service-prod` |
| **Prefix normalization** | "All services should start with an env or team name" | `service:payments*` | `{{env}}-{{service}}` |

---

## Triggers

Invoke this skill when the user wants to:
- Rename a service in Datadog without re-instrumenting
- Collapse multiple inferred service names into one (e.g. many `api.shopify.com/*` variants → `shopify`)
- Strip environment suffixes, version tags, or deployment metadata baked into service names
- Normalize `peer.service` names to something meaningful
- Rename a service after an org change, product rebrand, or migration
- Split a single service into per-env variants (`my-service` + `env:prod` → `my-service-prod`)
- List, review, or delete existing service remapping rules

Do NOT invoke this skill if:
- The user wants to rename the service in their application code — that requires a tracer config change (`DD_SERVICE`), not a remapping rule
- The user wants to correlate telemetry across infrastructure tags — that is the "Correlate telemetry" action type in the UI, not remapping

---

## Prerequisites

### pup-cli: check, install, and authenticate

### Claude runs

```bash
pup --version
```

If not found:

### Claude runs

```bash
brew tap datadog-labs/pack
brew install pup
```

Check auth:
```bash
pup auth status
```

If not authenticated:

### Claude runs

```bash
pup auth login
```

> This opens a browser tab for OAuth. Complete the login there — Claude will continue once the command exits.

### Credentials for write operations

`pup apm service-remapping list` and `get` work with OAuth. Create, update, and delete require API keys (`DD_API_KEY`, `DD_APP_KEY`, `DD_SITE`) until `apm_service_renaming_write` is added to pup's OAuth scopes.

### Claude runs

```bash
echo "DD_API_KEY set: $([ -n "${DD_API_KEY:-}" ] && echo yes || echo no)"
echo "DD_APP_KEY set: $([ -n "${DD_APP_KEY:-}" ] && echo yes || echo no)"
echo "DD_SITE: ${DD_SITE:-not set (defaulting to datadoghq.com)}"
```

If any are missing and you need to create/update/delete rules:

### What you need to do in a terminal

```bash
export DD_API_KEY=<your-api-key>
export DD_APP_KEY=<your-app-key>
export DD_SITE=datadoghq.com   # adjust for your site
```

> Common sites: `datadoghq.com` (US1), `datadoghq.eu` (EU1), `us3.datadoghq.com`, `us5.datadoghq.com`, `ap1.datadoghq.com`

Wait for the user to set credentials, then re-run the check above before continuing.

---

## Context to resolve before acting

| Variable | How to resolve |
|---|---|
| `ENV` | Ask the user which environment to target. Do NOT assume `prod`. |
| `ORIGINAL_SERVICE` | Current service name(s) to remap — discover with `pup apm services list` or ask the user |
| `ENTITY_TYPE` | Instrumented service (`rule_type: 0`) or inferred entity (`rule_type: 1`)? Ask if unclear — see Domain Knowledge |
| `TARGET_NAME` | The desired new service name — ask the user |
| `PATTERN` | Which pattern applies — identify from the user's description (see Domain Knowledge above) |

---

## Step 0: Discover Current Service Names

If the user hasn't specified exact names to remap, discover what exists first:

### Claude runs

```bash
pup apm services list --env <ENV> --from 1h
pup traces search --query "service:<PARTIAL_NAME>" --from 1h --limit 20
```

Use the output to help the user identify exact service names. Ask the user to confirm which names they want remapped before proceeding.

---

## Step 1: Build the Rule

Work through each component before writing any JSON.

### 1. Entity type

[DECISION: entity type — ask the user if unclear]
- Does the service appear because a tracer explicitly set its `service` tag? → `rule_type: 0` (SERVICE)
- Does it appear in the service map from outbound calls (e.g. a database, queue, or external API)? → `rule_type: 1` (INFERRED_ENTITY)

If the user wants to remap an inferred entity, verify `peer.service` is set before proceeding — see the prerequisite in Domain Knowledge. If it is not set, stop and ask the user to enable `DD_TRACE_PEER_SERVICE_DEFAULTS_ENABLED=true` first.

### 2. Filter

Write a single event-grammar query string targeting the service(s) to remap. Use the filter syntax and pattern table in Domain Knowledge to pick the right form.

### 3. New name (`value`)

Use the new name syntax and regex table in Domain Knowledge to pick the right form. For regex values, apply the constraints listed there.

### 4. Rule name

Suggest a descriptive name. Examples:
- `collapse-shopify-inferred-services`
- `strip-tropos-suffix`
- `rename-old-auth-to-auth-service`
- `env-split-my-service-prod`

---

## Step 2: Preview Impact

Before constructing the JSON, check what will be affected:

### Claude runs

```bash
# Confirm telemetry exists for the targeted service (zero spans = wrong query or wrong env)
pup traces search --query "service:<ORIGINAL_SERVICE>" --from 15m --limit 5

# Check for monitors referencing the old service name
pup monitors list | grep -i "<ORIGINAL_SERVICE>"

# Check for dashboards referencing the old service name
pup dashboards list | grep -i "<ORIGINAL_SERVICE>"

# List existing service remapping rules that may conflict
pup apm service-remapping list
```

Report to the user:

| Item | What to surface |
|---|---|
| **Telemetry volume** | Non-zero spans confirm the filter will match real data. Zero = likely wrong service name or env. |
| **Monitors** | Any monitor referencing the old service name will silently break after remapping. List them and offer to update. |
| **Dashboards** | Any dashboard with the old service name in its title will have stale references after remapping. List them and offer to update. |
| **Conflicting rules** | Existing rules targeting the same service may be overridden. Show conflicts and ask the user to confirm. |

If monitors reference the old service name, ask:
> *"I found `<N>` monitor(s) referencing `<ORIGINAL_SERVICE>`. After remapping, they'll need to be updated to use `<TARGET_NAME>`. Want me to update them now?"*

---

## Step 3: Confirm the Rule

Show the user the planned rule and confirm before creating:

> *"I'm going to create a service remapping rule named `<RULE_NAME>` with filter `<FILTER>` that maps `<ORIGINAL_SERVICE>` → `<TARGET_NAME>` (rule_type: `<TYPE>`). Ready to proceed?"*

Wait for confirmation before continuing.

---

## Step 4: Create the Rule

### Claude runs

```bash
pup apm service-remapping create \
  --name "<RULE_NAME>" \
  --filter "<FILTER>" \
  --rule-type <TYPE> \
  --value "<TARGET_NAME>"
```

If the response contains an `id` field — creation succeeded. Record the `id` and `version` values from the response.

ERROR: `400 Bad Request` with "Filter expression has invalid syntax" — the filter query is malformed. Check glob syntax and boolean operators.

ERROR: `400 Bad Request` with "Template value in target name is invalid" — the `value` regex is invalid. Check: max 1 capture group, non-greedy quantifiers inside groups (`(.+?)` not `(.+)`).

ERROR: `401 Unauthorized` — credentials are invalid or expired. Re-check `DD_API_KEY` and `DD_APP_KEY`.

ERROR: `403 Forbidden` — the API key lacks `apm_service_renaming_write` permission.

---

## Step 5: Verify

Allow 2–5 minutes for the rule to propagate, then confirm it is active.

### For SERVICE rules (rule_type 0)

### Claude runs

```bash
# Confirm new service name appears in APM
pup apm services list --env <ENV> --from 5m

# Confirm traces are arriving under the new name
pup traces search --query "service:<TARGET_NAME>" --from 5m --limit 5
```

If `<TARGET_NAME>` appears in either — rule is active.

### For INFERRED_ENTITY rules (rule_type 1)

Inferred entities don't produce their own spans, so they won't appear in `pup apm services list` or `pup traces search`. Verify in two steps:

**Step 5a — confirm the rule is stored correctly:**

### Claude runs

```bash
pup apm service-remapping get <RULE_ID>
```

Confirm the filter and value match what you intended.

**Step 5b — confirm the entity name changed in the service map:**

Ask the user to check the APM Service Map in the Datadog UI and look for `<TARGET_NAME>` where `<ORIGINAL_SERVICE>` used to appear. The service map is the authoritative view for inferred entity names.

Alternatively, confirm new `peer.service` values are arriving on spans from the instrumented service:

### Claude runs

```bash
pup traces search --query "service:<INSTRUMENTED_SERVICE> @peer.service:<TARGET_NAME>" --from 5m --limit 5
```

If spans appear with `peer.service:<TARGET_NAME>` — rule is active.

ERROR: New name not appearing after 5 minutes:
- Confirm old service is still sending traces with the original `peer.service`: `pup traces search --query "@peer.service:<ORIGINAL_SERVICE>" --from 5m`
- If old name still appears, propagation may still be in progress — wait 2 more minutes and retry
- If neither name appears, confirm `DD_TRACE_PEER_SERVICE_DEFAULTS_ENABLED=true` is set on the instrumented service — without it `peer.service` is never set and the rule will never fire

---

## Managing Existing Rules

### List all rules

### Claude runs

```bash
pup apm service-remapping list
```

### Get a single rule

### Claude runs

```bash
pup apm service-remapping get <RULE_ID>
```

### Update a rule

Update requires the current `version` from list/get output. Show the proposed changes to the user and confirm before running:

### Claude runs

```bash
pup apm service-remapping update <RULE_ID> \
  --name "<RULE_NAME>" \
  --filter "<FILTER>" \
  --rule-type <TYPE> \
  --value "<NEW_NAME>" \
  --version <VERSION>
```

ERROR: `409 Conflict` — the rule was modified since you fetched it. Re-fetch with `get` to get the current version and retry.

### Delete a rule

Show the user the rule's name and filter first, then ask for confirmation. Delete requires both the rule `id` and `version` from the list/get output:

### Claude runs

```bash
pup apm service-remapping delete <RULE_ID> <RULE_VERSION>
```

ERROR: `409 Conflict` — the rule was modified since you fetched it. Re-fetch with `get` to get the current version and retry.

---

## Done

Exit when ALL of the following are true:
- [ ] Rule shown to user and confirmed before creation
- [ ] Rule created and `id` returned in response
- [ ] New service name visible in `pup apm services list`
- [ ] Impacted monitors identified and offered for update
- [ ] User confirmed the remapping matches their intent

---

## Security constraints

- Never write a raw API key into any file or chat message — always use `$DD_API_KEY` and `$DD_APP_KEY`
- Never create or delete a rule without explicit user confirmation — show the full rule before creating
- Never assume `prod` as the environment — always confirm with the user
- Never run DELETE without showing the user the rule's name and filter first
- Never enable `enabled_org_wide` without explicit user confirmation — it affects the entire org
