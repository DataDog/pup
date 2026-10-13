'use strict';

const PLATFORMS = {
	'darwin-arm64': {os: 'darwin', cpu: 'arm64', asset: 'Darwin_arm64.tar.gz', binary: 'pup'},
	'darwin-x64': {os: 'darwin', cpu: 'x64', asset: 'Darwin_x86_64.tar.gz', binary: 'pup'},
	'linux-arm64': {os: 'linux', cpu: 'arm64', asset: 'Linux_arm64.tar.gz', binary: 'pup'},
	'linux-x64': {os: 'linux', cpu: 'x64', asset: 'Linux_x86_64.tar.gz', binary: 'pup'},
	'win32-x64': {os: 'win32', cpu: 'x64', asset: 'Windows_x86_64.zip', binary: 'pup.exe'},
};

function platformPackageName(key) {
	return `@datadog/pup-${key}`;
}

function resolveBinaryPath({
	platform = process.platform,
	arch = process.arch,
	resolve = require.resolve,
} = {}) {
	const key = `${platform}-${arch}`;
	const entry = PLATFORMS[key];
	if (!entry) {
		throw new Error(
			`pup does not publish a binary for ${key}. Supported platforms: ${Object.keys(PLATFORMS).join(', ')}.`,
		);
	}

	const packageName = platformPackageName(key);
	try {
		return resolve(`${packageName}/bin/${entry.binary}`);
	} catch {
		throw new Error(
			`The ${packageName} package is not installed. It is an optional dependency of @datadog/pup; reinstall without --omit=optional or --no-optional.`,
		);
	}
}

module.exports = {PLATFORMS, platformPackageName, resolveBinaryPath};
