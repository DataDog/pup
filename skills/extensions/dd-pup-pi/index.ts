/**
 * dd-pup — Datadog CLI integration for pi
 *
 * Exposes the `pup` CLI as first-class LLM tools so the model can:
 *   - query telemetry (logs, metrics, traces, RUM, events, audit logs)
 *   - inspect APM services, hosts, incidents, SLOs, monitors
 *   - manage Datadog resources (monitors, downtimes, dashboards, notebooks, ...)
 *
 * Features
 * --------
 * - `pup_run`              flexible escape-hatch: run any `pup ...` subcommand
 * - `pup_logs_search`      opinionated log search (JSON)
 * - `pup_logs_aggregate`   counts / distributions instead of fetching raw logs
 * - `pup_metrics_query`    time-series metrics
 * - `pup_traces_search`    APM trace search (durations are nanoseconds!)
 * - `pup_monitors_list`    list/filter monitors
 * - `pup_apm_services`     APM service catalog stats
 * - `pup_auth_status`      check / refresh auth
 *
 * Auto-recovery: on 401/403, the extension transparently runs
 * `pup auth refresh` once and retries the command.
 *
 * Slash commands:
 *   /pup       <args>      raw passthrough, prints output (no LLM round-trip)
 *   /pup-auth              login / refresh / status helper
 *
 * Status widget shows authenticated site + token expiry.
 */

import { Type, type Static } from "typebox";
import type {
	ExtensionAPI,
	ExtensionContext,
	ToolExecutionResult,
} from "@earendil-works/pi-coding-agent";

// ---------------------------------------------------------------------------
// pup runner with auto-refresh on 401/403
// ---------------------------------------------------------------------------

interface PupResult {
	stdout: string;
	stderr: string;
	code: number;
	killed: boolean;
}

interface RunOptions {
	signal?: AbortSignal;
	timeoutMs?: number;
	stdin?: string;
}

const PUP_BIN = process.env.DD_PUP_BIN || "pup";
const DEFAULT_TIMEOUT_MS = 90_000;
const AUTH_FAILURE_REGEX =
	/\b(401|403|unauthorized|forbidden|token (?:has )?expired|please (?:re)?-?authenticate|access token)\b/i;

async function runPup(
	pi: ExtensionAPI,
	args: string[],
	opts: RunOptions = {},
): Promise<PupResult> {
	const timeout = opts.timeoutMs ?? DEFAULT_TIMEOUT_MS;
	const result = await pi.exec(PUP_BIN, args, {
		signal: opts.signal,
		timeout,
		stdin: opts.stdin,
	});
	return {
		stdout: result.stdout ?? "",
		stderr: result.stderr ?? "",
		code: result.code ?? 0,
		killed: result.killed ?? false,
	};
}

function looksLikeAuthFailure(r: PupResult): boolean {
	if (r.code === 0) return false;
	const blob = `${r.stdout}\n${r.stderr}`;
	return AUTH_FAILURE_REGEX.test(blob);
}

/** Run pup, and if it fails on auth, run `pup auth refresh` once then retry. */
async function runPupWithRetry(
	pi: ExtensionAPI,
	args: string[],
	opts: RunOptions = {},
): Promise<{ result: PupResult; refreshed: boolean }> {
	const first = await runPup(pi, args, opts);
	if (!looksLikeAuthFailure(first)) return { result: first, refreshed: false };

	// Try a silent refresh, then retry once.
	const refresh = await runPup(pi, ["auth", "refresh"], {
		signal: opts.signal,
		timeoutMs: 15_000,
	});
	if (refresh.code !== 0) {
		// Refresh failed — return the original failure so the LLM sees it.
		return { result: first, refreshed: false };
	}
	const second = await runPup(pi, args, opts);
	return { result: second, refreshed: true };
}

// ---------------------------------------------------------------------------
// Output shaping
// ---------------------------------------------------------------------------

const MAX_TEXT_CHARS = 24_000;

function truncate(s: string, n = MAX_TEXT_CHARS): string {
	if (s.length <= n) return s;
	return `${s.slice(0, n)}\n…[truncated ${s.length - n} chars]`;
}

/** Try to compactly stringify JSON; fall back to raw on failure. */
function shapeOutput(stdout: string): { text: string; parsed?: unknown } {
	const trimmed = stdout.trim();
	if (!trimmed) return { text: "(empty output)" };
	try {
		const parsed = JSON.parse(trimmed);
		return { text: truncate(JSON.stringify(parsed, null, 2)), parsed };
	} catch {
		return { text: truncate(stdout) };
	}
}

