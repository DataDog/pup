#!/usr/bin/env node
'use strict';

const {execFileSync} = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const {parseArgs} = require('node:util');
const {PLATFORMS, platformPackageName} = require('../pup/lib/platforms.js');

const REPO_ROOT = path.resolve(__dirname, '..', '..');
const ROOT_PACKAGE_DIR = path.join(REPO_ROOT, 'npm', 'pup');
const VERSION_PATTERN = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/;

function parseCli(argv) {
	const {values} = parseArgs({
		args: argv,
		options: {
			version: {type: 'string'},
			assets: {type: 'string'},
			out: {type: 'string'},
			platform: {type: 'string', multiple: true},
		},
	});
	if (!values.version || !VERSION_PATTERN.test(values.version)) {
		throw new Error('--version must be a semver version such as 1.23.7');
	}

	if (!values.assets || !values.out) {
		throw new Error('--assets and --out are required');
	}

	const platforms = values.platform ?? Object.keys(PLATFORMS);
	for (const key of platforms) {
		if (!PLATFORMS[key]) {
			throw new Error(`Unknown platform ${key}. Known: ${Object.keys(PLATFORMS).join(', ')}`);
		}
	}

	return {version: values.version, assets: values.assets, out: values.out, platforms};
}

function extractBinary(archive, binary, destination) {
	const workDir = fs.mkdtempSync(path.join(os.tmpdir(), 'pup-npm-'));
	try {
		if (archive.endsWith('.zip')) {
			execFileSync('unzip', ['-q', '-o', archive, binary, '-d', workDir]);
		} else {
			execFileSync('tar', ['-xzf', archive, '-C', workDir, binary]);
		}

		fs.mkdirSync(path.dirname(destination), {recursive: true});
		fs.copyFileSync(path.join(workDir, binary), destination);
		fs.chmodSync(destination, 0o755);
	} finally {
		fs.rmSync(workDir, {recursive: true, force: true});
	}
}

function writeJson(file, value) {
	fs.writeFileSync(file, `${JSON.stringify(value, null, 2)}\n`);
}

function buildPlatformPackage({key, version, assets, out}) {
	const entry = PLATFORMS[key];
	const archive = path.join(assets, `pup_${version}_${entry.asset}`);
	if (!fs.existsSync(archive)) {
		throw new Error(`Missing release archive ${archive}`);
	}

	const packageDir = path.join(out, `pup-${key}`);
	extractBinary(archive, entry.binary, path.join(packageDir, 'bin', entry.binary));
	fs.copyFileSync(path.join(REPO_ROOT, 'LICENSE'), path.join(packageDir, 'LICENSE'));
	writeJson(path.join(packageDir, 'package.json'), {
		name: platformPackageName(key),
		version,
		description: `The ${key} binary for @datadog/pup`,
		license: 'Apache-2.0',
		repository: {type: 'git', url: 'git+https://github.com/DataDog/pup.git'},
		os: [entry.os],
		cpu: [entry.cpu],
		files: ['bin'],
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
