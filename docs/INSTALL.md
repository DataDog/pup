# `pup install` — design

This doc covers the design and rollout plan for `pup install <component> <platform>` — pup's unified entry point for installing Datadog components on a target. The first concrete leaf is `pup install ssi linux`, the rest follow in subsequent PRs.

## Motivation

Today, multiple agent-skills install Datadog components by embedding `bash -c "$(curl -L install.datadoghq.com/scripts/install_script_agent7.sh)"` in their SKILL.md. Security scanners (Snyk, etc.) correctly flag this as RCE-shaped — and the script itself lives outside any repo we control, with no pinned version or audit trail.

Workarounds we evaluated:

- **Pointing skills at docs** — breaks the "user does nothing" promise that SSI sells.
- **Vendoring the install script in the agent-skills repo** — Datadog ships updates per release; we'd inherit a sync treadmill.
- **Manual apt/yum install in the skill** — only works for the agent; the SSI components (apm-inject, language libs) aren't deb/rpm packages, they're OCI artifacts delivered by the `datadog-installer` binary.
- **Snyk policy exception** — pragmatic but doesn't fix the underlying audit-trail issue.

The clean answer is to move install logic out of markdown and into pup. The skill becomes a single line (`pup install ssi linux --host <X>`), and the install steps execute in compiled, reviewed Rust — using HTTP fetch + SHA-256 verification + signed package install rather than fetch-and-execute shell. This is the same shape every modern install CLI (helm, brew, AWS CLI) already uses.

## Command surface

```
pup install <component> <platform> [args]
```

Two dimensions:

- **Component** — what to install (`ssi`, `agent`, `dbm`, `llm-obs`, `csm`, …)
- **Platform** — where to install (`linux`, `k8s`, `docker`, `windows`, …)

Today, only `ssi linux` is wired up (and as scaffolding — see "Status" below). The matrix grows as future PRs add components/platforms.

### Examples

Today (after this scaffolding PR):

```
pup install ssi linux --host bastion.example.com --user ec2-user --key ~/.ssh/id_ed25519
pup install ssi linux --host bastion.example.com --dry-run
```

Future (out of scope for this PR):

```
pup install agent  linux  --host <X>           # just the agent, no SSI
pup install ssi    k8s    --context <CTX>      # SSI via helm + DatadogAgent CR
pup install dbm    linux  --host <X> --db pg   # Database Monitoring
pup install llm-obs local --runtime python     # LLM Obs SDK
```

## Linux SSI install flow

This is what `pup install ssi linux --host <X>` will do once implemented:

1. Open an SSH session to the target via the `russh` client (already an optional dep of pup; used today by the auth tunnel server).
2. Detect the host's package manager by reading `/etc/os-release` (`$ID` + `$ID_LIKE`).
3. Configure the Datadog package repo on the target:
   - **Debian / Ubuntu**: write `/etc/apt/sources.list.d/datadog.list` (with `signed-by=`), download `keys.datadoghq.com/DATADOG_APT_KEY_*.public` keys, import via `gpg --no-default-keyring`.
   - **RHEL / CentOS / Amazon / Rocky / Alma**: write `/etc/yum.repos.d/datadog.repo` referencing the same key URLs.
