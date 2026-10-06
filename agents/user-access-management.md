---
description: Comprehensive user and team management including users, service accounts, teams, memberships, SCIM, authentication mappings, and permissions.
---

# User and Access Management Agent

Use only the typed user, key, organization, and on-call commands listed here.

## Users, Organizations, and Roles

```bash
pup users list
pup users get <user-id>
pup users roles list
pup organizations list
pup organizations get <organization-id>
```

## API and Application Keys

```bash
pup api-keys list
pup api-keys get <key-id>
pup app-keys list
pup app-keys get <key-id>
```

## Service Accounts and Seats

```bash
pup users service-accounts create --file=service-account.json
pup users service-accounts app-keys list <service-account-id>
pup users service-accounts app-keys create <service-account-id> --file=app-key.json
pup users seats users list
```

Pup does not expose the former `teams`, `service-accounts`, or `scim` command groups, nor general user create/update/disable/invitation, role mutation, or permission commands. Some on-call team membership operations are available under `on-call teams memberships`; these are not a substitute for general team administration. Use the Datadog UI or API documentation for unsupported operations. Confirm before creating or deleting keys or service accounts.
