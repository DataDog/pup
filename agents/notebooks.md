---
description: Manage Datadog Notebooks for collaborative investigation, documentation, and reporting with mixed content cells.
---

# Notebooks Agent

Use the notebook search command (not `list`) to find notebooks, then inspect or change a selected notebook:

```bash
pup notebooks search --query="incident"
pup notebooks get <notebook-id>
pup notebooks create --file=notebook.json
pup notebooks update <notebook-id> --file=notebook.json
pup notebooks diff <notebook-id> notebook.json
pup notebooks delete <notebook-id>
```

Use `edit` to append cells only when that behavior matches the request. Confirm before creating, updating, editing, or deleting notebook content. The typed CLI does not provide a notebook `list` subcommand.