function toResult(
	args: string[],
	r: PupResult,
	refreshed: boolean,
	extra?: Record<string, unknown>,
): ToolExecutionResult {
	const shaped = shapeOutput(r.stdout);
	const ok = r.code === 0 && !r.killed;
	const header = ok
		? `$ pup ${args.join(" ")}${refreshed ? "  (auto-refreshed auth)" : ""}`
		: `$ pup ${args.join(" ")}  → exit ${r.code}${r.killed ? " (killed)" : ""}`;

	let body = shaped.text;
	if (!ok && r.stderr.trim()) {
		body = `${body}\n\nstderr:\n${truncate(r.stderr)}`;
	} else if (ok && r.stderr.trim()) {
		// pup writes some informational chatter to stderr
		body = `${body}\n\n[stderr]\n${truncate(r.stderr, 2_000)}`;
	}

	return {
		content: [{ type: "text", text: `${header}\n\n${body}` }],
		isError: !ok,
		details: {
			argv: args,
			exitCode: r.code,
			killed: r.killed,
			refreshed,
			parsed: shaped.parsed,
			...extra,
		},
	};
}

// ---------------------------------------------------------------------------
// Argument-builder helpers
// ---------------------------------------------------------------------------

function addFlag(argv: string[], flag: string, value: unknown): void {
	if (value === undefined || value === null) return;
	if (typeof value === "boolean") {
		if (value) argv.push(flag);
		return;
	}
	if (Array.isArray(value)) {
		for (const v of value) {
			if (v === undefined || v === null) continue;
			argv.push(flag, String(v));
		}
		return;
	}
	argv.push(flag, String(value));
}

// ---------------------------------------------------------------------------
// Tool schemas
// ---------------------------------------------------------------------------

const RunParams = Type.Object({
	args: Type.Array(Type.String(), {
		description:
			"Arguments to pass to `pup`, as an array. The leading `pup` is implicit. " +
			"Example: [\"monitors\", \"list\", \"--tags\", \"env:prod\", \"--limit\", \"5\"]. " +
			"JSON output is enforced unless you pass --output yourself.",
		minItems: 1,
	}),
	timeoutSeconds: Type.Optional(
		Type.Number({
			description: "Override default 90s timeout. Max 600.",
			minimum: 1,
			maximum: 600,
		}),
	),
});
type RunInput = Static<typeof RunParams>;

const LogsSearchParams = Type.Object({
	query: Type.String({
		description:
			'Datadog log query. Examples: "status:error service:payment-api", ' +
			'"@http.status_code:5*", "host:i-abc*"',
	}),
	from: Type.Optional(
		Type.String({
			description: "Relative time window (default 1h). Examples: 15m, 1h, 24h, 7d",
		}),
	),
	to: Type.Optional(Type.String({ description: "End time (default: now)" })),
	limit: Type.Optional(
		Type.Number({
			description: "Max logs to return (default 50). Keep small.",
			minimum: 1,
			maximum: 1000,
		}),
	),
	indexes: Type.Optional(
		Type.Array(Type.String(), {
			description: "Restrict to specific log indexes",
		}),
	),
});
type LogsSearchInput = Static<typeof LogsSearchParams>;

const LogsAggregateParams = Type.Object({
	query: Type.String({ description: "Datadog log query filter" }),
	compute: Type.String({
		description:
			"Aggregation, e.g. 'count', 'avg(@duration)', 'pc95(@latency)', 'cardinality(@user.id)'",
	}),
	groupBy: Type.Optional(
		Type.Array(Type.String(), {
			description: "Group-by facets, e.g. ['service', 'status']",
		}),
	),
	from: Type.Optional(Type.String({ description: "Relative window (default 1h)" })),
	to: Type.Optional(Type.String()),
});
type LogsAggregateInput = Static<typeof LogsAggregateParams>;

const MetricsQueryParams = Type.Object({
	query: Type.String({
		description:
			"Datadog metrics query. Must include an aggregation. " +
			'Example: "avg:system.cpu.user{env:prod} by {host}"',
	}),
	from: Type.Optional(Type.String({ description: "Relative window, default 1h" })),
	to: Type.Optional(Type.String()),
});
type MetricsQueryInput = Static<typeof MetricsQueryParams>;

