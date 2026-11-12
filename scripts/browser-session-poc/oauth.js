// pup browser OAuth PoC: exchange the logged-in session for an OAuth token.
//
// From DevTools on a logged-in https://app.{site} tab (with serve.py running):
//   const { results } = await import('http://127.0.0.1:8787/oauth.js?' + Date.now());
// then click the "Authorize pup PoC" button it adds (popups need a click).
//
// Flow: register a public client with an app-domain redirect URI (RFC 7591),
// authorize with PKCE in a popup (authenticated by the session cookie), exchange
// the code, call logs search with the Bearer token, then revoke the token.
// The token is never logged.

import init, { PupClient, PupClientOptions } from './pkg/pup_wasm.js';

const SCOPES = 'logs_read_data monitors_read';
const REDIRECT_URI = `${location.origin}/pup-poc/oauth-callback`;
const CLIENT_CACHE_KEY = `pup-poc-oauth-client:${REDIRECT_URI}`;

const results = [];
const record = (step, pass, status, detail) => {
    results.push({ step, pass, status, detail: String(detail ?? '').slice(0, 240) });
};

function b64url(bytes) {
    return btoa(String.fromCharCode(...new Uint8Array(bytes)))
        .replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

const randomString = () => b64url(crypto.getRandomValues(new Uint8Array(32)));

async function pkce() {
    const verifier = randomString();
    const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(verifier));
    return { verifier, challenge: b64url(digest) };
}

function cachedClientId() {
    try { return localStorage.getItem(CLIENT_CACHE_KEY); } catch { return null; }
}

function cacheClientId(id) {
    try { localStorage.setItem(CLIENT_CACHE_KEY, id); } catch { /* ignore */ }
}

async function registerClient() {
    const cached = cachedClientId();
    if (cached) {
        record('register client (cached)', true, null, `client_id ${cached}`);
        return cached;
    }
    const resp = await fetch(`${location.origin}/api/v2/oauth2/register`, {
        method: 'POST',
        credentials: 'omit',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
            client_name: 'pup-browser-poc',
            redirect_uris: [REDIRECT_URI],
            grant_types: ['authorization_code', 'refresh_token'],
        }),
    });
    const text = await resp.text();
    if (resp.status !== 201) {
        record('register client with app-domain redirect', false, resp.status, text);
        return null;
    }
    const body = JSON.parse(text);
    record('register client with app-domain redirect', true, resp.status,
        `client_id ${body.client_id}, redirect_uris ${JSON.stringify(body.redirect_uris)}`);
    cacheClientId(body.client_id);
    return body.client_id;
}

// Popups opened without a user gesture are blocked, so wait for a click.
function waitForClick(label) {
    return new Promise((resolve) => {
        const button = document.createElement('button');
        button.textContent = label;
        Object.assign(button.style, {
            position: 'fixed', top: '16px', right: '16px', zIndex: 2147483647,
            padding: '12px 16px', font: '600 14px system-ui', cursor: 'pointer',
            background: '#632ca6', color: '#fff', border: 0, borderRadius: '6px',
        });
        button.addEventListener('click', () => { button.remove(); resolve(); }, { once: true });
        document.body.append(button);
    });
}

async function authorize(clientId, challenge, state) {
    const params = new URLSearchParams({
        response_type: 'code',
        client_id: clientId,
        redirect_uri: REDIRECT_URI,
        state,
        scope: SCOPES,
        code_challenge: challenge,
        code_challenge_method: 'S256',
    });
    await waitForClick('Authorize pup PoC');
    const startedAt = performance.now();
    const popup = window.open(`${location.origin}/oauth2/v1/authorize?${params}`, 'pup-poc-oauth', 'width=640,height=760');
    if (!popup) {
        record('authorize (popup)', false, null, 'popup blocked');
        return null;
    }
    let sawConsentPage = false;
    for (let i = 0; i < 1200; i++) {
        await new Promise((r) => setTimeout(r, 150));
        if (popup.closed) {
            record('authorize (popup)', false, null, 'popup closed before redirect');
            return null;
        }
        let href;
        try { href = popup.location.href; } catch { continue; }
        if (href.startsWith(REDIRECT_URI)) {
            const url = new URL(href);
            popup.close();
            const secs = ((performance.now() - startedAt) / 1000).toFixed(1);
            if (url.searchParams.get('state') !== state) {
                record('authorize (popup)', false, null, 'state mismatch');
                return null;
            }
            const error = url.searchParams.get('error');
            if (error) {
                record('authorize (popup)', false, null, `${error}: ${url.searchParams.get('error_description') ?? ''}`);
                return null;
            }
            record('authorize (popup)', true, null,
                `code received after ${secs}s${sawConsentPage ? ', consent page shown' : ', no consent page'}`);
            return url.searchParams.get('code');
        }
        if (href.includes('/oauth2/v1/authorize') && popup.document?.readyState === 'complete') {
            sawConsentPage = true;
        }
    }
    popup.close();
    record('authorize (popup)', false, null, 'timed out after 3 minutes');
    return null;
}

async function exchange(clientId, code, verifier) {
    const form = new URLSearchParams({
        grant_type: 'authorization_code',
        client_id: clientId,
        code,
        redirect_uri: REDIRECT_URI,
        code_verifier: verifier,
    });
    for (const path of ['/api/v2/oauth2/token', '/oauth2/v1/token']) {
        const resp = await fetch(`${location.origin}${path}`, { method: 'POST', credentials: 'omit', body: form });
        const text = await resp.text();
        if (resp.ok) {
            const body = JSON.parse(text);
            record(`exchange code (${path})`, true, resp.status,
                `token_type ${body.token_type}, expires_in ${body.expires_in}, refresh_token ${Boolean(body.refresh_token)}, scope "${body.scope}"`);
            return body.access_token;
        }
        record(`exchange code (${path})`, false, resp.status, text);
        if (resp.status !== 404) return null;
    }
    return null;
}

async function call(step, fn) {
    try {
        const value = await fn();
        const json = JSON.stringify(value) ?? '';
        record(step, true, 200, json.length > 160 ? `${json.slice(0, 160)}…` : json);
    } catch (err) {
        const message = String(err?.message ?? err);
        record(step, false, Number(/HTTP (\d{3})/.exec(message)?.[1]) || null, message);
    }
}

async function revoke(clientId, token) {
    const resp = await fetch(`${location.origin}/oauth2/v1/revoke`, {
        method: 'POST',
        credentials: 'omit',
        headers: { authorization: `Bearer ${token}` },
        body: new URLSearchParams({ token, client_id: clientId }),
    });
    record('revoke token (cleanup)', resp.ok, resp.status, await resp.text());
}

await init();
const clientId = await registerClient();
if (clientId) {
    const { verifier, challenge } = await pkce();
    const state = randomString();
    const code = await authorize(clientId, challenge, state);
    const token = code && await exchange(clientId, code, verifier);
    if (token) {
        const opts = new PupClientOptions(location.hostname);
        opts.session_origin = location.origin;
        opts.access_token = token;
        const pup = new PupClient(opts);
        await call('POST /api/v2/logs/events/search (Bearer)', () => pup.logs_search(JSON.stringify({
            filter: { query: '*', from: 'now-15m', to: 'now' },
            page: { limit: 1 },
        })));
        await call('GET /api/v1/monitor (Bearer)', () => pup.monitors_list(null, null, 5n));
        await revoke(clientId, token);
    }
}

console.table(results);
export { results };
