'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const {spawn} = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const {PLATFORMS, platformPackageName} = require('../lib/platforms.js');

const LAUNCHER = path.join(__dirname, '..', 'bin', 'pup.js');
const HOST_KEY = `${process.platform}-${process.arch}`;
const skip = process.platform === 'win32' || !PLATFORMS[HOST_KEY] ? 'needs a supported POSIX host' : false;

function installFakeBinary(script) {
	const nodePath = fs.mkdtempSync(path.join(os.tmpdir(), 'pup-launcher-'));
	const binDir = path.join(nodePath, platformPackageName(HOST_KEY), 'bin');
	fs.mkdirSync(binDir, {recursive: true});
	fs.writeFileSync(path.join(binDir, PLATFORMS[HOST_KEY].binary), `#!/bin/sh\n${script}\n`, {mode: 0o755});
	return nodePath;
}

function runLauncher(nodePath, args = []) {
	return spawn(process.execPath, [LAUNCHER, ...args], {
		env: {...process.env, NODE_PATH: nodePath},
		stdio: ['ignore', 'pipe', 'pipe'],
	});
}

function waitForExit(child) {
	return new Promise(resolve => {
		child.on('exit', (code, signal) => resolve({code, signal}));
	});
}

test('passes arguments through and propagates the exit code', {skip}, async () => {
	const nodePath = installFakeBinary('echo "$@"; exit 7');
	const child = runLauncher(nodePath, ['monitors', 'list', '--tags=team:a b']);
	let stdout = '';
	child.stdout.on('data', chunk => {
		stdout += chunk;
	});

	assert.deepEqual(await waitForExit(child), {code: 7, signal: null});
	assert.equal(stdout, 'monitors list --tags=team:a b\n');
});

test('forwards SIGTERM to the binary and exits by the same signal', {skip}, async () => {
	const nodePath = installFakeBinary(`trap 'echo got-term > "${'$'}0.term"; exit 143' TERM; echo ready; while :; do sleep 0.1; done`);
	const marker = path.join(nodePath, platformPackageName(HOST_KEY), 'bin', `${PLATFORMS[HOST_KEY].binary}.term`);
	const child = runLauncher(nodePath);
	await new Promise(resolve => {
		child.stdout.once('data', resolve);
	});

	child.kill('SIGTERM');

	const result = await waitForExit(child);
	assert.equal(fs.readFileSync(marker, 'utf8'), 'got-term\n');
	assert.ok(result.code === 143 || result.signal === 'SIGTERM', JSON.stringify(result));
});

test('exits 1 with guidance when the platform package is missing', {skip}, async () => {
	const child = runLauncher(fs.mkdtempSync(path.join(os.tmpdir(), 'pup-launcher-empty-')));
	let stderr = '';
	child.stderr.on('data', chunk => {
		stderr += chunk;
	});

	assert.deepEqual(await waitForExit(child), {code: 1, signal: null});
	assert.match(stderr, /package is not installed/);
});
