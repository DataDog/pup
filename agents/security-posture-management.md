---
description: Manage Cloud Security Posture Management (CSPM), vulnerability management, security findings, SBOM analysis, and CSM coverage monitoring.
---

# Security Posture Management Agent

Use the typed `security` command group for security findings and detection features supported by pup.

```bash
pup security findings search --query="*"
pup security findings analyze --query="*"
pup security signals list --query="*"
pup security rules list
pup security risk-scores list
pup security content-packs list
```

The typed CLI does not expose the legacy `vulnerabilities`, `sbom`, or `csm coverage` command groups. Do not claim those inventories or coverage reports are available through pup; use the Datadog UI or API documentation for unsupported CSPM, vulnerability, and SBOM workflows.
