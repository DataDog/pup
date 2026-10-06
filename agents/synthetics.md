---
description: Manage synthetic tests including listing and viewing test configurations.
---

# Synthetics Agent

Synthetic test commands are nested under `synthetics tests`:

```bash
pup synthetics tests list
pup synthetics tests get <public-id>
pup synthetics tests search --text="login"
pup synthetics locations list
```

The typed CLI does not provide top-level `synthetics list` or `synthetics get` commands. Use the `tests` subgroup and the command-specific help for available filters.
