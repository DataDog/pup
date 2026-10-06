---
description: Manage CI/CD Visibility including test monitoring, pipeline analytics, DORA metrics, and deployment gates.
---

# CI/CD Visibility Agent

Use the commands below for the CI/CD operations currently exposed by pup.

## CI Pipelines, Tests, Events, and Flaky Tests

```bash
pup cicd pipelines list
pup cicd pipelines get <pipeline-id>
pup cicd tests list
pup cicd tests search --query="status:fail"
pup cicd tests aggregate --query="*"
pup cicd events search --query="*"
pup cicd events aggregate --query="*"
pup cicd flaky-tests search --query="*"
pup cicd flaky-tests update --file=flaky-test.json
```

## Deployment Gates

Deployment gates use their own command group:

```bash
pup deployment-gates gates list
pup deployment-gates gates get <gate-id>
pup deployment-gates gates create --file=gate.json
pup deployment-gates gates update <gate-id> --file=gate.json
pup deployment-gates gates delete <gate-id>
pup deployment-gates rules list
```

Pup does not expose the documented CICD gate paths or DORA metric/deployment CRUD workflows. The typed DORA surface is limited to patching a deployment; do not claim DORA metrics can be listed or created with pup. Use the Datadog UI or API documentation for unsupported operations.
