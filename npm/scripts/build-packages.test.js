'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const {execFileSync} = require('node:child_process');
const crypto = require('node:crypto');
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
	fs.writeFileSync(path.join(staging, 'LICENSE-3rdparty.csv'), 'Component,Origin,License\n');
	const name = `pup_${version}_${PLATFORMS[key].asset}`;
	const archive = path.join(assets, name);
	execFileSync('tar', ['-czf', archive, '-C', staging, PLATFORMS[key].binary, 'LICENSE-3rdparty.csv']);
	fs.rmSync(staging, {recursive: true, force: true});
	const sha256 = crypto.createHash('sha256').update(fs.readFileSync(archive)).digest('hex');
	return {name, sha256};
}

test('builds a platform package from a release archive', () => {
	const assets = tempDir();
	const out = tempDir();
	const {name, sha256} = fakeReleaseArchive(assets, '1.2.3', 'linux-x64');

	const packageDir = buildPlatformPackage({
		key: 'linux-x64',
		version: '1.2.3',
		assets,
		checksums: new Map([[name, sha256]]),
		out,
	});

	const manifest = JSON.parse(fs.readFileSync(path.join(packageDir, 'package.json'), 'utf8'));
	assert.equal(manifest.name, '@datadog/pup-linux-x64');
	assert.equal(manifest.version, '1.2.3');
	assert.deepEqual(manifest.os, ['linux']);
	assert.deepEqual(manifest.cpu, ['x64']);
	assert.deepEqual(manifest.files, ['bin', 'LICENSE-3rdparty.csv']);
	const binary = path.join(packageDir, 'bin', 'pup');
	assert.equal(fs.readFileSync(binary, 'utf8'), '#!/bin/sh\necho fake\n');
	assert.equal(fs.statSync(binary).mode & 0o111, 0o111);
	assert.equal(fs.readFileSync(path.join(packageDir, 'LICENSE-3rdparty.csv'), 'utf8'), 'Component,Origin,License\n');
});

test('refuses an archive missing from the release checksums', () => {
	const assets = tempDir();
	fakeReleaseArchive(assets, '1.2.3', 'linux-x64');

	assert.throws(
		() => buildPlatformPackage({key: 'linux-x64', version: '1.2.3', assets, checksums: new Map(), out: tempDir()}),
		/pup_1\.2\.3_Linux_x86_64\.tar\.gz is not listed in the release checksums/,
	);
});

test('refuses an archive that does not match its release checksum', () => {
	const assets = tempDir();
	const {name} = fakeReleaseArchive(assets, '1.2.3', 'linux-x64');

	assert.throws(
		() =>
			buildPlatformPackage({
				key: 'linux-x64',
				version: '1.2.3',
				assets,
				checksums: new Map([[name, '0'.repeat(64)]]),
				out: tempDir(),
			}),
		/does not match its release checksum/,
	);
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
		() =>
			buildPlatformPackage({
				key: 'darwin-arm64',
				version: '1.2.3',
				assets: tempDir(),
				checksums: new Map(),
				out: tempDir(),
			}),
		/Missing release archive .*pup_1\.2\.3_Darwin_arm64\.tar\.gz/,
	);
});

test('rejects invalid versions, unknown platforms, and missing directories', () => {
	const checksums = path.join(tempDir(), 'checksums.txt');
	fs.writeFileSync(checksums, `${'a'.repeat(64)}  pup_1.2.3_Linux_x86_64.tar.gz\n`);
	const base = ['--assets', 'a', '--checksums', checksums, '--out', 'b'];

	assert.throws(() => parseCli(['--version', 'v1.2.3', ...base]), /semver version/);
	assert.throws(() => parseCli(['--version', '1.2.3']), /--assets, --checksums and --out are required/);
	assert.throws(
		() => parseCli(['--version', '1.2.3', ...base, '--platform', 'linux-riscv64']),
		/Unknown platform linux-riscv64/,
	);
	const options = parseCli(['--version', '1.2.3', ...base]);
	assert.deepEqual(options.platforms, Object.keys(PLATFORMS));
	assert.equal(options.checksums.get('pup_1.2.3_Linux_x86_64.tar.gz'), 'a'.repeat(64));
});
