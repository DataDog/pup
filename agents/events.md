---
description: Manage Datadog events including event submission, search, filtering, and custom event stream management.
---

# Events Agent

The typed CLI supports posting events, listing recent events, searching, and retrieving an event by ID.

```bash
pup events list
pup events search --query="source:deploy"
pup events get <event-id>
pup events post --file=event.json
```

Use `post` to submit an event; `submit` is not a pup subcommand. Do not claim pup can configure custom event streams unless a supported command performs that operation.
