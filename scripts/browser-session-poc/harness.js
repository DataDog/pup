// pup browser session-auth PoC.
//
// From DevTools on a logged-in https://app.{site} tab (with serve.py running):
//   const { results } = await import('http://127.0.0.1:8787/harness.js?' + Date.now());
//
// Read-only except POST /api/v1/monitor/validate, which validates a monitor
// definition without creating anything.

import init, { PupClient, PupClientOptions } from './pkg/pup_wasm.js';

// Same lookup as web-ui's getTokenFromDom (authentication/lib/csrf/csrf-token.ts).
function csrfFromDom() {
    for (const el of document.querySelectorAll('#_authentication_token')) {
        if (el.value) return el.value;
    }
    return null;
}

function client(csrfToken) {
    const opts = new PupClientOptions(location.hostname);
    opts.session_origin = location.origin;
    opts.csrf_token = csrfToken ?? undefined;
    return new PupClient(opts);
}

function summarize(value) {
    const json = JSON.stringify(value) ?? '';
    return json.length > 160 ? `${json.slice(0, 160)}…` : json;
}

async function step(name, expected, fn) {
    try {
        const value = await fn();
        return { step: name, expected: expected.join('|'), status: 200, pass: expected.includes(200), detail: summarize(value) };
    } catch (err) {
        const message = String(err?.message ?? err);
        const status = Number(/HTTP (\d{3})/.exec(message)?.[1]) || null;
        return { step: name, expected: expected.join('|'), status, pass: expected.includes(status), detail: message.slice(0, 200) };
    }
}

const VALIDATE_BODY = JSON.stringify({
    type: 'metric alert',
    name: 'pup browser PoC (validate only)',
    query: 'avg(last_5m):avg:system.cpu.user{*} > 90',
    message: 'pup browser PoC',
    options: { thresholds: { critical: 90 } },
});

const LOGS_BODY = JSON.stringify({
    filter: { query: '*', from: 'now-15m', to: 'now' },
    page: { limit: 1 },
});

const csrf = csrfFromDom();
const page = await fetch(location.href, { credentials: 'same-origin' });
const env = {
    origin: location.origin,
    csrfTokenFound: Boolean(csrf),
    crossOriginIsolated: self.crossOriginIsolated,
    csp: page.headers.get('content-security-policy'),
    cspReportOnly: page.headers.get('content-security-policy-report-only'),
};
console.log('[pup-poc] environment', env);

const results = [];
results.push(await step('compile + instantiate wasm', [200], async () => {
    await init();
    return { version: client(csrf).version() };
}));

if (results[0].pass) {
    const withCsrf = client(csrf);
    const withoutCsrf = client(null);
    results.push(await step('GET /api/v1/monitor (cookie only)', [200], () => withCsrf.monitors_list(null, null, 5n)));
    results.push(await step('POST /api/v1/monitor/validate + x-csrf-token', [200, 400], () => withCsrf.raw_post('/api/v1/monitor/validate', VALIDATE_BODY)));
    results.push(await step('POST /api/v1/monitor/validate, no CSRF token', [401, 403], () => withoutCsrf.raw_post('/api/v1/monitor/validate', VALIDATE_BODY)));
    results.push(await step('POST /api/v2/logs/events/search (expected blocker)', [401, 403], () => withCsrf.logs_search(LOGS_BODY)));
}

console.table(results);
export { env, results };
