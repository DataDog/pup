---
name: pup
description: Datadog API CLI with 49 command groups, 300+ subcommands. Skills and domain agents for monitoring, logs, APM, security, and infrastructure.
metadata:
  version: "0.25.0"
  author:
    name: Datadog
    email: support@datadoghq.com
  repository: https://github.com/DataDog/pup
  tags: datadog,cli,monitoring,logs,apm,metrics,security,infrastructure
---

# Datadog Pup CLI

Rust-based CLI for Datadog APIs. 49 command groups, 300+ subcommands across 53 command modules.

## Install Skills

```bash
# Install all skills and agents for the auto-detected AI assistant
pup skills install

# Or install for a specific platform (claude, cursor, codex, opencode, pi)
pup skills install claude
pup skills install codex
pup skills install cursor

# Install for every supported platform at once
pup skills install all

# Install a single skill by name
pup skills install claude --name dd-pup

# Default scope is user-global; pass --project to install into the repo
pup skills install claude --project

# List all available skills
pup skills list
```

## Skills

| Skill | Description |
|-------|-------------|
| **dd-pup** | Primary CLI - all pup commands, auth, site config |
| **dd-monitors** | Create, manage, mute monitors and alerts |
| **dd-logs** | Search logs, pipelines, archives |
| **dd-apm** | Traces, services, performance analysis |
| **dd-docs** | Search Datadog documentation via llms.txt |
| **dd-code-generation** | CLI vs code-gen decision, multi-language examples |
| **dd-file-issue** | Issue routing to correct repo, duplicate search |

## Domain Agents (48)

Specialized agents for every Datadog API domain: logs, metrics, dashboards, monitors, APM, security, infrastructure, incidents, and more.

```bash
pup skills list --category=agent
```

## Quick Start

```bash
# Install pup
brew tap datadog-labs/pack && brew install pup

# Authenticate
pup auth login

# Install skills for your AI assistant
pup skills install
```
