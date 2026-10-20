'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const {PLATFORMS, platformPackageName, resolveBinaryPath} = require('../lib/platforms.js');

test('resolves the platform package binary for every supported platform', () => {
	for (const [key, entry] of Object.entries(PLATFORMS)) {
		const [platform, arch] = key.split('-');
		const requested = [];
		const resolved = resolveBinaryPath({
			platform,
			arch,
			resolve: id => {
				requested.push(id);
				return `/node_modules/${id}`;
			},
		});
		assert.deepEqual(requested, [`${platformPackageName(key)}/bin/${entry.binary}`]);
		assert.equal(resolved, `/node_modules/${platformPackageName(key)}/bin/${entry.binary}`);
	}
});

test('uses pup.exe only on Windows', () => {
	assert.equal(PLATFORMS['win32-x64'].binary, 'pup.exe');
	for (const [key, entry] of Object.entries(PLATFORMS)) {
		if (key !== 'win32-x64') {
			assert.equal(entry.binary, 'pup');
		}
	}
});

test('rejects platforms pup does not release', () => {
	for (const [platform, arch] of [
		['win32', 'arm64'],
		['linux', 'ia32'],
		['freebsd', 'x64'],
	]) {
		assert.throws(
			() => resolveBinaryPath({platform, arch, resolve: () => assert.fail('must not resolve')}),
			new RegExp(`does not publish a binary for ${platform}-${arch}`),
		);
	}
});

test('explains a missing optional dependency', () => {
	assert.throws(
		() =>
			resolveBinaryPath({
				platform: 'linux',
				arch: 'x64',
				resolve: () => {
					throw new Error('Cannot find module');
				},
			}),
		/@datadog\/pup-linux-x64 package is not installed.*--omit=optional/,
	);
});
