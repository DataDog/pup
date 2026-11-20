import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import test from 'node:test';
import {fileURLToPath} from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const pkgDir = path.resolve(__dirname, '..', 'pkg');

const wasmMod = await import(path.join(pkgDir, 'pup_wasm.js'));
const wasmBytes = await fs.readFile(path.join(pkgDir, 'pup_wasm_bg.wasm'));
await wasmMod.default({module_or_path: wasmBytes});

const {PupClient, PupClientOptions, run_unauthenticated_cli} = wasmMod;

function withMockFetch(handler, fn) {
	const originalFetch = globalThis.fetch;
	globalThis.fetch = async (input, init) => {
		const req = input instanceof Request ? input : new Request(input, init);
		const resp = await handler(req);
		if (!resp.url) {
			Object.defineProperty(resp, 'url', {value: req.url});
		}
		return resp;
	};
	return Promise.resolve()
		.then(fn)
		.finally(() => {
			globalThis.fetch = originalFetch;
		});
}

function jsonResponse(body, status = 200, headers = {}) {
	return new Response(JSON.stringify(body), {
		status,
		headers: {'content-type': 'application/json', ...headers},
	});
}

function makeSessionClient(csrfToken = 'csrf-secret-123') {
	const opts = new PupClientOptions('datadoghq.com');
	opts.session_origin = 'https://app.datadoghq.com';
	opts.csrf_token = csrfToken;
	return new PupClient(opts);
}

test('missing subcommands report usage error on stderr with exit code 2, while --help exits 0', async () => {
	const missing = await run_unauthenticated_cli('datadoghq.com', ['monitors'], undefined, undefined);
	assert.equal(missing.exit_code, 2);
	assert.equal(missing.stdout, '');
	assert.match(missing.stderr, /Usage:/);

	const help = await run_unauthenticated_cli('datadoghq.com', ['monitors', '--help'], undefined, undefined);
	assert.equal(help.exit_code, 0);
	assert.equal(help.stderr, '');
	assert.match(help.stdout, /Usage:/);

	const emptyArgs = await run_unauthenticated_cli('datadoghq.com', [], undefined, undefined);
	assert.equal(emptyArgs.exit_code, 0);
	assert.match(emptyArgs.stdout, /Usage:/);
});

test('auth status succeeds without native threads for unauthenticated and session clients', async () => {
	const unauth = await run_unauthenticated_cli('datadoghq.com', ['auth', 'status'], undefined, undefined);
	assert.equal(unauth.exit_code, 0, unauth.stderr);
	const unauthJson = JSON.parse(unauth.stdout);
	assert.equal(unauthJson.authenticated, false);
	assert.equal(unauthJson.status, 'no token');

	const client = makeSessionClient();
	const sessionStatus = await client.run_cli(['auth', 'status'], undefined, undefined);
	assert.equal(sessionStatus.exit_code, 0, sessionStatus.stderr);
	const sessionJson = JSON.parse(sessionStatus.stdout);
	assert.equal(sessionJson.authenticated, true);
	assert.equal(sessionJson.auth_method, 'browser_session');
});

test('overlapping CLI invocations isolate stdout, raw-output mode, and command metadata', async () => {
	const client = makeSessionClient();
	await withMockFetch(
		async req => {
			if (new URL(req.url).pathname === '/api/v1/monitor') {
				await new Promise(resolve => setTimeout(resolve, 40));
				return jsonResponse([
					{
						id: 42,
						name: 'CPU high',
						query: 'avg(last_5m):avg:system.cpu.user{*} > 90',
						type: 'metric alert',
					},
				]);
			}
			throw new Error(`unexpected URL: ${req.url}`);
		},
		async () => {
			const [monitorsRes, versionRes] = await Promise.all([
				client.run_cli(['monitors', 'list', '--no-envelope'], undefined, undefined),
				run_unauthenticated_cli('datadoghq.com', ['version'], undefined, undefined),
			]);

			assert.equal(monitorsRes.exit_code, 0, monitorsRes.stderr);
			const parsedMonitors = JSON.parse(monitorsRes.stdout);
			assert.ok(Array.isArray(parsedMonitors), `expected raw array without envelope, got: ${monitorsRes.stdout}`);
			assert.equal(parsedMonitors[0].id, 42);

			assert.equal(versionRes.exit_code, 0, versionRes.stderr);
			assert.match(versionRes.stdout, /^Pup /);
			assert.doesNotMatch(versionRes.stdout, /CPU high/);
		},
	);
});

