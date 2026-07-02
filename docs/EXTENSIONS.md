# Extensions Guide

## Overview

Pup extensions are standalone executables that add new subcommands to pup. When you run `pup foo ...`, pup checks if `foo` is a built-in command. If not, it looks for an installed extension named `pup-foo` and runs it with your arguments and auth credentials.

Extensions let teams ship experimental features independently without modifying pup's core or doing a full release. Any language works - extensions are just executables.

## Quick Start

### Install an extension from GitHub

```bash
# Install from a GitHub repository (downloads the latest release)
pup extension install owner/pup-foo

# Install a specific release version
pup extension install owner/pup-foo --tag v1.0.0

# List extensions available from a shared release repository
pup extension list-remote owner/repo

# Install one extension from a shared release repository
pup extension install owner/repo --extension foo

# Install all extensions from a shared release repository's latest release
pup extension install owner/repo --all
```

### Install from a local file

```bash
# Install from a local binary
pup extension install --local /path/to/pup-foo

# Install as a symlink (for development)
pup extension install --local /path/to/pup-foo --link
```

### Use it

```bash
# The extension becomes a pup subcommand
pup foo --some-flag value
```

### Manage extensions

```bash
# List installed extensions
pup extension list

# List in table format
pup -o table extension list

# Upgrade a single extension to the latest release
pup extension upgrade foo

# Upgrade all installed extensions
pup extension upgrade --all

# Remove an extension
pup extension remove foo
```

## Writing an Extension

An extension is any executable named `pup-<name>`. It can be a shell script, a compiled binary, a Python script with a shebang - anything that can run.

### Minimal example (shell script)

```bash
#!/bin/bash
echo "Hello from pup extension!"
echo "Site: $DD_SITE"
echo "Args: $@"
```

Save this as `pup-foo`, make it executable (`chmod +x pup-foo`), and install it:

```bash
pup extension install --local ./pup-foo
pup foo world
# Output:
# Hello from pup extension!
# Site: datadoghq.com
# Args: world
```

### Naming rules

- The executable must be named `pup-<name>` (or `pup-<name>.exe` on Windows)
- `<name>` must be lowercase letters, digits, and hyphens only, starting with a letter
- `<name>` must not conflict with a built-in pup command (e.g., `monitors`, `logs`, `auth`)

Valid: `pup-foo`, `pup-cost-report`, `pup-lint`
Invalid: `pup-Foo`, `pup-2fast`, `pup-my_tool`, `pup-monitors`

## Auth Forwarding

Extensions receive pup's auth credentials via environment variables. This means extensions don't need to implement keychain access, token refresh, or config file parsing.

### Environment variables set by pup

| Variable | Set When | Value |
|---|---|---|
| `DD_ACCESS_TOKEN` | OAuth2 auth is active | Current (non-expired) access token |
| `DD_API_KEY` | API key is configured | API key |
| `DD_APP_KEY` | App key is configured | Application key |
| `DD_SITE` | Always | Datadog site (e.g., `datadoghq.com`) |
| `DD_ORG` | Org is specified | Named org session |
| `PUP_OUTPUT` | Always | Output format (`json`, `table`, `yaml`, `csv`, `tsv`) |
| `PUP_FILTER` | `--jq` flag is set | The jq expression, verbatim |
| `PUP_AUTO_APPROVE` | `--yes` flag or agent mode | `true` |
| `PUP_READ_ONLY` | Read-only mode | `true` |
| `PUP_AGENT_MODE` | Agent mode | `true` |

Pup refreshes the OAuth2 token if needed before passing it to the extension, so extensions always receive a valid token.

Variables not active in the current session are explicitly removed from the child environment to prevent stale credentials from leaking through the parent shell.

The `PUP_OUTPUT`, `PUP_FILTER`, `PUP_AGENT_MODE`, `PUP_READ_ONLY`, and `PUP_AUTO_APPROVE` variables are read back by a child `pup` process. So if your extension shells out to `pup` (see below), those nested calls automatically inherit the format, `--jq` filter, and mode the user selected on the parent command. An extension that prints its own JSON directly — rather than delegating to a nested `pup` call — still needs to read `PUP_FILTER` itself and apply the jq expression if it wants to honor `--jq`.

