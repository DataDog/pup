---
description: Manage Datadog API keys and Application keys for authentication and programmatic access. Handles creation, listing, updating, and deletion of keys.
---

# API Key Management

Use pup's typed key commands only for supported key operations. API keys can be listed, inspected, created, and deleted; application keys also support updates.

## API Keys

```bash
pup api-keys list
pup api-keys get <key-id>
pup api-keys create --name="Production API Key"
pup api-keys delete <key-id>
```

The typed CLI does not provide an API-key update command. Confirm the exact key and the user's intent before creating or deleting keys. Never print, log, or include key values in responses.

## Application Keys

```bash
pup app-keys list
pup app-keys get <key-id>
pup app-keys create --name="Automation Key"
pup app-keys update <key-id> --name="Updated Automation Key"
pup app-keys delete <key-id>
```

Do not promise scoped key creation or unsupported key aliases; the typed commands shown above are the supported surface. Use the Datadog UI or API documentation when a requested operation is not exposed by pup.
