# HTTP proxy options for Pup

## Recommendation

Support standard HTTP forward proxies for native HTTP API traffic through
`HTTPS_PROXY`, `HTTP_PROXY`, and `NO_PROXY`. Keep `DD_SITE` as the destination
site. Use a CONNECT-capable proxy such as Squid for network routing, or
mitmproxy when developers need to inspect, modify, or replay HTTPS requests.
Keep local mock servers for deterministic debug/unit tests.

Start by testing and documenting the existing reqwest environment-proxy
behavior. Add explicit proxy flags/configuration only if there is a concrete
need to override the process environment. This avoids adding another endpoint
override to release binaries.

This document is a proposal; no new proxy implementation is included in this
change. Repository inspection establishes the client paths below. Proxy
interoperability and third-party proxy tools have not been exercised in this
sandbox, and the upstream reference pages have not been fetched here.

## What is present today

- `Cargo.lock` pins reqwest 0.13.2, and `Cargo.toml` leaves its default features
  enabled. Reqwest provides environment/system proxy discovery and an explicit
  `Proxy` API. `docs/TROUBLESHOOTING.md` already documents `HTTP_PROXY` and
  `HTTPS_PROXY`; the proposal is to verify and define the supported behavior.
- SDK calls build a reqwest middleware client in `src/client.rs`. Raw HTTP
  calls in `src/raw_client.rs` and OAuth registration/token requests in
  `src/auth/dcr.rs` construct their own reqwest clients. Several commands also
  construct clients directly. None of those constructors explicitly calls
  `proxy()` or `no_proxy()` today.
- `DD_SITE` / `--site` supports literal HTTPS gateway hosts, including an
  explicit trust check before sending credentials to a non-Datadog host.
  That changes the destination host and OAuth host selection.
- `PUP_MOCK_SERVER` changes the resolved API host/base URL for SDK and raw
  calls. It does not replace `DcrClient`'s independently resolved OAuth hosts.
  It is now compiled only into debug and unit-test builds, outside browser
  builds. Standard release executables contain neither environment lookup.
- The Synthetics tunnel uses `tokio_tungstenite::connect_async` and direct TCP
  sockets in `src/tunnel.rs`. Reqwest proxy settings do not configure these.
  Browser and WASI networking also require separate support decisions.

## Options and tradeoffs

| Option | Strengths | Costs and limits | Suggested role |
| --- | --- | --- | --- |
| Standard forward proxy via environment variables | Preserves Datadog URLs, TLS hostname checks, region selection, and OAuth storage keys; uses the existing HTTP dependency; compatible with Squid and corporate proxies | Must validate all HTTP client paths, proxy authentication, bypass rules, and OS defaults; a normal CONNECT tunnel cannot inspect or synthesize HTTPS responses | Recommended initial native HTTP support |
| Explicit `--proxy` / config through `reqwest::Proxy` | Predictable per-invocation selection; can add a direct-connection option | Requires shared configuration across SDK, raw, OAuth, and command-specific clients; precedence, bypass behavior, and credential redaction become Pup responsibilities | Follow-up only when environment variables are insufficient |
| mitmproxy / mitmdump as an external forward proxy | Interactive inspection, scripted responses, fault injection, and saved-flow replay while keeping real destination URLs | HTTPS inspection requires trusting a local CA; recordings can contain credentials; replay matching and unmatched-request handling need configuration | Recommended developer inspection/replay tool |
| Squid as an external forward proxy | Established CONNECT proxy for controlled egress, authentication, and destination ACLs; ordinary tunneling preserves end-to-end TLS | Separate service/configuration; ordinary CONNECT cannot inspect encrypted API bodies or replay fixtures; authentication compatibility needs testing | Production/enterprise proxy reference |
| Reverse proxy/API gateway via `DD_SITE` (for example, nginx or Envoy) | Fits the existing HTTPS custom-host/trust model; can centralize routing and policy | Changes the logical destination and OAuth host/storage identity; gateway must handle API and OAuth routes, certificates, redirects, and regional routing | Retain for managed gateways, not the default outbound-proxy interface |
| Existing local mock override | Fast deterministic fixtures, no proxy service or local CA needed | Changes API URLs, does not cover the complete OAuth flow, and is unavailable in release executables | Retain for debug/unit tests |

Squid and mitmproxy serve different needs: choose Squid for forwarding and
access control, and mitmproxy for developer visibility and response control.
Neither needs to be embedded in Pup. SOCKS would require a separate feature
and compatibility review; it is outside this initial HTTP proxy plan.

## Proposed support contract

For native reqwest-based traffic, begin with:

```bash
HTTPS_PROXY=http://127.0.0.1:8080 \
HTTP_PROXY=http://127.0.0.1:8080 \
NO_PROXY=localhost,127.0.0.1 \
DD_SITE=datadoghq.com \
pup monitors list
```

For an HTTPS destination, `HTTPS_PROXY=http://...` means Pup reaches the proxy
over HTTP and asks it to CONNECT to the destination, then performs TLS to the
Datadog host through that tunnel. TLS inspection is not needed for forwarding.
The destination remains `https://api.datadoghq.com`, including its TLS name.
`HTTP_PROXY` alone does not select a proxy for an HTTPS destination.

Verify `ALL_PROXY`, lowercase variants, environment/OS precedence, and the
exact `NO_PROXY` matching rules with the pinned reqwest version before making
them part of the support contract. Do not implement a second environment
parser or assume curl's matching and precedence rules are identical.

For HTTPS inspection/replay, run mitmdump bound to loopback, point
`HTTPS_PROXY` at it, and install its CA in an isolated developer trust store
using the platform trust configuration described in `TROUBLESHOOTING.md`.
Use fixture credentials and scrub API keys, app keys, bearer/refresh tokens,
and proxy credentials from saved flows. Configure replay to reject unmatched
requests so a fixture run cannot fall through to the live API. Keep TLS
verification enabled.

Support scope must explicitly exclude Synthetics WebSocket/TCP tunnels and
browser-managed networking initially. Validate WASI runtime capabilities
separately before promising proxy support there. The browser launched for
interactive OAuth authorization has its own proxy/trust configuration;
Pup's environment only configures its own HTTP requests.

## Implementation and validation plan

1. **Prove the existing behavior.** Add subprocess-based tests using only
   loopback fixtures and a small CONNECT-capable test proxy. Set proxy
   variables on each child rather than mutating the test runner environment.
   Test a release binary as well as a debug binary, without using
   `PUP_MOCK_SERVER` for the release case. Capture CONNECT destinations and
   confirm the URL/Host/TLS name remain those of the selected site. Reject
   destinations outside the local fixture allowlist; use a test-scoped CA
   and route only to the local TLS fixture, with no public Datadog requests.
2. **Cover each client family.** Exercise an SDK operation, raw API command,
   OAuth registration/token exchange, and command-specific reqwest builders.
   Cover API+app-key auth and bearer auth, a regional site, pagination, and
   redirects. Preserve existing commands that intentionally disable redirects.
   Verify destination credentials reach only the TLS origin and proxy auth
   reaches only the proxy.
3. **Exercise failures and bypasses.** Check proxy connection failure, CONNECT
   rejection, a 407 authentication challenge, malformed configuration, and
   `NO_PROXY` matches/non-matches including loopback. Test precedence with a
   sanitized child environment. Fail explicitly when the selected proxy is
   unavailable; do not silently retry directly. Assert errors omit proxy
   userinfo and credentials. Test trusted and untrusted TLS chains.
4. **Publish the native HTTP contract.** If all constructors honor the same
   settings, documentation/tests may be sufficient. Fix inconsistent paths
   through a small shared builder as needed. Add a Squid CONNECT smoke test
   and a separate mitmproxy inspection/replay recipe; do not require either
   external tool for ordinary unit tests. Run compatibility checks on Linux,
   macOS, and Windows before claiming cross-platform support.
5. **Consider explicit controls later.** If users need `--proxy`, apply flag
   > Pup environment setting > config > reqwest's standard discovery. An
   explicit direct-connection option would call `no_proxy()`. Feed the same
   settings into SDK, raw, OAuth, and command-specific clients; define bypass
   behavior explicitly because adding a reqwest `Proxy` disables automatic
   system-proxy discovery. Require tests for precedence, bypasses, and secret
   redaction before introducing those controls.

The first milestone is verified standard CONNECT forwarding for native HTTP
calls in release binaries. Inspection/replay is an optional development
workflow. Explicit flags, WASI, and tunnel proxying are separate follow-ups.

## Upstream references

- [Reqwest 0.13.2 proxy configuration](https://docs.rs/reqwest/0.13.2/reqwest/struct.Proxy.html)
- [Reqwest 0.13.2 ClientBuilder](https://docs.rs/reqwest/0.13.2/reqwest/struct.ClientBuilder.html)
- [mitmproxy proxy modes](https://docs.mitmproxy.org/stable/concepts/modes/)
- [mitmproxy replay features](https://docs.mitmproxy.org/stable/overview/features/)
- [mitmproxy certificate setup](https://docs.mitmproxy.org/stable/concepts/certificates/)
- [Squid HTTPS/CONNECT guidance](https://wiki.squid-cache.org/Features/HTTPS)