4. Install signed packages via the host's package manager: `datadog-agent` (+ `datadog-signing-keys` on apt).
5. Download the `datadog-installer` binary from the OCI registry (`gcr.io/datadoghq/installer-package/blobs/sha256:<pin>`), verify against a pinned SHA-256, then upload it to the remote host.
6. Execute `datadog-installer setup` on the remote host to install `apm-inject` + the language libraries and to write `/etc/ld.so.preload`. (The exact subcommand and flags — `--flavor "APM SSI"`, `--apm-instrumentation-enabled=host`, etc. — need verification with whoever owns `datadog-installer`. See "Open questions" below.)
7. Patch `/etc/datadog-agent/datadog.yaml` with `DD_API_KEY` and `DD_SITE` (sourced from pup's existing auth config).
8. Restart the agent and confirm `datadog-agent status` reports healthy.

The critical property: nowhere in this flow does pup execute a remote shell script. Every step is either (a) an `apt-get` / `yum` install against signed package repos, (b) an HTTP fetch of a key file or binary followed by signature/hash verification, or (c) executing a known binary with known arguments. The `curl | bash` pattern is gone.

## Security model

- **No fetch-and-execute.** No `curl | bash`, no `bash -c "$(curl ...)"`, no `wget | sh`. All downloads write to a file, then verify, then either parse or execute.
- **Signed packages.** Agent + SSI components install via apt/yum with the Datadog signing keys imported into a keyring; signature verification is enforced by the package manager.
- **Pinned binary.** The `datadog-installer` binary is downloaded by SHA-256 (pinned in pup's source) rather than "whatever the latest URL serves." This trades update friction for a real audit trail.
- **Auth surface.** SSH credentials are passed via flags / config (same shape as existing ssh tooling); Datadog API credentials use pup's existing `auth` infrastructure.
- **No new outbound endpoints beyond what the official install script already hits** — `apt.datadoghq.com`, `yum.datadoghq.com`, `keys.datadoghq.com`, `gcr.io` / `public.ecr.aws`.

## Status (as of this PR)

This is a **scaffolding PR**. What's in vs out:

**In:**
- The `pup install` command surface in `src/main.rs` (clap subcommands `Install` → `Ssi` → `Linux`).
- A new `src/commands/install.rs` module with the `ssi_linux` dispatcher.
- `--dry-run` prints the planned install steps for the given target.
- Without `--dry-run`, the command bails with an actionable error that points at this doc.
- Unit tests for both behaviors.

**Out (follow-up PRs):**
- The actual `russh` client session (connect, exec, file transfer).
- The OCI registry manifest fetch + blob download + SHA-256 verification.
- The repo file / keyring setup on the remote host.
- The `datadog-installer setup` invocation.
- The agent restart + health check.
- Integration tests against a real Linux host.
- Multi-distro test matrix (Debian 11/12, Ubuntu 22.04/24.04, RHEL 8/9, Amazon Linux 2/2023, Rocky 9).

The intent is to land the *surface* now so the pup team can review the shape, and to land the implementation in small, reviewable follow-ups.

## Open questions for the pup team

1. **Where should the install module live?** This PR puts everything in `src/commands/install.rs`. Once the implementation lands the install logic might want its own top-level `src/install/` module with submodules (`ssh.rs`, `oci.rs`, `linux_ssi.rs`). Happy to refactor at that point.
2. **SSH transport.** `russh` is already an optional dep but currently used only as a server (`src/tunnel.rs`). Should the install module reuse `russh` as a client, or shell out to the system `ssh` binary? `russh` keeps pup self-contained (especially on Windows); shelling out is simpler but adds a runtime dependency.
3. **`datadog-installer` CLI surface.** The exact subcommand and flags used by `install-ssi.sh` (`setup --flavor "APM SSI"` per one read of the script) need to be confirmed with whoever owns that binary as a public, stable contract. If the contract isn't stable, we need a different integration point.
4. **OCI version pinning.** Who owns bumping the `datadog-installer` SHA-256 in pup when Datadog ships a new release? Manual on each agent release, or auto-bumped from a manifest?
5. **k8s parity.** `pup install ssi k8s` is the natural next leaf. The mechanics are entirely different (helm + DatadogAgent CR + admission webhook) — does it deserve its own command tree, or stay under `install ssi`?
6. **Auth integration.** Pup's existing `auth` config gives access to Datadog API. The install flow needs `DD_API_KEY` + `DD_SITE` to write `/etc/datadog-agent/datadog.yaml` — should the install command reuse `cfg.api_key` / `cfg.site`, or take its own flags?
