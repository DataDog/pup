# Browser session-auth PoC

Runs pup's `browser` WASM build inside a logged-in Datadog app tab, authenticating
with the page's `dogweb` session cookie (no API keys, no bearer token). Writes add
the page's CSRF token as `x-csrf-token`.

## Run

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.114 --locked
scripts/browser-session-poc/build.sh
python3 -I scripts/browser-session-poc/serve.py
```

Then, in DevTools on a logged-in `https://app.{site}` tab:

```js
const { env, results } = await import('http://127.0.0.1:8787/harness.js?' + Date.now());
```

Chrome may ask to allow the page to access devices on your local network; allow it
for this test. The harness logs the page's CSP header and a results table:

| Step | Expected |
|---|---|
| compile + instantiate wasm | 200 (fails if CSP blocks WebAssembly) |
| `GET /api/v1/monitor` | 200 |
| `POST /api/v1/monitor/validate` with CSRF | 200 or 400 (reached the handler) |
| same, without CSRF | 401/403 |
| `POST /api/v2/logs/events/search` | 401/403 (route does not accept session auth) |

The only write-method call is monitor validation, which creates nothing.

## OAuth exchange

Routes that reject session auth (e.g. logs search) accept OAuth tokens, which the
page can obtain from the session:

```js
const { results } = await import('http://127.0.0.1:8787/oauth.js?' + Date.now());
```

Click the "Authorize pup PoC" button it adds. It registers a public client with an
app-domain redirect URI (cached in localStorage), authorizes with PKCE in a popup,
exchanges the code, calls logs search and monitors with the Bearer token, then
revokes the token. The registered `pup-browser-poc` client is not deleted.

`smoke.html` checks the request shape locally with `fetch` stubbed: open
`http://localhost:8787/smoke.html` and read `window.smoke`.