### Example: using auth in a Python extension

The example below hand-rolls the HTTP call. If pup is on `PATH` (it is, when pup dispatched your extension), prefer shelling out to `pup api` and `pup format` instead — see [Reusing pup's client and formatter](#reusing-pups-client-and-formatter) below.

```python
#!/usr/bin/env python3
import os, requests

token = os.environ.get("DD_ACCESS_TOKEN")
site = os.environ.get("DD_SITE", "datadoghq.com")

headers = {"Authorization": f"Bearer {token}"} if token else {
    "DD-API-KEY": os.environ.get("DD_API_KEY", ""),
    "DD-APPLICATION-KEY": os.environ.get("DD_APP_KEY", ""),
}

resp = requests.get(f"https://api.{site}/api/v1/dashboard", headers=headers)
print(resp.json())
```

### Example: using auth in a Rust extension

Extensions written in Rust can use the `datadog-api-client` crate. The standard Datadog SDK env vars (`DD_API_KEY`, `DD_APP_KEY`, `DD_SITE`) are forwarded automatically, so most SDKs will work without any extra configuration.

## Reusing pup's client and formatter

The examples above hand-roll HTTP and auth. You usually don't need to. Because pup is on `PATH` when it dispatched your extension, an extension in **any language** can shell out to the parent `pup` binary and reuse pup's request handling and output formatting directly — no auth, refresh, site-resolution, or table/CSV code of your own.

### Make authenticated API calls with `pup api`

`pup api <ENDPOINT>` reuses pup's full auth handler: it chooses OAuth bearer vs. API-key auth, applies the per-endpoint fallback for endpoints that don't accept OAuth (e.g. `/api/v2/api_keys`, fleet, cost), sets the branded User-Agent, and resolves the site. The extension already has a fresh `DD_ACCESS_TOKEN` (or the `DD_API_KEY`/`DD_APP_KEY` pair) in its environment, so these calls are authenticated automatically.

```bash
#!/bin/bash
# GET as JSON, then extract fields. --silent suppresses pup's own rendering.
pup api v2/monitors --silent | jq -r '.[].name'

# POST a typed body (-F coerces ints/bools/null; -f keeps raw strings).
pup api v2/tags/hosts/myhost -X POST -F source=web
```

### Render output with `pup format`

`pup format` (alias `fmt`) reads a JSON document from stdin (or `--input FILE`) and prints it using the caller's output format and agent mode — the same JSON / YAML / table / CSV / TSV rendering and agent envelope every built-in command uses. Because pup forwards `PUP_OUTPUT` and a child `pup` reads it back, your extension inherits the format the user originally requested.

```bash
#!/bin/bash
results='[{"id":1,"name":"alpha"},{"id":2,"name":"beta"}]'

# Honor whatever -o the user passed to `pup foo`.
echo "$results" | pup format

# Or force a specific format.
echo "$results" | pup format --output table

# Populate the agent-mode envelope metadata.
echo "$results" | pup format --count 2 --command "foo list"
```

### Combine them

A fully consistent extension that adds zero auth or formatting code:

```bash
#!/bin/bash
pup api v2/monitors --silent | pup format
```

## Global Flags

Pup's global flags (`--output`, `--yes`, `--agent`, `--read-only`, `--org`) are parsed by pup before dispatching to the extension. They are NOT passed as CLI arguments to the extension - instead, they are forwarded as environment variables (see the table above).

```bash
# --output table is consumed by pup, extension receives PUP_OUTPUT=table
pup --output table foo do-something

# The extension receives only: ["do-something"]
# Not: ["--output", "table", "do-something"]
```

Extension-specific flags (anything pup doesn't recognize) are passed through to the extension unchanged:

```bash
pup foo plan --workspace prod --var-file vars.tfvars
# Extension receives: ["plan", "--workspace", "prod", "--var-file", "vars.tfvars"]
```

## Installation Details

### GitHub install

```bash
pup extension install owner/repo
```

Downloads the platform-specific binary from the repository's latest GitHub Release and installs it. The extension name is derived from the repo name (stripping the `pup-` prefix if present). For example, `owner/pup-foo` installs as `foo`.

GitHub releases must include assets following the naming convention:

```
pup-<name>-<os>-<arch>
```

Where:
- `<name>` is the extension name (e.g., `foo`)
- `<os>` is one of: `darwin`, `linux`, `windows`
- `<arch>` is one of: `x86_64`, `aarch64`

Example assets for an extension named `foo`:

```
pup-foo-darwin-aarch64
pup-foo-darwin-x86_64
pup-foo-linux-aarch64
pup-foo-linux-x86_64
pup-foo-windows-x86_64.exe
```

To install a specific release tag:

```bash
pup extension install owner/repo --tag v1.0.0
```

`--tag` expects the exact GitHub release tag. If the release is tagged `v1.0.0`, use `--tag v1.0.0`, not `--tag 1.0.0`.

### Shared GitHub release repositories

A GitHub repository can also publish one platform archive containing multiple top-level extension executables:

```
repo_1.2.3_Darwin_arm64.tar.gz
repo_1.2.3_Linux_x86_64.tar.gz
repo_1.2.3_Windows_x86_64.zip
```

Each archive can contain executables such as:

```
pup-foo
pup-bar
```

List remote versions inferred from release archives:

```bash
pup extension list-remote owner/repo
pup extension list-remote owner/repo --extension foo
```

The table output shows both the extension version and the GitHub release tag in parentheses. Use the tag value when installing a specific release.

Install one extension from the newest release archive that contains it:

```bash
pup extension install owner/repo --extension foo
```

Install one extension from a specific release tag:

```bash
pup extension install owner/repo --extension foo --tag v1.0.0
```

Install all extensions from the latest release archive:

```bash
pup extension install owner/repo --all
```

If a release archive contains exactly one extension, `pup extension install owner/repo` can infer it. If it contains multiple extensions, pup asks you to choose `--extension <name>` or `--all`.

Private GitHub repositories are supported with either an explicit token or an existing GitHub CLI login. Token resolution order is:

1. `GH_TOKEN`, `GITHUB_TOKEN`, or `HOMEBREW_GITHUB_API_TOKEN`
2. the active GitHub CLI account from `gh auth token --hostname github.com`
3. anonymous GitHub access for public repositories

`gh` is optional. Pup uses it only as a credential helper when no explicit token is set. Pup does not switch accounts, refresh scopes, create tokens, store GitHub tokens, or pass GitHub tokens to extensions. If access fails and multiple GitHub CLI accounts are configured, choose the desired account with `gh auth switch --hostname github.com` or set `GH_TOKEN` explicitly.

### Local install (copy)

```bash
pup extension install --local /path/to/pup-foo
```

Copies the binary into pup's extensions directory and sets executable permissions.

### Local install (symlink)

```bash
pup extension install --local /path/to/pup-foo --link
```

Creates a symlink instead of copying. Useful during development so changes to the source binary take effect immediately without reinstalling.

### Custom name

```bash
pup extension install --local /path/to/my-binary --name foo
```

By default, the extension name is derived from the filename (stripping `pup-` prefix and `.exe` suffix) for local installs, or from the repo name for GitHub installs. Use `--name` to override.

### Force reinstall

```bash
pup extension install --local /path/to/pup-foo --force
pup extension install owner/repo --force
```

Overwrites an existing extension with the same name.

## Upgrading Extensions

### Upgrade a single extension

```bash
pup extension upgrade foo
```

Checks GitHub for a newer version. For single-binary repositories, pup checks the latest release. For shared release repositories, pup searches releases newest-first and upgrades to the newest release archive that contains that extension. If the extension is already at the latest version, prints a message and does nothing.

### Upgrade all extensions

```bash
pup extension upgrade --all
```

Iterates through all installed extensions. GitHub-sourced extensions are checked for updates and upgraded if a newer release is available. Local extensions are skipped with a message.

Only GitHub-sourced extensions can be upgraded automatically. Extensions installed from local files must be reinstalled manually:

```bash
pup extension install --local /path/to/updated-binary --force
```

## Extension Directory

Extensions are stored in pup's config directory:

```
<config_dir>/extensions/
  pup-foo/
    pup-foo              # the executable
    manifest.json        # metadata (written by pup at install time)
```

The config directory location depends on your platform:
- **macOS**: `~/Library/Application Support/pup/extensions/`
- **Linux**: `~/.config/pup/extensions/` (or `$XDG_CONFIG_HOME/pup/extensions/`)
- **Windows**: `%APPDATA%\pup\extensions\`

Override with `PUP_CONFIG_DIR` environment variable.

## Exit Codes

Pup propagates the extension's exit code. If the extension exits with code 1, pup exits with code 1. On Unix, if the extension is killed by a signal, pup exits with 128 + signal number (standard convention).

## Read-Only Mode

When pup runs in read-only mode (`--read-only`), the built-in `pup extension install`, `pup extension remove`, and `pup extension upgrade` commands are blocked. Extension dispatch itself is not blocked - instead, `PUP_READ_ONLY=true` is forwarded and the extension is responsible for honoring it.

## Command Discovery via `pup agent schema`

Extensions that need to know what pup commands are available (e.g., to generate tool definitions for AI assistants) can consume the output of `pup agent schema`. This outputs a JSON object describing pup's full command tree.

```bash
pup agent schema | jq '.commands[0]'
```

### Schema structure per command

Each command in the `commands` array has:

| Field | Type | Present | Description |
|---|---|---|---|
| `name` | string | Always | Command name (e.g., `"get"`) |
| `full_path` | string | Always | Full command path (e.g., `"monitors get"`) |
| `description` | string | When available | Human-readable description |
| `read_only` | bool | Always | `true` if the command does not modify state |
| `args` | array | When command has positional args | Positional arguments (see below) |
| `flags` | array | When command has flags | Named `--flags` (see below) |
| `subcommands` | array | When command is a group | Nested commands |

### Positional args (`args[]`)

| Field | Type | Description |
|---|---|---|
| `name` | string | Argument identifier (e.g., `"monitor_id"`) |
| `type` | string | Always `"string"` |
| `required` | bool | Whether the argument is mandatory |
| `index` | number | 1-based position order for CLI invocation |
| `description` | string | Human-readable description (when available) |

### Named flags (`flags[]`)

| Field | Type | Description |
|---|---|---|
| `name` | string | Flag with prefix (e.g., `"--query"`) |
| `type` | string | `"bool"`, `"int"`, or `"string"` |
| `required` | bool | Whether the flag is mandatory |
| `default` | string | Default value (when one exists) |
| `description` | string | Human-readable description (when available) |

### Identifying actionable commands

Only **leaf commands** (those without `subcommands`) can be executed. Group commands like `monitors` just organize subcommands. To find leaf commands, walk the tree and collect commands where `subcommands` is absent.

### Constructing CLI invocations

To execute a command from the schema:

```
pup --output json --yes <full_path segments> <positional args in index order> --flag value
```

Positional args must come before named flags, ordered by their `index` field.

### Example: building a tool definition from schema

```python
import json, subprocess

schema = json.loads(subprocess.check_output(["pup", "agent", "schema"]))

for cmd in schema["commands"]:
    for leaf in walk_leaves(cmd):  # your recursive walker
        tool = {
            "name": leaf["full_path"].replace(" ", "_"),
            "description": leaf.get("description", ""),
            "parameters": {}
        }
        # Merge args and flags into parameters...
```

## Migrating a Feature to an Extension

To extract an existing pup feature into an extension:

1. Create a standalone executable that implements the feature
2. Read auth from environment variables instead of calling pup's internal auth
3. Name it `pup-<feature>` and test it via `pup extension install --local`
4. Remove the feature from pup's core `Commands` enum
5. Distribute the extension binary separately

## Limitations

- **Source must be a regular file**: `pup extension install --local` requires the source path to be a regular file, not a directory.
- **Agent-mode help**: `pup --agent <ext-name> --help` prints pup's top-level schema, not the extension's help. In normal mode, `--help` is passed through to the extension.
- **No signing**: Downloaded binaries are not signed. If a release includes `checksums.txt`, pup verifies the selected archive checksum before installing. Only install extensions from trusted sources.