const TracesSearchParams = Type.Object({
	query: Type.String({
		description:
			"APM trace query. Durations are in NANOSECONDS. " +
			'Example: "service:api @duration:>1000000000 status:error"',
	}),
	from: Type.Optional(Type.String({ description: "Relative window, default 1h" })),
	to: Type.Optional(Type.String()),
	limit: Type.Optional(Type.Number({ minimum: 1, maximum: 500 })),
});
type TracesSearchInput = Static<typeof TracesSearchParams>;

const MonitorsListParams = Type.Object({
	tags: Type.Optional(
		Type.String({
			description: "Comma-separated tag filter, e.g. 'env:prod,team:platform'",
		}),
	),
	name: Type.Optional(
		Type.String({ description: "Name substring filter" }),
	),
	limit: Type.Optional(Type.Number({ minimum: 1, maximum: 1000 })),
});
type MonitorsListInput = Static<typeof MonitorsListParams>;

const ApmServicesParams = Type.Object({
	env: Type.String({ description: "Environment, e.g. 'production'" }),
	stats: Type.Optional(
		Type.Boolean({
			description: "Return per-service stats instead of plain list (default false)",
		}),
	),
});
type ApmServicesInput = Static<typeof ApmServicesParams>;

const AuthStatusParams = Type.Object({
	refresh: Type.Optional(
		Type.Boolean({
			description: "If true, attempts `pup auth refresh` before reporting status",
		}),
	),
});
type AuthStatusInput = Static<typeof AuthStatusParams>;

// ---------------------------------------------------------------------------
// Extension
// ---------------------------------------------------------------------------

