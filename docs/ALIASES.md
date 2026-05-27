# Aliases

## Overview

Aliases let you define short names for any `pup` command. Once set, an alias
can be used exactly like a built-in command — with the same global flags and
any extra arguments appended after the alias name.

```bash
pup alias set infra-list "infrastructure hosts list"
pup infra-list                            # same as: pup infrastructure hosts list
pup infra-list --filter env:production    # extra args are appended to the expansion
pup --output json infra-list             # global flags before the alias still work
```

## Managing Aliases

```bash
# Create or update an alias
pup alias set infra-list "infrastructure hosts list"
pup alias set prod-errors "logs search --query='status:error' --tag='env:prod'"

# List all configured aliases
pup alias list
pup --output json alias list

# Delete one or more aliases
pup alias delete infra-list
pup alias delete infra-list prod-errors

# Bulk-import from a YAML or JSON file
pup alias import my-aliases.yaml
pup alias import my-aliases.json
```

### Import file format

YAML:

```yaml
infra-list: infrastructure hosts list
prod-errors: logs search --query='status:error' --tag='env:prod'
```

JSON:

```json
{
  "infra-list": "infrastructure hosts list",
  "prod-errors": "logs search --query='status:error' --tag='env:prod'"
}
```

## Storage

Aliases are stored in a YAML file named `aliases.yaml` inside pup's config
directory. The location depends on your platform:

| Operating System | Path |
|------------------|------|
| macOS | `~/Library/Application Support/pup/aliases.yaml` |
| Linux | `~/.config/pup/aliases.yaml` (or `$XDG_CONFIG_HOME/pup/aliases.yaml`) |
| Windows | `%APPDATA%\pup\aliases.yaml` |

The file is created automatically on the first `pup alias set`. It is a plain
key/value map and can be edited directly in any text editor. Importing merges
into the existing file — aliases not present in the import file are left
untouched.

## How Expansion Works

When `pup` starts it reads `~/.config/pup/aliases.yaml` and checks whether the
first positional argument (the subcommand) matches a known alias. If it does,
the alias token is replaced with the stored command tokens **before** the
argument list is handed to the command parser.

This means:

- **Global flags before the alias are preserved.** `pup --output json infra-list`
  expands to `pup --output json infrastructure hosts list`.
- **Extra arguments after the alias are appended.** `pup infra-list --filter env:prod`
  expands to `pup infrastructure hosts list --filter env:prod`.
- **Extensions take priority over aliases.** If a pup extension binary matches
  the alias name, the extension is dispatched instead.
- **Built-in commands cannot be aliased over.** An alias named `monitors` or
  `logs` will never shadow the built-in of the same name.

## Implementation

| File | Role |
|------|------|
| `src/commands/alias.rs` | CRUD commands (`list`, `set`, `delete`, `import`) and expansion logic (`expand`, `apply_expansion`) |
| `src/main.rs` | Calls `commands::alias::expand` on startup, before clap parses the argument list |

The core expansion function (`apply_expansion`) operates on an in-memory alias
map, making it straightforward to unit-test without filesystem access. The
public `expand` function wraps it with a disk read from `aliases.yaml`.

## Examples

```bash
# Shorten a frequently used infrastructure command
pup alias set infra-list "infrastructure hosts list"
pup infra-list
pup infra-list --filter env:production --count 50

# Bookmark a common log search
pup alias set prod-errors "logs search --query='status:error' --tag='env:prod'"
pup prod-errors
pup prod-errors --from 30m   # append extra flags on the fly

# Snapshot a metrics query
pup alias set cpu "metrics query --query='avg:system.cpu.user{*}' --from=1h"
pup cpu

# Review all aliases
pup alias list

# Remove an alias you no longer need
pup alias delete cpu

# Share a set of aliases with your team via a checked-in file
pup alias import team-aliases.yaml
```
