#!/usr/bin/env node
'use strict';

const {spawn} = require('node:child_process');
const {resolveBinaryPath} = require('../lib/platforms.js');

const FORWARDED_SIGNALS = ['SIGINT', 'SIGTERM', 'SIGHUP'];

let binary;
try {
	binary = resolveBinaryPath();
} catch (error) {
	console.error(error.message);
	process.exit(1);
}

const child = spawn(binary, process.argv.slice(2), {stdio: 'inherit'});

// A supervisor may signal only this launcher, not the whole process group.
for (const signal of FORWARDED_SIGNALS) {
	process.on(signal, () => child.kill(signal));
}

child.on('error', error => {
	console.error(`Failed to run ${binary}: ${error.message}`);
	process.exit(1);
});

child.on('exit', (code, signal) => {
	if (signal) {
		for (const forwarded of FORWARDED_SIGNALS) {
			process.removeAllListeners(forwarded);
		}

		process.kill(process.pid, signal);
		return;
	}

	process.exit(code ?? 1);
});