test('cookie-session auth applies to incidents attachments delete and debugger probes create', async () => {
	const client = makeSessionClient('csrf-tok-xyz');
	const seen = [];

	await withMockFetch(
		async req => {
			seen.push({
				method: req.method,
				url: req.url,
				credentials: req.credentials,
				csrf: req.headers.get('x-csrf-token'),
			});
			const {pathname} = new URL(req.url);
			if (req.method === 'DELETE' && pathname === '/api/v2/incidents/inc-1/attachments/att-9') {
				return new Response(null, {status: 204});
			}
			if (req.method === 'POST' && pathname === '/api/ui/debugger/parse-expression') {
				return jsonResponse({
					data: {attributes: {dsl: 'x > 0', json: {gt: [{ref: 'x'}, 0]}}},
				});
			}
			if (req.method === 'POST' && pathname === '/api/ui/debugger/parse-template') {
				return jsonResponse({
					data: {attributes: {segments: [{str: 'hi'}]}},
				});
			}
			if (req.method === 'POST' && pathname === '/api/ui/debugger/live-debugger/probes/log/') {
				return jsonResponse({data: {id: 'probe-1', attributes: {}}});
			}
			throw new Error(`unexpected ${req.method} ${req.url}`);
		},
		async () => {
			const delRes = await client.run_cli(
				['incidents', 'attachments', 'delete', 'inc-1', 'att-9', '--yes'],
				undefined,
				undefined,
			);
			assert.equal(delRes.exit_code, 0, delRes.stderr);
			assert.match(delRes.stdout, /att-9/);

			const probeRes = await client.run_cli(
				[
					'debugger',
					'probes',
					'create',
					'--service',
					'web-store',
					'--env',
					'prod',
					'--probe-location',
					'OrderService:checkout',
					'--language',
					'java',
					'--condition',
					'x > 0',
					'--template',
					'hi',
					'--yes',
				],
				undefined,
				undefined,
			);
			assert.equal(probeRes.exit_code, 0, probeRes.stderr);
			assert.equal(seen.length, 4);
			for (const call of seen) {
				assert.equal(call.credentials, 'same-origin', `missing same-origin credentials on ${call.url}`);
				assert.equal(call.csrf, 'csrf-tok-xyz', `missing x-csrf-token on ${call.url}`);
			}
		},
	);
});

