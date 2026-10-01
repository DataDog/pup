'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const {execFileSync} = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const {parseCli, buildPlatformPackage, buildRootPackage} = require('./build-packages.js');
const {PLATFORMS, platformPackageName} = require('../pup/lib/platforms.js');

function tempDir() {
	return fs.mkdtempSync(path.join(os.tmpdir(), 'pup-npm-test-'));
}

function fakeReleaseArchive(assets, version, key) {
	const staging = tempDir();
	fs.writeFileSync(path.join(staging, PLATFORMS[key].binary), '#!/bin/sh\necho fake\n');
	execFileSync('tar', [
		'-czf',
		path.join(assets, `pup_${version}_${PLATFORMS[key].asset}`),
		'-C',
		staging,
		PLATFORMS[key].binary,
	]);
	fs.rmSync(staging, {recursive: true, force: true});
}

test('builds a platform package from a release archive', () => {
	const assets = tempDir();
	const out = tempDir();
	fakeReleaseArchive(assets, '1.2.3', 'linux-x64');

	const packageDir = buildPlatformPackage({key: 'linux-x64', version: '1.2.3', assets, out});

	const manifest = JSON.parse(fs.readFileSync(path.join(packageDir, 'package.json'), 'utf8'));
	assert.equal(manifest.name, '@datadog/pup-linux-x64');
	assert.equal(manifest.version, '1.2.3');
	assert.deepEqual(manifest.os, ['linux']);
	assert.deepEqual(manifest.cpu, ['x64']);
	const binary = path.join(packageDir, 'bin', 'pup');
	assert.equal(fs.readFileSync(binary, 'utf8'), '#!/bin/sh\necho fake\n');
	assert.equal(fs.statSync(binary).mode & 0o111, 0o111);
});

test('stamps the version into the root package and every optional dependency', () => {
	const out = tempDir();

	const packageDir = buildRootPackage({version: '1.2.3', out});

	const manifest = JSON.parse(fs.readFileSync(path.join(packageDir, 'package.json'), 'utf8'));
	assert.equal(manifest.version, '1.2.3');
	assert.deepEqual(
		manifest.optionalDependencies,
		Object.fromEntries(Object.keys(PLATFORMS).map(key => [platformPackageName(key), '1.2.3'])),
	);
	assert.ok(fs.existsSync(path.join(packageDir, 'bin', 'pup.js')));
	assert.ok(fs.existsSync(path.join(packageDir, 'lib', 'platforms.js')));
});

test('committed root manifest lists exactly the supported platforms', () => {
	const manifest = require('../pup/package.json');
	assert.deepEqual(
		Object.keys(manifest.optionalDependencies).sort(),
		Object.keys(PLATFORMS).map(platformPackageName).sort(),
	);
});

test('fails when the release archive is missing', () => {
	assert.throws(
		() => buildPlatformPackage({key: 'darwin-arm64', version: '1.2.3', assets: tempDir(), out: tempDir()}),
		/Missing release archive .*pup_1\.2\.3_Darwin_arm64\.tar\.gz/,
	);
});

test('rejects invalid versions, unknown platforms, and missing directories', () => {
	assert.throws(() => parseCli(['--version', 'v1.2.3', '--assets', 'a', '--out', 'b']), /semver version/);
	assert.throws(() => parseCli(['--version', '1.2.3']), /--assets and --out are required/);
	assert.throws(
		() => parseCli(['--version', '1.2.3', '--assets', 'a', '--out', 'b', '--platform', 'linux-riscv64']),
		/Unknown platform linux-riscv64/,
	);
	assert.deepEqual(parseCli(['--version', '1.2.3', '--assets', 'a', '--out', 'b']).platforms, Object.keys(PLATFORMS));
});