export default function ddPupExtension(pi: ExtensionAPI) {
	let lastAuthSummary = "unknown";

	// ---- helpers --------------------------------------------------------------

	async function refreshAuthWidget(ctx?: ExtensionContext) {
		const r = await runPup(pi, ["auth", "status"], { timeoutMs: 8_000 });
		if (r.code !== 0) {
			lastAuthSummary = "unauthenticated";
		} else {
			let parsed: any = null;
			try {
				parsed = JSON.parse(r.stdout);
			} catch {
				/* ignore */
			}
			if (parsed?.authenticated) {
				const site = parsed.site || process.env.DD_SITE || "datadoghq.com";
				const exp = parsed.expires_at
					? ` (exp ${String(parsed.expires_at).replace(/T/, " ").slice(0, 19)})`
					: "";
				lastAuthSummary = `✓ ${site}${exp}`;
			} else {
				lastAuthSummary = "unauthenticated";
			}
		}
		const widget = ctx?.ui ?? (ctx as any);
		if (widget?.setStatus) {
			widget.setStatus("dd-pup", `pup: ${lastAuthSummary}`);
		}
	}

	function ensureJsonOutput(args: string[]): string[] {
		// Honor explicit --output already in args.
		for (const a of args) {
			if (a === "--output" || a.startsWith("--output=")) return args;
		}
		return [...args, "--output", "json"];
	}

	// ---- tools ----------------------------------------------------------------

	pi.registerTool({
		name: "pup_run",
		label: "pup (raw)",
		description:
			"Run any `pup` subcommand against Datadog. The leading `pup` is implicit; " +
			"pass the rest as `args`. JSON output is enforced unless you set --output. " +
			"Use this for anything not covered by the focused pup_* tools (dashboards, " +
			"incidents, SLOs, downtimes, RUM, security, infra, on-call, etc.). " +
			"Auth is auto-refreshed on 401/403.",
		promptSnippet:
			"Run any `pup` Datadog CLI subcommand. Use for Datadog telemetry & resource management.",
		promptGuidelines: [
			"Use pup_run for Datadog operations the focused pup_* tools don't cover (incidents, SLOs, dashboards, downtimes, RUM, security signals, infra hosts, on-call, etc.).",
			"When calling pup_run, never include the leading 'pup' word; pass the subcommand and flags as an args array.",
			"For pup_run telemetry queries, always pass --from to bound the time range; start narrow (1h) before widening.",
			"Remember APM durations in pup queries are NANOSECONDS (1s = 1000000000). Use pup_logs_aggregate to count instead of fetching raw logs.",
		],
		parameters: RunParams,
		async execute(_id, params: RunInput, signal): Promise<ToolExecutionResult> {
			const argv = ensureJsonOutput(params.args.slice());
			const { result, refreshed } = await runPupWithRetry(pi, argv, {
				signal,
				timeoutMs: (params.timeoutSeconds ?? 90) * 1000,
			});
			return toResult(argv, result, refreshed);
		},
	});

	pi.registerTool({
		name: "pup_logs_search",
		label: "pup logs search",
		description:
			"Search Datadog logs with a query string. Returns up to `limit` log events as JSON. " +
			"Prefer pup_logs_aggregate when you only need counts/distributions.",
		promptSnippet: "Search Datadog logs by query + time window.",
		promptGuidelines: [
			"Use pup_logs_search to retrieve example log events; use pup_logs_aggregate for counts.",
			"For pup_logs_search keep --limit small (≤50) on first call; widen only if needed.",
		],
		parameters: LogsSearchParams,
		async execute(_id, params: LogsSearchInput, signal): Promise<ToolExecutionResult> {
			const argv = ["logs", "search", "--query", params.query];
			addFlag(argv, "--from", params.from ?? "1h");
			addFlag(argv, "--to", params.to);
			addFlag(argv, "--limit", params.limit ?? 50);
			addFlag(argv, "--indexes", params.indexes);
			argv.push("--output", "json");
			const { result, refreshed } = await runPupWithRetry(pi, argv, { signal });
			return toResult(argv, result, refreshed);
		},
	});

	pi.registerTool({
		name: "pup_logs_aggregate",
		label: "pup logs aggregate",
		description:
			"Aggregate Datadog logs (count, avg, percentiles, cardinality) with optional " +
			"group-by. Use this instead of fetching raw logs whenever the question is " +
			"'how many?', 'what's the rate?', 'top N by …?'.",
		promptSnippet: "Aggregate Datadog logs (count, percentile, etc.) by facet.",
		promptGuidelines: [
			"Use pup_logs_aggregate for log counts / distributions instead of fetching and counting locally.",
		],
		parameters: LogsAggregateParams,
		async execute(_id, params: LogsAggregateInput, signal): Promise<ToolExecutionResult> {
			const argv = [
				"logs",
				"aggregate",
				"--query",
				params.query,
				"--compute",
				params.compute,
			];
			addFlag(argv, "--from", params.from ?? "1h");
			addFlag(argv, "--to", params.to);
			if (params.groupBy?.length) {
				for (const g of params.groupBy) argv.push("--group-by", g);
			}
			argv.push("--output", "json");
			const { result, refreshed } = await runPupWithRetry(pi, argv, { signal });
			return toResult(argv, result, refreshed);
		},
	});

	pi.registerTool({
		name: "pup_metrics_query",
		label: "pup metrics query",
		description:
			"Query a Datadog metric time-series. Query MUST include an aggregation " +
			"(avg, sum, max, min, count). Example: avg:system.cpu.user{env:prod} by {host}.",
		promptSnippet: "Query Datadog metrics (time-series).",
		promptGuidelines: [
			"For pup_metrics_query the query string must start with an aggregation (avg:, sum:, max:, min:, count:).",
		],
		parameters: MetricsQueryParams,
		async execute(_id, params: MetricsQueryInput, signal): Promise<ToolExecutionResult> {
			const argv = ["metrics", "query", "--query", params.query];
			addFlag(argv, "--from", params.from ?? "1h");
			addFlag(argv, "--to", params.to);
			argv.push("--output", "json");
			const { result, refreshed } = await runPupWithRetry(pi, argv, { signal });
			return toResult(argv, result, refreshed);
		},
	});

	pi.registerTool({
		name: "pup_traces_search",
		label: "pup traces search",
		description:
			"Search APM traces. WARNING: @duration is in NANOSECONDS " +
			"(1s = 1000000000, 5ms = 5000000).",
		promptSnippet: "Search APM traces (nanosecond durations).",
		promptGuidelines: [
			"For pup_traces_search, @duration values are in NANOSECONDS — never seconds or ms.",
		],
		parameters: TracesSearchParams,
		async execute(_id, params: TracesSearchInput, signal): Promise<ToolExecutionResult> {
			const argv = ["traces", "search", `--query=${params.query}`];
			addFlag(argv, "--from", params.from ?? "1h");
			addFlag(argv, "--to", params.to);
			addFlag(argv, "--limit", params.limit);
			argv.push("--output", "json");
			const { result, refreshed } = await runPupWithRetry(pi, argv, { signal });
			return toResult(argv, result, refreshed);
		},
	});

	pi.registerTool({
		name: "pup_monitors_list",
		label: "pup monitors list",
		description: "List Datadog monitors with optional tag/name filters.",
		promptSnippet: "List Datadog monitors with optional filters.",
		parameters: MonitorsListParams,
		async execute(_id, params: MonitorsListInput, signal): Promise<ToolExecutionResult> {
			const argv = ["monitors", "list"];
			addFlag(argv, "--tags", params.tags);
			addFlag(argv, "--name", params.name);
			addFlag(argv, "--limit", params.limit ?? 25);
			argv.push("--output", "json");
			const { result, refreshed } = await runPupWithRetry(pi, argv, { signal });
			return toResult(argv, result, refreshed);
		},
	});

	pi.registerTool({
		name: "pup_apm_services",
		label: "pup apm services",
		description:
			"List APM services in a given environment, or fetch per-service stats when stats=true.",
		promptSnippet: "List APM services or per-service stats for an env.",
		parameters: ApmServicesParams,
		async execute(_id, params: ApmServicesInput, signal): Promise<ToolExecutionResult> {
			const argv = ["apm", "services", params.stats ? "stats" : "list", "--env", params.env];
			argv.push("--output", "json");
			const { result, refreshed } = await runPupWithRetry(pi, argv, { signal });
			return toResult(argv, result, refreshed);
		},
	});

	pi.registerTool({
		name: "pup_auth_status",
		label: "pup auth status",
		description:
			"Check current Datadog auth status. Set refresh=true to first run `pup auth refresh`.",
		promptSnippet: "Check / refresh pup (Datadog) auth.",
		parameters: AuthStatusParams,
		async execute(_id, params: AuthStatusInput, signal): Promise<ToolExecutionResult> {
			if (params.refresh) {
				const refreshed = await runPup(pi, ["auth", "refresh"], { signal, timeoutMs: 15_000 });
				if (refreshed.code !== 0) {
					return toResult(["auth", "refresh"], refreshed, false);
				}
			}
			const status = await runPup(pi, ["auth", "status", "--output", "json"], { signal });
			// Update widget side-effect
			void refreshAuthWidget();
			return toResult(["auth", "status"], status, !!params.refresh);
		},
	});

	// ---- commands -------------------------------------------------------------

	/** /pup <args>  — quick passthrough for the human (no LLM round-trip) */
	pi.registerCommand("pup", {
		description: "Run `pup <args>` directly and print the output",
		handler: async (args, ctx) => {
			const argv = args.trim().length > 0 ? args.trim().split(/\s+/) : ["--help"];
			ctx.ui.setStatus("dd-pup-run", `running: pup ${argv.join(" ")}`);
			const { result } = await runPupWithRetry(pi, argv, { timeoutMs: 60_000 });
			ctx.ui.setStatus("dd-pup-run", "");
			const head = `$ pup ${argv.join(" ")}  (exit ${result.code})`;
			const out = (result.stdout || result.stderr || "(no output)").trim();
			ctx.ui.notify(`${head}\n\n${truncate(out, 4_000)}`, result.code === 0 ? "info" : "error");
		},
	});

	/** /pup-auth — interactive auth helper */
	pi.registerCommand("pup-auth", {
		description: "Datadog auth: status, refresh, login, logout",
		handler: async (_args, ctx) => {
			const choice = await ctx.ui.select("pup auth", [
				"status",
				"refresh",
				"login",
				"logout",
			]);
			if (!choice) return;

			if (choice === "login") {
				ctx.ui.notify("Opening browser for `pup auth login` …", "info");
				// login is interactive; spawn but don't block the UI thread on stdin
				const res = await runPup(pi, ["auth", "login"], { timeoutMs: 300_000 });
				ctx.ui.notify(
					res.code === 0 ? "Login complete." : `Login failed (exit ${res.code})`,
					res.code === 0 ? "success" : "error",
				);
			} else {
				const res = await runPup(pi, ["auth", choice], { timeoutMs: 30_000 });
				ctx.ui.notify(
					truncate((res.stdout || res.stderr || "").trim() || `(exit ${res.code})`, 2_000),
					res.code === 0 ? "info" : "error",
				);
			}
			await refreshAuthWidget(ctx);
		},
	});

	// ---- lifecycle ------------------------------------------------------------

	pi.on("session_start", async (_event, ctx) => {
		// Verify pup is installed
		const probe = await runPup(pi, ["--version"], { timeoutMs: 5_000 });
		if (probe.code !== 0) {
			ctx.ui.setStatus(
				"dd-pup",
				`pup: NOT FOUND (install: brew tap datadog-labs/pack && brew install pup)`,
			);
			return;
		}
		await refreshAuthWidget(ctx);
	});
}
