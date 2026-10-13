#!/usr/bin/env node
'use strict';

const {execFileSync} = require('node:child_process');
const crypto = require('node:crypto');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const {parseArgs} = require('node:util');
const {PLATFORMS, platformPackageName} = require('../pup/lib/platforms.js');

const REPO_ROOT = path.resolve(__dirname, '..', '..');
const ROOT_PACKAGE_DIR = path.join(REPO_ROOT, 'npm', 'pup');
const VERSION_PATTERN = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/;
const THIRD_PARTY_NOTICES = 'LICENSE-3rdparty.csv';

function parseCli(argv) {
	const {values} = parseArgs({
		args: argv,
		options: {
			version: {type: 'string'},
			assets: {type: 'string'},
			checksums: {type: 'string'},
			out: {type: 'string'},
			platform: {type: 'string', multiple: true},
		},
	});
	if (!values.version || !VERSION_PATTERN.test(values.version)) {
		throw new Error('--version must be a semver version such as 1.23.7');
	}

	if (!values.assets || !values.checksums || !values.out) {
		throw new Error('--assets, --checksums and --out are required');
	}

	const platforms = values.platform ?? Object.keys(PLATFORMS);
	for (const key of platforms) {
		if (!PLATFORMS[key]) {
			throw new Error(`Unknown platform ${key}. Known: ${Object.keys(PLATFORMS).join(', ')}`);
		}
	}

	return {
		version: values.version,
		assets: values.assets,
		checksums: readChecksums(values.checksums),
		out: values.out,
		platforms,
	};
}

function readChecksums(file) {
	const checksums = new Map();
	for (const line of fs.readFileSync(file, 'utf8').split('\n')) {
		const match = /^([0-9a-f]{64}) [ *](.+)$/.exec(line.trim());
		if (match) {
			checksums.set(match[2], match[1]);
		}
	}

	return checksums;
}

function verifyArchive(archive, checksums) {
	const name = path.basename(archive);
	const expected = checksums.get(name);
	if (!expected) {
		throw new Error(`${name} is not listed in the release checksums`);
	}

	const actual = crypto.createHash('sha256').update(fs.readFileSync(archive)).digest('hex');
	if (actual !== expected) {
		throw new Error(`${name} does not match its release checksum`);
	}
}

function extractFiles(archive, destinations) {
	const files = Object.keys(destinations);
	const workDir = fs.mkdtempSync(path.join(os.tmpdir(), 'pup-npm-'));
	try {
		if (archive.endsWith('.zip')) {
			execFileSync('unzip', ['-q', '-o', archive, ...files, '-d', workDir]);
		} else {
			execFileSync('tar', ['-xzf', archive, '-C', workDir, ...files]);
		}

		for (const [file, destination] of Object.entries(destinations)) {
			fs.mkdirSync(path.dirname(destination), {recursive: true});
			fs.copyFileSync(path.join(workDir, file), destination);
		}
	} finally {
		fs.rmSync(workDir, {recursive: true, force: true});
	}
}

function writeJson(file, value) {
	fs.writeFileSync(file, `${JSON.stringify(value, null, 2)}\n`);
}

function buildPlatformPackage({key, version, assets, checksums, out}) {
	const entry = PLATFORMS[key];
	const archive = path.join(assets, `pup_${version}_${entry.asset}`);
	if (!fs.existsSync(archive)) {
		throw new Error(`Missing release archive ${archive}`);
	}

	verifyArchive(archive, checksums);
	const packageDir = path.join(out, `pup-${key}`);
	const binary = path.join(packageDir, 'bin', entry.binary);
	extractFiles(archive, {
		[entry.binary]: binary,
		[THIRD_PARTY_NOTICES]: path.join(packageDir, THIRD_PARTY_NOTICES),
	});
	fs.chmodSync(binary, 0o755);
	fs.copyFileSync(path.join(REPO_ROOT, 'LICENSE'), path.join(packageDir, 'LICENSE'));
	writeJson(path.join(packageDir, 'package.json'), {
		name: platformPackageName(key),
		version,
		description: `The ${key} binary for @datadog/pup`,
		license: 'Apache-2.0',
		repository: {type: 'git', url: 'git+https://github.com/DataDog/pup.git'},
		os: [entry.os],
		cpu: [entry.cpu],
		files: ['bin', THIRD_PARTY_NOTICES],
		preferUnplugged: true,
	});
	return packageDir;
}

function buildRootPackage({version, out}) {
	const packageDir = path.join(out, 'pup');
	fs.mkdirSync(packageDir, {recursive: true});
	for (const entry of ['bin', 'lib', 'README.md']) {
		fs.cpSync(path.join(ROOT_PACKAGE_DIR, entry), path.join(packageDir, entry), {recursive: true});
	}

	fs.copyFileSync(path.join(REPO_ROOT, 'LICENSE'), path.join(packageDir, 'LICENSE'));
	const manifest = JSON.parse(fs.readFileSync(path.join(ROOT_PACKAGE_DIR, 'package.json'), 'utf8'));
	manifest.version = version;
	manifest.optionalDependencies = Object.fromEntries(
		Object.keys(PLATFORMS).map(key => [platformPackageName(key), version]),
	);
	writeJson(path.join(packageDir, 'package.json'), manifest);
	return packageDir;
}

function main() {
	const options = parseCli(process.argv.slice(2));
	fs.mkdirSync(options.out, {recursive: true});
	for (const key of options.platforms) {
		console.log(buildPlatformPackage({...options, key}));
	}

	console.log(buildRootPackage(options));
}

if (require.main === module) {
	try {
		main();
	} catch (error) {
		console.error(error.message);
		process.exit(1);
	}
}

module.exports = {parseCli, buildPlatformPackage, buildRootPackage};