test('virtual files_json and stdin work for dashboards widgets, idp register, ddsql polling, and workflows --wait', async () => {
	const client = makeSessionClient();
	let ddsqlPolls = 0;
	let workflowPolls = 0;

	await withMockFetch(
		async req => {
			const {pathname} = new URL(req.url);
			if (req.method === 'GET' && pathname === '/api/v1/dashboard/abc-123') {
				return jsonResponse({
					id: 'abc-123',
					title: 'Demo',
					layout_type: 'ordered',
					widgets: [],
				});
			}
			if (req.method === 'PUT' && pathname === '/api/v1/dashboard/abc-123') {
				const body = await req.json();
				return jsonResponse(body);
			}
			if (req.method === 'POST' && pathname === '/api/v2/catalog/entity') {
				return jsonResponse({
					data: [{attributes: {kind: 'service', name: 'checkout', namespace: 'default'}}],
				});
			}
			if (req.method === 'POST' && pathname === '/api/v2/ddsql/query/tabular') {
				const body = await req.json();
				assert.equal(body.data.attributes.query, 'SELECT 1');
				return jsonResponse({data: {attributes: {state: 'running', query_id: 'q-1'}}});
			}
			if (req.method === 'POST' && pathname === '/api/v2/ddsql/query/tabular/fetch') {
				ddsqlPolls += 1;
				return jsonResponse({
					data: {
						attributes: {
							state: 'completed',
							columns: [{name: 'one', type: 'int64', values: [1]}],
						},
					},
				});
			}
			if (req.method === 'POST' && pathname === '/api/v2/workflows/wf-1/instances') {
				const body = await req.json();
				assert.equal(body.meta.payload.env, 'staging');
				return jsonResponse({data: {id: 'inst-1'}});
			}
			if (req.method === 'GET' && pathname === '/api/v2/workflows/wf-1/instances/inst-1') {
				workflowPolls += 1;
				return jsonResponse({
					data: {
						id: 'inst-1',
						attributes: {instanceStatus: {detailsKind: 'COMPLETED'}},
					},
				});
			}
			if (req.method === 'POST' && pathname === '/api/v2/workflows/wf-timeout/instances') {
				return jsonResponse({data: {id: 'inst-slow'}});
			}
			if (req.method === 'GET' && pathname === '/api/v2/workflows/wf-timeout/instances/inst-slow') {
				return jsonResponse({
					data: {
						id: 'inst-slow',
						attributes: {instanceStatus: {detailsKind: 'IN_PROGRESS'}},
					},
				});
			}
			throw new Error(`unexpected ${req.method} ${req.url}`);
		},
		async () => {
			const widgetJson = JSON.stringify({
				definition: {type: 'note', content: 'hello from virtual file'},
			});
			const dashRes = await client.run_cli(
				['dashboards', 'widgets', 'add', 'abc-123', '--file', 'widget.json', '--yes', '--no-envelope'],
				undefined,
				JSON.stringify({'widget.json': widgetJson}),
			);
			assert.equal(dashRes.exit_code, 0, dashRes.stderr);
			assert.equal(JSON.parse(dashRes.stdout).widgets.length, 1);

			const missingDashRes = await client.run_cli(
				['dashboards', 'widgets', 'add', 'abc-123', '--file', 'missing.json', '--yes'],
				undefined,
				undefined,
			);
			assert.notEqual(missingDashRes.exit_code, 0);
			assert.match(missingDashRes.stderr, /no such file in browser shell/);

			const idpRes = await client.run_cli(
				['idp', 'register', 'entity.yaml', '--yes'],
				undefined,
				JSON.stringify({'entity.yaml': 'apiVersion: v3\nkind: service\nmetadata:\n  name: checkout\n'}),
			);
			assert.equal(idpRes.exit_code, 0, idpRes.stderr);

			const migrateRes = await client.run_cli(['idp', 'migrate-schema', '--yes'], undefined, undefined);
			assert.notEqual(migrateRes.exit_code, 0);
			assert.match(migrateRes.stderr, /not available in browser WASM builds/);

			const ddsqlRes = await client.run_cli(
				['ddsql', 'table', '--query', '-', '--no-envelope'],
				'SELECT 1',
				undefined,
			);
			assert.equal(ddsqlRes.exit_code, 0, ddsqlRes.stderr);
			assert.equal(ddsqlPolls, 1);

			const wfRes = await client.run_cli(
				[
					'workflows',
					'run',
					'wf-1',
					'--payload-file',
					'payload.json',
					'--wait',
					'--timeout',
					'10s',
					'--yes',
					'--no-envelope',
				],
				undefined,
				JSON.stringify({'payload.json': JSON.stringify({env: 'staging'})}),
			);
			assert.equal(wfRes.exit_code, 0, wfRes.stderr);
			assert.equal(workflowPolls, 1);

			const wfTimeoutRes = await client.run_cli(
				['workflows', 'run', 'wf-timeout', '--payload', '{"env":"staging"}', '--wait', '--timeout', '1s', '--yes'],
				undefined,
				undefined,
			);
			assert.notEqual(wfTimeoutRes.exit_code, 0);
			assert.match(wfTimeoutRes.stderr, /timed out after 1s waiting for instance inst-slow/);
		},
	);
});

test('files_json base64 values upload binary image bytes unchanged', async () => {
	const client = makeSessionClient();
	const png = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0xff]);
	let uploaded;

	await withMockFetch(
		async req => {
			const {pathname} = new URL(req.url);
			if (req.method === 'POST' && pathname === '/api/ui/images') {
				return jsonResponse({
					data: {attributes: {upload_url: 'https://storage.example/put', img_uuid: 'img-1'}},
				});
			}
			if (req.method === 'PUT' && req.url === 'https://storage.example/put') {
				uploaded = Buffer.from(await req.arrayBuffer());
				return new Response('', {status: 200});
			}
			if (req.method === 'POST' && pathname === '/api/ui/image_status/img-1') {
				return jsonResponse({});
			}
			return jsonResponse({errors: [`unexpected ${req.method} ${req.url}`]}, 500);
		},
		async () => {
			const res = await client.run_cli(
				['notebooks', 'images', 'upload', 'shot.png', '--no-envelope'],
				undefined,
				JSON.stringify({'shot.png': {base64: png.toString('base64')}}),
			);
			assert.equal(res.exit_code, 0, res.stderr);
			assert.deepEqual(uploaded, png);
			assert.equal(JSON.parse(res.stdout).img_uuid, 'img-1');
		},
	);

	await assert.rejects(
		client.run_cli(['notebooks', 'images', 'upload', 'shot.png'], undefined, JSON.stringify({'shot.png': {base64: '%%%'}})),
		/invalid base64 for file "shot.png"/,
	);
});
