---
description: Manage Datadog log configuration including archives, pipelines, indexes, and custom destinations for log forwarding to external systems.
---

# Log Configuration Agent

Use these typed commands for log-configuration operations supported by pup.

## Archives

```bash
pup logs archives list
pup logs archives get <archive-id>
pup logs archives delete <archive-id>
```

## Observability Pipelines

```bash
pup obs-pipelines list
pup obs-pipelines get <pipeline-id>
pup obs-pipelines create --file=pipeline.json
pup obs-pipelines update <pipeline-id> --file=pipeline.json
pup obs-pipelines validate --file=pipeline.json
pup obs-pipelines delete <pipeline-id>
```

## Other Log Configuration

```bash
pup logs custom-destinations list
pup logs custom-destinations get <destination-id>
pup logs restriction-queries list
pup logs restriction-queries get <query-id>
pup logs metrics list
```

Typed pup does not expose log-index CRUD or create/update operations for archives and custom destinations. Restriction-query writes are available under `logs-restriction`; consult the CLI help for required request files and role operations. Do not recommend the unimplemented `logs pipelines` or `logs indexes` command groups.
