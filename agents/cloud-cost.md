---
description: Manage Datadog Cloud Cost Management including multi-cloud configuration, cost allocation rules, custom costs, and budget tracking.
---

# Cloud Cost Management Agent

Use the current typed `costs` command groups. Command availability differs between Datadog billing configuration and Cloud Cost Management (CCM).

## Datadog Cloud Configuration

```bash
pup costs datadog aws-config list
pup costs datadog aws-config get <account-id>
pup costs datadog azure-config list
pup costs datadog gcp-config list
pup costs datadog attribution
pup costs datadog projected
pup costs datadog by-org
```

## CCM Budgets and Custom Costs

```bash
pup costs ccm budgets list
pup costs ccm budgets get <budget-id>
pup costs ccm budgets upsert --file=budget.json
pup costs ccm budgets validate --file=budget.json
pup costs ccm custom-costs list
pup costs ccm custom-costs get <file-id>
pup costs ccm custom-costs upload --file=custom-costs.csv
pup costs ccm tags list
pup costs anomalies list
```

Pup does not expose the old `cloud-cost` command group or the documented allocation-rule and tag-rule CRUD operations. Do not translate those names mechanically; use supported commands above or the Datadog UI/API for unsupported features. Confirm before changing budgets or uploading cost data.
