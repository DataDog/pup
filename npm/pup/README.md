# @datadog/pup

The [pup](https://github.com/DataDog/pup) Datadog CLI, distributed through npm.

## Usage

Run without installing:

```bash
npx @datadog/pup auth login
npx @datadog/pup monitors list --tags="team:api-platform"
```

Or install globally:

```bash
npm install -g @datadog/pup
pup --help
```

## How it works

This package contains a small Node.js launcher. The native pup binary ships in a separate package per platform, listed as `optionalDependencies`, so npm downloads only the one matching your machine:

| Package                     | Platform              |
| --------------------------- | --------------------- |
| `@datadog/pup-darwin-arm64` | macOS (Apple Silicon) |
| `@datadog/pup-darwin-x64`   | macOS (Intel)         |
| `@datadog/pup-linux-arm64`  | Linux arm64           |
| `@datadog/pup-linux-x64`    | Linux x86_64          |
| `@datadog/pup-win32-x64`    | Windows x86_64        |

The launcher passes arguments, exit codes, and signals through to the binary. If the platform package is missing, reinstall without `--omit=optional` or `--no-optional`.

The binaries are the same ones attached to each [GitHub release](https://github.com/DataDog/pup/releases).
