#!/usr/bin/env python3
"""
pup integration test harness

Builds the release binary, injects DD_API_*/DD_APP_KEY credentials from dd-auth,
ensures a valid OAuth2 token (status → refresh → login), executes all read commands,
checks for output defects and visual regressions, then generates an HTML report.

Usage:
    python3 scripts/test_harness.py [options]

Options:
    --update-snapshots      Accept current output as the new baseline
    --no-build              Skip cargo build (use existing binary)
    --no-auth               Skip auth checks and dd-auth credential injection
    --dd-auth-domain DOMAIN dd-auth domain to use (default: app.datadoghq.com)
    --output FILE           Path for the HTML report (default: /tmp/pup-dev/harness_report.html)
    --filter PATTERN        Only run tests whose label contains PATTERN
    --timeout SECS          Per-command timeout in seconds (default: 30)
"""

import argparse
import difflib
import json
import os
import re
import signal
import subprocess
import sys
import time
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

# ── paths ─────────────────────────────────────────────────────────────────────

REPO_ROOT = Path(__file__).parent.parent.resolve()
BINARY = REPO_ROOT / "target" / "release" / "pup"
SNAPSHOT_DIR = REPO_ROOT / "tests" / "snapshots"
LAST_OUTPUT_DIR = Path("/tmp/pup-dev")
DEFAULT_REPORT = LAST_OUTPUT_DIR / "harness_report.html"

# ── snapshot value normalization ──────────────────────────────────────────────
#
# Before computing the structural schema we normalize "stable-but-opaque" values
# so snapshots don't regress on rotating IDs, timestamps, or counts.
#
# Rules are applied in order; the first matching pattern wins for each string.

# Patterns that, when a dict's KEYS all match, collapse the whole dict to
# {"*": <value_schema>} so we don't snapshot individual service/metric/tag names.
DYNAMIC_KEY_PATTERNS: list[re.Pattern] = [
    re.compile(r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$", re.I),  # UUID
    re.compile(r"^[a-z0-9]([a-z0-9\-\.]*[a-z0-9])?:[^:]+$"),  # tag (env:prod, host:web-01)
    re.compile(r"^\d+$"),                                        # pure numeric id
    re.compile(r"^[a-z][a-z0-9_\-\.]+\.[a-z][a-z0-9_\-\.]+"),  # dotted.metric.name
    re.compile(r"^[a-z][a-z0-9_]*(-[a-z0-9_]+)+$"),             # hyphenated service names (apm-service, account-config)
]

# Minimum number of keys before auto-detecting a "dynamic keys" dict.
DYNAMIC_KEY_MIN = 4

# Explicit path patterns (dot-joined key path, regex) that force "dynamic keys"
# treatment regardless of key content.  Add entries here when you encounter a
# command whose top-level or nested dict uses caller-defined string keys.
FORCE_DYNAMIC_KEY_PATHS: list[re.Pattern] = [
    re.compile(r".*\.tags$"),
    re.compile(r".*\.meta$"),
    re.compile(r".*\.attributes$"),
    re.compile(r".*\.attributes\.\*$"),   # inner log/event attribute dicts (CN, action, client_ip …)
    re.compile(r".*\.metrics$"),
    # Relationships dicts (on cases, incidents, etc.) have resource-specific optional
    # keys (assignee, project, created_by, …) that vary per item.  Collapse to {*: …}.
    re.compile(r".*\.relationships$"),
    re.compile(r".*\.usage$"),
    # Infrastructure hosts: tags_by_source keys are cloud provider names ("Amazon Web
    # Services", "Google Cloud Platform", …) that vary depending on which hosts
    # the API returns on a given run — collapse to {"*": …} so snapshots are stable.
    re.compile(r".*\.tags_by_source$"),
    # Any path already two levels deep in dynamic-key territory (.*.*) will vary
    # too much across API calls to snapshot reliably — collapse it.
    re.compile(r".*\.\*\.\*.*"),
]

# String value normalization: replace with a stable placeholder.
# Applied to leaf string values before schema extraction so that rotating
# IDs/timestamps don't cause false positives.
VALUE_NORMALIZERS: list[tuple[re.Pattern, str]] = [
    (re.compile(r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$", re.I), "<uuid>"),
    (re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}"), "<timestamp>"),
    (re.compile(r"^\d{10,13}$"), "<unix_ts>"),
    (re.compile(r"^https?://"), "<url>"),
    (re.compile(r"^[0-9a-f]{32,64}$", re.I), "<hash>"),
]

# All possible normalized placeholder values.  When both the baseline and
# the current value are placeholders, the actual content is just "some string"
# in both cases — no structural regression.
_NORMALIZED_PLACEHOLDERS = {"str", "<uuid>", "<timestamp>", "<unix_ts>", "<url>", "<hash>"}

# Schema paths that are known-optional: pup emits them only when non-default
# (e.g. truncated=false is skipped, next_action=None is skipped).  Treat their
# absence in the current output as non-regressing.
_OPTIONAL_SCHEMA_PATHS: frozenset[str] = frozenset({
    ".metadata.next_action",
    ".metadata.truncated",
})


def _all_keys_dynamic(keys: list[str]) -> bool:
    """Return True when every key in a dict looks like a dynamic identifier."""
    if len(keys) < DYNAMIC_KEY_MIN:
        return False
    return all(
        any(p.fullmatch(k) for p in DYNAMIC_KEY_PATTERNS)
        for k in keys
    )


def _path_forces_dynamic(path: str) -> bool:
    return any(p.fullmatch(path) for p in FORCE_DYNAMIC_KEY_PATHS)


def normalize_value(val: str) -> str:
    for pattern, placeholder in VALUE_NORMALIZERS:
        if pattern.search(val):
            return placeholder
    return val


def extract_json_schema(data: Any, max_depth: int = 6, path: str = "") -> Any:
    """
    Recursively extract the structural schema of a JSON value.

    - dict  → {"key": child_schema, ...}
              OR {"*": child_schema} when keys are detected as dynamic
    - list  → ["<list>", first_element_schema] or ["<empty_list>"]
    - str   → normalized placeholder or "str"
    - other → type name string
    """
    if max_depth == 0:
        return "<...>"

    if isinstance(data, dict):
        keys = list(data.keys())
        if not keys:
            return {}

        if _path_forces_dynamic(path) or _all_keys_dynamic(keys):
            child_path = f"{path}.*"
            # Short-circuit: if the child path would also be forced dynamic, don't
            # recurse further — the values are too variable to snapshot reliably.
            if _path_forces_dynamic(child_path):
                return {"*": "<...>"}
            # Merge ALL value schemas into a union so the result is stable across
            # API calls that return different subsets of optional fields.
            merged: Any = None
            for v in data.values():
                child = extract_json_schema(v, max_depth - 1, child_path)
                if merged is None:
                    merged = child
                elif isinstance(merged, dict) and isinstance(child, dict):
                    for k, cv in child.items():
                        if k not in merged:
                            merged[k] = cv
            return {"*": merged if merged is not None else "<empty>"}

        return {k: extract_json_schema(v, max_depth - 1, f"{path}.{k}") for k, v in sorted(data.items())}

    if isinstance(data, list):
        if not data:
            return ["<empty_list>"]
        # Merge schemas from ALL elements so the snapshot is a stable union of
        # optional fields rather than an unstable sample of whichever element
        # happens to appear first on this particular API call.
        child_path = f"{path}[]"
        merged = extract_json_schema(data[0], max_depth - 1, child_path)
        for item in data[1:]:
            child = extract_json_schema(item, max_depth - 1, child_path)
            if isinstance(merged, dict) and isinstance(child, dict):
                for k, cv in child.items():
                    if k not in merged:
                        merged[k] = cv
        return ["<list>", merged]

    if isinstance(data, str):
        normalized = normalize_value(data)
        if normalized != data:
            return normalized
        return "str"

    if isinstance(data, bool):
        return "bool"

    return type(data).__name__


# ── snapshot / visual regression ──────────────────────────────────────────────

def snapshot_path(label: str, mode: str = "human") -> Path:
    safe = re.sub(r"[^a-zA-Z0-9_-]", "_", label)
    suffix = f"__{mode}" if mode else ""
    return SNAPSHOT_DIR / f"{safe}{suffix}.json"


def _last_output_path(label: str, mode: str) -> Path:
    """Path for the raw stdout from the previous run — used for per-row diffs."""
    safe = re.sub(r"[^a-zA-Z0-9_-]", "_", label)
    return LAST_OUTPUT_DIR / f"{safe}__{mode}.last"


def load_last_output(label: str, mode: str) -> str | None:
    p = _last_output_path(label, mode)
    if p.exists():
        try:
            return p.read_text()
        except Exception:
            return None
    return None


def save_last_output(label: str, mode: str, text: str) -> None:
    LAST_OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    _last_output_path(label, mode).write_text(text)


def load_snapshot(label: str, mode: str = "human") -> Any:
    p = snapshot_path(label, mode)
    if p.exists():
        try:
            return json.loads(p.read_text())
        except Exception:
            return None
    return None


def save_snapshot(label: str, schema: Any, mode: str = "human") -> None:
    SNAPSHOT_DIR.mkdir(parents=True, exist_ok=True)
    p = snapshot_path(label, mode)
    p.write_text(json.dumps(schema, indent=2))


def diff_schemas(old: Any, new: Any, path: str = "") -> tuple[list[str], list[str]]:
    """
    Compare two schemas.  Returns (regressions, additions).

    regressions — keys removed or types changed: these are real failures.
    additions   — keys present in new but absent from old: these are auto-merged
                  into the snapshot so future runs include them.
    """
    regressions: list[str] = []
    additions: list[str] = []

    if type(old) != type(new):
        regressions.append(f"{path or 'root'}: type changed {type(old).__name__} → {type(new).__name__}")
        return regressions, additions

    if isinstance(old, dict):
        old_keys = set(old)
        new_keys = set(new)
        for k in sorted(old_keys - new_keys):
            full_path = f"{path}.{k}"
            if full_path in _OPTIONAL_SCHEMA_PATHS:
                continue  # known optional field — its absence is not a regression
            regressions.append(f"{full_path}: key removed")
        for k in sorted(new_keys - old_keys):
            additions.append(f"{path}.{k}: key added")
        for k in sorted(old_keys & new_keys):
            r, a = diff_schemas(old[k], new[k], f"{path}.{k}")
            regressions.extend(r)
            additions.extend(a)

    elif isinstance(old, list):
        old_inner = old[1] if len(old) > 1 else None
        new_inner = new[1] if len(new) > 1 else None
        if old[0] != new[0]:
            # Treat any transition between empty and populated list as additive.
            # "<empty_list>" → "<list>": baseline had no data; API now returns items.
            # "<list>" → "<empty_list>": baseline had items; this run happens to be
            #   empty (no outages, no results, etc.) — keep the richer baseline schema.
            # Both directions are non-regressing: list emptiness is a runtime condition,
            # not a structural change in the API or pup's output format.
            empty_to_list = (old[0] == "<empty_list>" and new[0] == "<list>")
            list_to_empty = (old[0] == "<list>" and new[0] == "<empty_list>")
            if empty_to_list or list_to_empty:
                additions.append(f"{path}: list emptiness changed '{old[0]}' → '{new[0]}'")
            else:
                regressions.append(f"{path}: list structure changed '{old[0]}' → '{new[0]}'")
        elif old_inner is not None and new_inner is not None:
            r, a = diff_schemas(old_inner, new_inner, f"{path}[]")
            regressions.extend(r)
            additions.extend(a)

    elif old != new:
        # If both values are normalized placeholders they represent the same
        # thing ("some string") — the content merely changed category, which
        # is not a structural regression worth failing on.
        if old in _NORMALIZED_PLACEHOLDERS and new in _NORMALIZED_PLACEHOLDERS:
            additions.append(f"{path or 'root'}: placeholder changed '{old}' → '{new}'")
        else:
            regressions.append(f"{path or 'root'}: value changed '{old}' → '{new}'")

    return regressions, additions


def _truncate_schema(schema: Any, depth: int) -> Any:
    """
    Collapse schema nodes below `depth` levels to "<...>".

    Used by check_regression when a catalog entry sets max_regression_depth.
    Both the stored baseline and the current schema are truncated identically
    before diff_schemas runs, so volatility below the threshold never triggers
    a regression while structural changes above it still do.

    depth=1 checks that top-level keys exist and their immediate types match.
    depth=2 checks one level of nesting beneath the root, etc.
    """
    if depth <= 0:
        return "<...>"
    if isinstance(schema, dict):
        return {k: _truncate_schema(v, depth - 1) for k, v in schema.items()}
    if isinstance(schema, list):
        if not schema:
            return schema
        result: list = [schema[0]]
        if len(schema) > 1:
            result.append(_truncate_schema(schema[1], depth - 1))
        return result
    return schema  # scalar leaf — keep as-is


def _merge_schemas(base: Any, new: Any) -> Any:
    """Merge new schema into base, adding any keys present in new but absent from base."""
    if isinstance(base, dict) and isinstance(new, dict):
        merged = dict(base)
        for k, v in new.items():
            if k in merged:
                merged[k] = _merge_schemas(merged[k], v)
            else:
                merged[k] = v
        return merged
    if isinstance(base, list) and isinstance(new, list):
        # Upgrade ["<empty_list>"] → ["<list>", schema] when the API finally
        # returns data for a list that was previously empty.
        if base and base[0] == "<empty_list>" and new and new[0] == "<list>":
            return new
        # Merge list element schemas (keep the richer union)
        if base and new and base[0] == new[0] == "<list>":
            merged_inner = _merge_schemas(base[1], new[1]) if len(base) > 1 and len(new) > 1 else (base[1] if len(base) > 1 else new[1] if len(new) > 1 else None)
            return ["<list>", merged_inner] if merged_inner is not None else base
    return base  # for scalars keep the base


def check_regression(
    result: "TestResult",
    update_snapshots: bool,
    max_regression_depth: int = 0,
) -> tuple[str | None, bool]:
    """
    Returns (regression_description | None, snapshot_was_created_or_updated).

    Regression policy:
    - Removed keys or type changes → FAIL (real regression)
    - New keys in current output   → auto-merge into snapshot (additive update, no failure)
      This makes the baseline a stable "floor schema" that only grows over time.

    max_regression_depth (0 = unlimited):
      When set, both the stored baseline and the current schema are truncated to
      this many levels before comparison.  Nodes deeper than the limit become
      "<...>" on both sides, so volatility in deeply-nested optional fields never
      produces false-positive regressions.  The snapshot itself is still saved at
      full depth so future runs benefit from a richer baseline.
    """
    if result.exit_code != 0 or not result.stdout.strip():
        return None, False

    try:
        data = json.loads(result.stdout.strip())
    except (json.JSONDecodeError, ValueError):
        return None, False

    current_schema = extract_json_schema(data)
    existing = load_snapshot(result.label, result.mode)

    if existing is None:
        save_snapshot(result.label, current_schema, result.mode)
        return None, True

    if update_snapshots:
        save_snapshot(result.label, current_schema, result.mode)
        return None, False

    # Apply depth truncation when requested.  Both schemas are truncated
    # identically so structural changes above the threshold still register.
    if max_regression_depth > 0:
        cmp_existing = _truncate_schema(existing, max_regression_depth)
        cmp_current  = _truncate_schema(current_schema, max_regression_depth)
    else:
        cmp_existing = existing
        cmp_current  = current_schema

    regressions, additions = diff_schemas(cmp_existing, cmp_current)

    # Auto-merge new keys into the snapshot so they're included in future baselines.
    if additions:
        merged = _merge_schemas(existing, current_schema)
        save_snapshot(result.label, merged, result.mode)

    if regressions:
        return "\n".join(regressions), bool(additions)

    return None, bool(additions)


# ── defect detection ──────────────────────────────────────────────────────────

# Match both Linux   "thread '...' panicked at "
# and     macOS      "thread '...' (tid) panicked at "
PANIC_PATTERN = re.compile(r"panicked at ", re.IGNORECASE)
UNWRAP_PATTERN = re.compile(r"called `Option::unwrap\(\)` on a `None` value", re.IGNORECASE)
STACK_TRACE_PATTERN = re.compile(r"^\s+\d+:\s+0x[0-9a-f]", re.MULTILINE)
BACKTRACE_PATTERN = re.compile(r"stack backtrace:", re.IGNORECASE)
INTERNAL_ERROR_PATTERN = re.compile(r"internal error:|RUST_BACKTRACE", re.IGNORECASE)

AUTH_FAILURE_PATTERNS = [
    re.compile(p, re.IGNORECASE) for p in [
        r"401",
        r"403",
        r"authentication",
        r"not authenticated",
        r"no credentials",
        r"run `pup auth login`",
        r"invalid api key",
        r"forbidden",
        r"unauthorized",
        r"DD_API_KEY",
        r"DD_APP_KEY",
    ]
]


def is_auth_failure(text: str) -> bool:
    return any(p.search(text) for p in AUTH_FAILURE_PATTERNS)


# HTTP 4xx semantics used to classify auth_fail results.
_HTTP_STATUS_REASONS: list[tuple[re.Pattern, str]] = [
    (re.compile(r"\b400\b"), "400 Bad Request"),
    (re.compile(r"\b401\b"), "401 Unauthorized"),
    (re.compile(r"\b403\b"), "403 Forbidden"),
    (re.compile(r"\b404\b"), "404 Not Found"),
    (re.compile(r"\b429\b"), "429 Too Many Requests"),
    (re.compile(r"\b5\d\d\b"), "5xx Server Error"),
]


def classify_auth_reason(stdout: str, stderr: str) -> str:
    """
    Return a short human-readable reason for an auth/4xx failure.

    Scans stderr first (where pup prints API error messages), then stdout,
    and returns the first HTTP status match or a generic label.
    """
    combined = stderr + "\n" + stdout
    for pat, label in _HTTP_STATUS_REASONS:
        if pat.search(combined):
            return label
    # Fallback: generic auth description from pup's error text
    if re.search(r"no credentials|authentication required|set DD_API_KEY", combined, re.I):
        return "no credentials configured"
    if re.search(r"forbidden", combined, re.I):
        return "403 Forbidden"
    if re.search(r"unauthorized", combined, re.I):
        return "401 Unauthorized"
    return "auth error"


def check_defects(
    result: "TestResult",
    expect_json: bool,
    expect_exit: int | None = None,
) -> list[str]:
    defects = []
    # Rust runtime signals (panics, backtraces, internal errors) go to stderr.
    # Checking stdout for these patterns produces false positives when API
    # responses contain phrases like "internal error:" in their payload text
    # (e.g. events list describing a monitored system error).
    stderr_only = result.stderr
    combined = result.stdout + "\n" + result.stderr

    if PANIC_PATTERN.search(stderr_only):
        defects.append("PANIC detected in output")

    if UNWRAP_PATTERN.search(stderr_only):
        defects.append("unwrap() on None detected")

    if BACKTRACE_PATTERN.search(stderr_only) and STACK_TRACE_PATTERN.search(combined):
        defects.append("Rust stack trace in output")

    if INTERNAL_ERROR_PATTERN.search(stderr_only):
        defects.append("Internal error message in output")

    if expect_exit is not None and result.exit_code != expect_exit:
        defects.append(f"Unexpected exit code: got {result.exit_code}, expected {expect_exit}")

    if expect_json and result.exit_code == 0 and result.stdout.strip():
        try:
            parsed = json.loads(result.stdout.strip())
            if parsed is None:
                defects.append("JSON output is null on success")
        except json.JSONDecodeError as exc:
            if not is_auth_failure(result.stdout) and not is_auth_failure(result.stderr):
                defects.append(f"Invalid JSON output: {exc}")

    if (expect_exit is None or expect_exit == 0) and \
       result.exit_code == 0 and \
       not result.stdout.strip() and not result.stderr.strip():
        defects.append("Empty output on successful exit")

    return defects


# ── result dataclass ──────────────────────────────────────────────────────────

@dataclass
class TestResult:
    label: str
    args: list[str]
    category: str
    exit_code: int
    stdout: str
    stderr: str
    duration_ms: float
    expect_exit: int | None = None
    note: str = ""
    defects: list[str] = field(default_factory=list)
    regression: str | None = None
    snapshot_created: bool = False
    skipped: bool = False
    skip_reason: str = ""
    # Override computed status (used for synthetic untested/write rows)
    status_override: str | None = None
    # "human" | "agent" | "" (synthetic rows that were not run)
    mode: str = "human"
    # HTTP status classification when auth_fail (e.g. "401 Unauthorized")
    auth_reason: str = ""
    # Per-row diff: current stdout vs previous run's stdout (same mode)
    row_diff: tuple[str, str] = field(default_factory=lambda: ("", ""))

    @property
    def test_id(self) -> str:
        """Stable slug derived from the label and mode, usable as an HTML anchor."""
        base = re.sub(r"[^a-z0-9]+", "-", self.label.lower()).strip("-")
        return f"{base}-{self.mode}" if self.mode else base

    @property
    def status(self) -> str:
        if self.status_override:
            return self.status_override
        if self.skipped:
            return "skipped"
        if self.defects or self.regression:
            return "fail"
        if self.expect_exit is not None:
            return "pass" if self.exit_code == self.expect_exit else "fail"
        if self.exit_code != 0:
            return "auth_fail"
        return "pass"


# ── test catalog ──────────────────────────────────────────────────────────────

READ_COMMANDS: list[dict] = [
    # ── no-auth commands ──────────────────────────────────────────────────
    {
        "label": "version",
        "args": ["version"],
        "category": "no_auth",
        "expect_json": False,
    },
    {
        "label": "test",
        "args": ["test"],
        "category": "no_auth",
        "expect_json": False,
        "note": "Diagnostic command: shows configured site, API host, key presence, and output format.",
    },
    {
        "label": "agent schema",
        "args": ["agent", "schema"],
        "category": "no_auth",
        "expect_json": True,
    },
    {
        "label": "agent schema --compact",
        "args": ["agent", "schema", "--compact"],
        "category": "no_auth",
        "expect_json": True,
    },
    {
        "label": "agent guide",
        "args": ["agent", "guide"],
        "category": "no_auth",
        "expect_json": False,
    },
    {
        "label": "misc ip-ranges",
        "args": ["misc", "ip-ranges"],
        "category": "no_auth",
        "expect_json": True,
    },
    {
        "label": "misc status",
        "args": ["misc", "status"],
        "category": "no_auth",
        "expect_json": True,
    },
    {
        "label": "completions bash",
        "args": ["completions", "bash"],
        "category": "no_auth",
        "expect_json": False,
        "note": "BUG (debug build only): panics with clap debug_assert — "
                "'type_id' referenced in conflicts_with* does not exist. "
                "Release builds suppress debug_asserts and generate completions normally.",
    },
    {
        "label": "completions zsh",
        "args": ["completions", "zsh"],
        "category": "no_auth",
        "expect_json": False,
        "note": "BUG (debug build only): same clap debug_assert panic as 'completions bash'.",
    },
    # ── --help smoke tests ────────────────────────────────────────────────
    {
        "label": "help (root)",
        "args": ["--help"],
        "category": "no_auth",
        "expect_json": False,
        "expect_exit": 0,
    },
    {
        "label": "monitors --help",
        "args": ["monitors", "--help"],
        "category": "no_auth",
        "expect_json": False,
        "expect_exit": 0,
    },
    {
        "label": "logs --help",
        "args": ["logs", "--help"],
        "category": "no_auth",
        "expect_json": False,
        "expect_exit": 0,
    },
    # ── auth status ───────────────────────────────────────────────────────
    {
        "label": "auth status",
        "args": ["auth", "status"],
        "category": "auth_status",
        "expect_json": False,
    },
    # ── api-keys ──────────────────────────────────────────────────────────
    {
        "label": "api-keys list",
        "args": ["api-keys", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── app-keys ──────────────────────────────────────────────────────────
    {
        "label": "app-keys list",
        "args": ["app-keys", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── apm ───────────────────────────────────────────────────────────────
    {
        "label": "apm services list",
        "args": ["apm", "services", "list", "--env=prod", "--from=1h"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "apm services stats",
        "args": ["apm", "services", "stats", "--env=prod", "--from=1h"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "apm entities list",
        "args": ["apm", "entities", "list", "--env=prod", "--from=1h"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "apm dependencies list",
        "args": ["apm", "dependencies", "list", "--env=prod", "--from=1h"],
        "category": "auth_required",
        "expect_json": True,
        "skip_regression": True,
        # The service dependency map has 6000+ service names as top-level dict keys.
        # They are a mix of hyphenated, underscore_separated, dotted, and single-word
        # names — DYNAMIC_KEY_PATTERNS cannot collapse them all because _all_keys_dynamic
        # requires EVERY key to match at least one pattern, and single-word names like
        # "abacus" don't match any.  Service names appear/disappear between API calls,
        # making the snapshot inherently volatile.  skip_regression is required.
    },
    {
        "label": "apm flow-map",
        "args": ["apm", "flow-map", "--env=prod", "--query=*", "--from=1h"],
        "category": "auth_required",
        "expect_json": True,
        "note": "pup bug: --from is not propagated into the API query body; API returns 400 'missing parameter from in query'.",
    },
    # ── audit-logs ────────────────────────────────────────────────────────
    {
        "label": "audit-logs list",
        "args": ["audit-logs", "list", "--from=1h"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "audit-logs search",
        "args": ["audit-logs", "search", "--from=1h", "--query=*"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── cases ─────────────────────────────────────────────────────────────
    {
        "label": "cases projects list",
        "args": ["cases", "projects", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "cases search",
        "args": ["cases", "search", "--query=*"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── cicd ──────────────────────────────────────────────────────────────
    {
        "label": "cicd pipelines list",
        "args": ["cicd", "pipelines", "list", "--from=1h"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "cicd events search",
        "args": ["cicd", "events", "search", "--from=1h", "--query=*"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "cicd tests list",
        "args": ["cicd", "tests", "list", "--from=1h"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "cicd tests search",
        "args": ["cicd", "tests", "search", "--from=1h", "--query=*"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "cicd flaky-tests search",
        "args": ["cicd", "flaky-tests", "search", "--query=*"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── cloud ─────────────────────────────────────────────────────────────
    {
        "label": "cloud aws list",
        "args": ["cloud", "aws", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "cloud azure list",
        "args": ["cloud", "azure", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "cloud gcp list",
        "args": ["cloud", "gcp", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "cloud oci tenancies list",
        "args": ["cloud", "oci", "tenancies", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "cloud oci products list",
        "args": ["cloud", "oci", "products", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── dashboards ────────────────────────────────────────────────────────
    {
        "label": "dashboards list",
        "args": ["dashboards", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── data-governance ───────────────────────────────────────────────────
    {
        "label": "data-governance scanner rules list",
        "args": ["data-governance", "scanner", "rules"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── downtime ──────────────────────────────────────────────────────────
    {
        "label": "downtime list",
        "args": ["downtime", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── error-tracking ────────────────────────────────────────────────────
    {
        "label": "error-tracking issues search",
        "args": ["error-tracking", "issues", "search", "--from=1h", "--query=*"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── events ────────────────────────────────────────────────────────────
    {
        "label": "events list",
        "args": ["events", "list", "--from=1h"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "events search",
        "args": ["events", "search", "--from=1h", "--query=*"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── fleet ─────────────────────────────────────────────────────────────
    {
        "label": "fleet agents list",
        "args": ["fleet", "agents", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "fleet agents versions",
        "args": ["fleet", "agents", "versions"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "fleet deployments list",
        "args": ["fleet", "deployments", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "fleet schedules list",
        "args": ["fleet", "schedules", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── incidents ─────────────────────────────────────────────────────────
    {
        "label": "incidents list",
        "args": ["incidents", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "incidents handles list",
        "args": ["incidents", "handles", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "incidents postmortem-templates list",
        "args": ["incidents", "postmortem-templates", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "incidents settings get",
        "args": ["incidents", "settings", "get"],
        "category": "auth_required",
        "expect_json": True,
        "note": "pup bug: serde deserialization error — missing field `analytics_dashboard_id` in API response model.",
    },
    # ── infrastructure ────────────────────────────────────────────────────
    {
        "label": "infrastructure hosts list",
        "args": ["infrastructure", "hosts", "list"],
        "category": "auth_required",
        "expect_json": True,
        "max_regression_depth": 2,
        "note": "Per-host fields like aws_id/aws_name are optional (only on AWS hosts) and vary per run; depth-2 checks root envelope and that host_list is a populated list without inspecting per-host fields.",
    },
    # ── integrations ──────────────────────────────────────────────────────
    {
        "label": "integrations jira accounts",
        "args": ["integrations", "jira", "accounts"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "integrations jira accounts list",
        "args": ["integrations", "jira", "accounts", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "integrations jira templates list",
        "args": ["integrations", "jira", "templates", "list"],
        "category": "auth_required",
        "expect_json": True,
        "note": "pup bug: serde deserialization error — missing field `attributes` in Jira template response model.",
    },
    {
        "label": "integrations pagerduty list",
        "args": ["integrations", "pagerduty", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "integrations servicenow assignment-groups list",
        "args": ["integrations", "servicenow", "assignment-groups", "list"],
        "category": "auth_required",
        "expect_json": True,
        "note": "pup bug: validates INSTANCE_NAME positional arg as UUID; real instance names like 'dev186409' are rejected.",
    },
    {
        "label": "integrations servicenow business-services list",
        "args": ["integrations", "servicenow", "business-services", "list"],
        "category": "auth_required",
        "expect_json": True,
        "note": "pup bug: same UUID validation bug as assignment-groups list.",
    },
    {
        "label": "integrations servicenow instances list",
        "args": ["integrations", "servicenow", "instances", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "integrations servicenow templates list",
        "args": ["integrations", "servicenow", "templates", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "integrations servicenow users list",
        "args": ["integrations", "servicenow", "users", "list"],
        "category": "auth_required",
        "expect_json": True,
        "note": "pup bug: same UUID validation bug as assignment-groups list.",
    },
    {
        "label": "integrations slack list",
        "args": ["integrations", "slack", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "integrations webhooks list",
        "args": ["integrations", "webhooks", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── hamr ──────────────────────────────────────────────────────────────
    {
        "label": "hamr connections get",
        "args": ["hamr", "connections", "get"],
        "category": "auth_required",
        "expect_json": True,
        "note": "pup bug: serde error 'invalid type: null, expected a mapping' — HAMR API may return null for connection config.",
    },
    # ── investigations ────────────────────────────────────────────────────
    {
        "label": "investigations list",
        "args": ["investigations", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── logs ──────────────────────────────────────────────────────────────
    {
        "label": "logs archives list",
        "args": ["logs", "archives", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "logs custom-destinations list",
        "args": ["logs", "custom-destinations", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "logs list",
        "args": ["logs", "list", "--from=1h", "--query=*"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "logs metrics list",
        "args": ["logs", "metrics", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "logs restriction-queries list",
        "args": ["logs", "restriction-queries", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "logs query",
        "args": ["logs", "query", "--query=*", "--from=1h"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "logs search",
        "args": ["logs", "search", "--from=1h", "--query=*"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── metrics ───────────────────────────────────────────────────────────
    {
        "label": "metrics list",
        "args": ["metrics", "list", "--filter=system.cpu"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "metrics query",
        "args": ["metrics", "query", "--query=avg:system.cpu.user{*}", "--from=1h"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "metrics search",
        "args": ["metrics", "search", "--query=system.cpu"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "metrics tags list",
        "args": ["metrics", "tags", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "metrics metadata get",
        "args": ["metrics", "metadata", "get", "system.cpu.user"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── monitors ──────────────────────────────────────────────────────────
    {
        "label": "monitors list",
        "args": ["monitors", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "monitors search",
        "args": ["monitors", "search", "--query=*"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── network ───────────────────────────────────────────────────────────
    {
        "label": "network list",
        "args": ["network", "list"],
        "category": "auth_required",
        "expect_json": False,
        "expect_exit": 1,
        "note": "pup stub: 'network commands are not yet implemented'. Tracked here to detect when it ships.",
    },
    {
        "label": "network devices list",
        "args": ["network", "devices", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "network flows list",
        "args": ["network", "flows", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── notebooks ─────────────────────────────────────────────────────────
    {
        "label": "notebooks list",
        "args": ["notebooks", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── obs-pipelines ─────────────────────────────────────────────────────
    {
        "label": "obs-pipelines list",
        "args": ["obs-pipelines", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── on-call ───────────────────────────────────────────────────────────
    {
        "label": "on-call teams list",
        "args": ["on-call", "teams", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── organizations ─────────────────────────────────────────────────────
    {
        "label": "organizations list",
        "args": ["organizations", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "organizations get",
        "args": ["organizations", "get"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── rum ───────────────────────────────────────────────────────────────
    {
        "label": "rum apps list",
        "args": ["rum", "apps", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "rum events",
        "args": ["rum", "events", "--from=1h"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "rum heatmaps query",
        "args": ["rum", "heatmaps", "query", "--from=1h"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "rum metrics list",
        "args": ["rum", "metrics", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "rum playlists list",
        "args": ["rum", "playlists", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "rum retention-filters list",
        "args": ["rum", "retention-filters", "list", "ac8218cf-498b-4d33-bd44-151095959547"],
        "category": "auth_required",
        "expect_json": True,
        "note": "Requires a RUM app ID; using the org's primary browser app.",
    },
    {
        "label": "rum sessions list",
        "args": ["rum", "sessions", "list", "--from=1h"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "rum sessions search",
        "args": ["rum", "sessions", "search", "--from=1h", "--query=*"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── scorecards ────────────────────────────────────────────────────────
    {
        "label": "scorecards list",
        "args": ["scorecards", "list"],
        "category": "auth_required",
        "expect_json": False,
        "expect_exit": 1,
        "note": "pup stub: 'scorecards commands are not yet implemented'. Tracked here to detect when it ships.",
    },
    # ── security ──────────────────────────────────────────────────────────
    {
        "label": "security content-packs list",
        "args": ["security", "content-packs", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "security findings search",
        "args": ["security", "findings", "search", "--query=*"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "security rules list",
        "args": ["security", "rules", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "security signals list",
        "args": ["security", "signals", "list", "--query=*", "--from=1h"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "security risk-scores list",
        "args": ["security", "risk-scores", "list"],
        "category": "auth_required",
        "expect_json": True,
        "note": "pup bug: serde deserialization error — missing field `entity_id` in risk score response model.",
    },
    # ── service-catalog ───────────────────────────────────────────────────
    {
        "label": "service-catalog list",
        "args": ["service-catalog", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── slos ──────────────────────────────────────────────────────────────
    {
        "label": "slos list",
        "args": ["slos", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── static-analysis ───────────────────────────────────────────────────
    {
        "label": "static-analysis ast list",
        "args": ["static-analysis", "ast", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "static-analysis coverage list",
        "args": ["static-analysis", "coverage", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "static-analysis sca list",
        "args": ["static-analysis", "sca", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "static-analysis custom-rulesets list",
        "args": ["static-analysis", "custom-rulesets", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── status-pages ──────────────────────────────────────────────────────
    {
        "label": "status-pages pages list",
        "args": ["status-pages", "pages", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "status-pages degradations list",
        "args": ["status-pages", "degradations", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "status-pages third-party list",
        "args": ["status-pages", "third-party", "list"],
        "category": "auth_required",
        "expect_json": True,
        "max_regression_depth": 3,
        "note": "Live third-party outage data; depth-3 check confirms provider structure without inspecting volatile outage fields.",
    },
    # ── synthetics ────────────────────────────────────────────────────────
    {
        "label": "synthetics locations list",
        "args": ["synthetics", "locations", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "synthetics tests list",
        "args": ["synthetics", "tests", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "synthetics suites list",
        "args": ["synthetics", "suites", "list", "--query=*"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "synthetics tests search",
        "args": ["synthetics", "tests", "search"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── tags ──────────────────────────────────────────────────────────────
    {
        "label": "tags list",
        "args": ["tags", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── test-optimization ────────────────────────────────────────────────
    {
        "label": "test-optimization flaky-tests search",
        "args": ["test-optimization", "flaky-tests", "search"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── traces ────────────────────────────────────────────────────────────
    {
        "label": "traces search",
        "args": ["traces", "search", "--from=1h", "--query=*"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── usage ─────────────────────────────────────────────────────────────
    {
        "label": "usage hourly",
        "args": ["usage", "hourly", "--from=1h"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "usage summary",
        "args": ["usage", "summary", "--from=1h"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── users ─────────────────────────────────────────────────────────────
    {
        "label": "users list",
        "args": ["users", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "users roles list",
        "args": ["users", "roles", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── alias ─────────────────────────────────────────────────────────────
    {
        "label": "alias list",
        "args": ["alias", "list"],
        "category": "no_auth",
        "expect_json": True,
    },
    # ── costs ─────────────────────────────────────────────────────────────
    {
        "label": "costs datadog projected",
        "args": ["costs", "datadog", "projected"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── Previously untested read-only commands ──────────────────────────────
    {
        "label": "apm services operations",
        "args": ["apm", "services", "operations", "--env=prod", "--service=nginx", "--from=1h"],
        "category": "auth_required",
        "expect_json": True,
        "skip_regression": True,
        "note": "Uses nginx as a sentinel service name; may return empty list or 404 in some orgs.",
    },
    {
        "label": "apm services resources",
        "args": ["apm", "services", "resources", "--env=prod", "--service=nginx", "--name=http.request", "--from=1h"],
        "category": "auth_required",
        "expect_json": True,
        "skip_regression": True,
        "note": "Uses nginx as a sentinel service name; may return empty list or 404 in some orgs.",
    },
    {
        "label": "cicd events aggregate",
        "args": ["cicd", "events", "aggregate", "--query=*", "--from=1h"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "cicd tests aggregate",
        "args": ["cicd", "tests", "aggregate", "--query=*", "--from=1h"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "costs datadog attribution",
        "args": ["costs", "datadog", "attribution", "--start=2024-01"],
        "category": "auth_required",
        "expect_json": True,
        "timeout": 90,
        "note": "pup bug: --start is routed through the generic relative-time parser instead of accepting YYYY-MM format; exits with parse error.",
    },
    {
        "label": "costs datadog by-org",
        "args": ["costs", "datadog", "by-org", "--start-month=2024-01"],
        "category": "auth_required",
        "expect_json": True,
        "timeout": 90,
        "note": "pup bug: --start-month is routed through the generic relative-time parser instead of accepting YYYY-MM format; exits with parse error.",
    },
    {
        "label": "logs aggregate",
        "args": ["logs", "aggregate", "--from=1h"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "security rules bulk-export",
        "args": ["security", "rules", "bulk-export"],
        "category": "auth_required",
        "expect_json": False,
        "note": "Returns a binary/zip export, not JSON.",
    },
    {
        "label": "traces aggregate",
        "args": ["traces", "aggregate", "--compute=count", "--from=1h"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── Newly added: no-ID-required read-only commands ─────────────────────
    # ── apm troubleshooting ────────────────────────────────────────────────
    {
        "label": "apm troubleshooting list",
        "args": ["apm", "troubleshooting", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── app-builder ────────────────────────────────────────────────────────
    {
        "label": "app-builder list",
        "args": ["app-builder", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── auth ──────────────────────────────────────────────────────────────
    {
        "label": "auth list",
        "args": ["auth", "list"],
        "category": "no_auth",
        "expect_json": True,
        "note": "Lists configured org sessions; no API call required.",
    },
    # ── containers ────────────────────────────────────────────────────────
    {
        "label": "containers list",
        "args": ["containers", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "containers images list",
        "args": ["containers", "images", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── costs cloud configs ────────────────────────────────────────────────
    {
        "label": "costs datadog aws-config list",
        "args": ["costs", "datadog", "aws-config", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "costs datadog azure-config list",
        "args": ["costs", "datadog", "azure-config", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "costs datadog gcp-config list",
        "args": ["costs", "datadog", "gcp-config", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── ddsql ─────────────────────────────────────────────────────────────
    {
        "label": "ddsql table",
        "args": ["ddsql", "table", "--query=SELECT 1"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── integrations (new) ────────────────────────────────────────────────
    {
        "label": "integrations list",
        "args": ["integrations", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "integrations aws cloud-auth persona-mappings list",
        "args": ["integrations", "aws", "cloud-auth", "persona-mappings", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "integrations google-chat handles list",
        "args": ["integrations", "google-chat", "handles", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── llm-obs ───────────────────────────────────────────────────────────
    {
        "label": "llm-obs datasets list",
        "args": ["llm-obs", "datasets", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "llm-obs experiments list",
        "args": ["llm-obs", "experiments", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "llm-obs projects list",
        "args": ["llm-obs", "projects", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "llm-obs spans search",
        "args": ["llm-obs", "spans", "search", "--from=1h", "--query=*"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── reference-tables ──────────────────────────────────────────────────
    {
        "label": "reference-tables list",
        "args": ["reference-tables", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── security suppressions ─────────────────────────────────────────────
    {
        "label": "security suppressions list",
        "args": ["security", "suppressions", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── skills ────────────────────────────────────────────────────────────
    {
        "label": "skills list",
        "args": ["skills", "list"],
        "category": "no_auth",
        "expect_json": True,
    },
    {
        "label": "skills path",
        "args": ["skills", "path"],
        "category": "no_auth",
        "expect_json": False,
    },
    # ── status-pages (new) ────────────────────────────────────────────────
    {
        "label": "status-pages maintenances list",
        "args": ["status-pages", "maintenances", "list"],
        "category": "auth_required",
        "expect_json": True,
    },
    # ── Newly added: ID-dependent commands (resolved via id_from) ──────────
    # ── api-keys get ──────────────────────────────────────────────────────
    {
        "label": "api-keys get",
        "args": ["api-keys", "get", "{id}"],
        "id_from": {"args": ["api-keys", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    # ── app-builder get ───────────────────────────────────────────────────
    {
        "label": "app-builder get",
        "args": ["app-builder", "get", "{id}"],
        "id_from": {"args": ["app-builder", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    # ── app-keys get ──────────────────────────────────────────────────────
    {
        "label": "app-keys get",
        "args": ["app-keys", "get", "{id}"],
        "id_from": {"args": ["app-keys", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    # ── cases get ─────────────────────────────────────────────────────────
    {
        "label": "cases get",
        "args": ["cases", "get", "{id}"],
        "id_from": {"args": ["cases", "search", "--query=*"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "cases projects get",
        "args": ["cases", "projects", "get", "{id}"],
        "id_from": {"args": ["cases", "projects", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "cases projects notification-rules list",
        "args": ["cases", "projects", "notification-rules", "list", "{id}"],
        "id_from": {"args": ["cases", "projects", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    # ── cicd pipelines get ────────────────────────────────────────────────
    {
        "label": "cicd pipelines get",
        "args": ["cicd", "pipelines", "get", "{id}"],
        "id_from": {"args": ["cicd", "pipelines", "list", "--from=1h"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    # ── cloud oci tenancies get ───────────────────────────────────────────
    {
        "label": "cloud oci tenancies get",
        "args": ["cloud", "oci", "tenancies", "get", "{id}"],
        "id_from": {"args": ["cloud", "oci", "tenancies", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    # ── costs cloud config gets ───────────────────────────────────────────
    {
        "label": "costs datadog aws-config get",
        "args": ["costs", "datadog", "aws-config", "get", "{id}"],
        "id_from": {"args": ["costs", "datadog", "aws-config", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "costs datadog azure-config get",
        "args": ["costs", "datadog", "azure-config", "get", "{id}"],
        "id_from": {"args": ["costs", "datadog", "azure-config", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "costs datadog gcp-config get",
        "args": ["costs", "datadog", "gcp-config", "get", "{id}"],
        "id_from": {"args": ["costs", "datadog", "gcp-config", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    # ── dashboards get ────────────────────────────────────────────────────
    {
        "label": "dashboards get",
        "args": ["dashboards", "get", "{id}"],
        "id_from": {"args": ["dashboards", "list"], "json_path": ["dashboards", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    # ── downtime get ──────────────────────────────────────────────────────
    {
        "label": "downtime get",
        "args": ["downtime", "get", "{id}"],
        "id_from": {"args": ["downtime", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    # ── error-tracking issues get ─────────────────────────────────────────
    {
        "label": "error-tracking issues get",
        "args": ["error-tracking", "issues", "get", "{id}"],
        "id_from": {
            "args": ["error-tracking", "issues", "search", "--from=1h", "--query=*"],
            "json_path": ["data", 0, "id"],
        },
        "category": "auth_required",
        "expect_json": True,
    },
    # ── events get ────────────────────────────────────────────────────────
    {
        "label": "events get",
        "args": ["events", "get", "{id}"],
        "id_from": {"args": ["events", "list", "--from=1h"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    # ── fleet get commands ────────────────────────────────────────────────
    {
        "label": "fleet agents get",
        "args": ["fleet", "agents", "get", "{id}"],
        "id_from": {"args": ["fleet", "agents", "list"], "json_path": ["data", "id"]},
        "category": "auth_required",
        "expect_json": True,
        "note": "fleet agents list returns a single agent object (data is a dict, not a list).",
    },
    {
        "label": "fleet deployments get",
        "args": ["fleet", "deployments", "get", "{id}"],
        "id_from": {"args": ["fleet", "deployments", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "fleet schedules get",
        "args": ["fleet", "schedules", "get", "{id}"],
        "id_from": {"args": ["fleet", "schedules", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    # ── incidents get commands ────────────────────────────────────────────
    {
        "label": "incidents get",
        "args": ["incidents", "get", "{id}"],
        "id_from": {"args": ["incidents", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "incidents attachments list",
        "args": ["incidents", "attachments", "list", "{id}"],
        "id_from": {"args": ["incidents", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "incidents postmortem-templates get",
        "args": ["incidents", "postmortem-templates", "get", "{id}"],
        "id_from": {
            "args": ["incidents", "postmortem-templates", "list"],
            "json_path": ["data", 0, "id"],
        },
        "category": "auth_required",
        "expect_json": True,
    },
    # ── infrastructure hosts get ──────────────────────────────────────────
    {
        "label": "infrastructure hosts get",
        "args": ["infrastructure", "hosts", "get", "{id}"],
        "id_from": {"args": ["infrastructure", "hosts", "list"], "json_path": ["host_list", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    # ── integrations get commands ─────────────────────────────────────────
    {
        "label": "integrations aws cloud-auth persona-mappings get",
        "args": ["integrations", "aws", "cloud-auth", "persona-mappings", "get", "{id}"],
        "id_from": {
            "args": ["integrations", "aws", "cloud-auth", "persona-mappings", "list"],
            "json_path": ["data", 0, "id"],
        },
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "integrations google-chat handles get",
        "args": ["integrations", "google-chat", "handles", "get", "{id}"],
        "id_from": {
            "args": ["integrations", "google-chat", "handles", "list"],
            "json_path": ["data", 0, "id"],
        },
        "category": "auth_required",
        "expect_json": True,
    },
    # ── investigations get ────────────────────────────────────────────────
    {
        "label": "investigations get",
        "args": ["investigations", "get", "{id}"],
        "id_from": {"args": ["investigations", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    # ── llm-obs experiment commands ───────────────────────────────────────
    {
        "label": "llm-obs experiments dimension-values",
        "args": ["llm-obs", "experiments", "dimension-values", "{id}"],
        "id_from": {"args": ["llm-obs", "experiments", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "llm-obs experiments events list",
        "args": ["llm-obs", "experiments", "events", "list", "{id}"],
        "id_from": {"args": ["llm-obs", "experiments", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "llm-obs experiments summary",
        "args": ["llm-obs", "experiments", "summary", "{id}"],
        "id_from": {"args": ["llm-obs", "experiments", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "llm-obs experiments metric-values",
        "args": ["llm-obs", "experiments", "metric-values", "{id}"],
        "id_from": {"args": ["llm-obs", "experiments", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    # ── logs get commands ─────────────────────────────────────────────────
    {
        "label": "logs archives get",
        "args": ["logs", "archives", "get", "{id}"],
        "id_from": {"args": ["logs", "archives", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "logs custom-destinations get",
        "args": ["logs", "custom-destinations", "get", "{id}"],
        "id_from": {"args": ["logs", "custom-destinations", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "logs metrics get",
        "args": ["logs", "metrics", "get", "{id}"],
        "id_from": {"args": ["logs", "metrics", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "logs restriction-queries get",
        "args": ["logs", "restriction-queries", "get", "{id}"],
        "id_from": {"args": ["logs", "restriction-queries", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    # ── monitors get ──────────────────────────────────────────────────────
    {
        "label": "monitors get",
        "args": ["monitors", "get", "{id}"],
        "id_from": {"args": ["monitors", "list"], "json_path": [0, "id"]},
        "category": "auth_required",
        "expect_json": True,
        "note": "monitors list returns a raw JSON array (not wrapped in data:); json_path indexes directly into the list.",
    },
    # ── network device commands ───────────────────────────────────────────
    {
        "label": "network devices get",
        "args": ["network", "devices", "get", "{id}"],
        "id_from": {"args": ["network", "devices", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "network devices interfaces",
        "args": ["network", "devices", "interfaces", "{id}"],
        "id_from": {"args": ["network", "devices", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "network devices tags list",
        "args": ["network", "devices", "tags", "list", "{id}"],
        "id_from": {"args": ["network", "devices", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    # ── network interfaces list ───────────────────────────────────────────
    {
        "label": "network interfaces list",
        "args": ["network", "interfaces", "list", "{id}"],
        "id_from": {"args": ["network", "devices", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    # ── notebooks get ─────────────────────────────────────────────────────
    {
        "label": "notebooks get",
        "args": ["notebooks", "get", "{id}"],
        "id_from": {"args": ["notebooks", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    # ── obs-pipelines get ─────────────────────────────────────────────────
    {
        "label": "obs-pipelines get",
        "args": ["obs-pipelines", "get", "{id}"],
        "id_from": {"args": ["obs-pipelines", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    # ── on-call commands ──────────────────────────────────────────────────
    {
        "label": "on-call teams get",
        "args": ["on-call", "teams", "get", "{id}"],
        "id_from": {"args": ["on-call", "teams", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "on-call teams memberships list",
        "args": ["on-call", "teams", "memberships", "list", "{id}"],
        "id_from": {"args": ["on-call", "teams", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    # ── reference-tables get ──────────────────────────────────────────────
    {
        "label": "reference-tables get",
        "args": ["reference-tables", "get", "{id}"],
        "id_from": {"args": ["reference-tables", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    # ── rum get commands ──────────────────────────────────────────────────
    {
        "label": "rum apps get",
        "args": ["rum", "apps", "get", "{id}"],
        "id_from": {"args": ["rum", "apps", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "rum metrics get",
        "args": ["rum", "metrics", "get", "{id}"],
        "id_from": {"args": ["rum", "metrics", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "rum playlists get",
        "args": ["rum", "playlists", "get", "{id}"],
        "id_from": {"args": ["rum", "playlists", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "rum retention-filters get",
        "args": ["rum", "retention-filters", "get", "ac8218cf-498b-4d33-bd44-151095959547", "{id}"],
        "id_from": {
            "args": ["rum", "retention-filters", "list", "ac8218cf-498b-4d33-bd44-151095959547"],
            "json_path": ["data", 0, "id"],
        },
        "category": "auth_required",
        "expect_json": True,
        "note": "Uses the org's primary browser RUM app ID (same hardcoded ID as retention-filters list).",
    },
    # ── security get commands ─────────────────────────────────────────────
    {
        "label": "security rules get",
        "args": ["security", "rules", "get", "{id}"],
        "id_from": {"args": ["security", "rules", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "security suppressions get",
        "args": ["security", "suppressions", "get", "{id}"],
        "id_from": {"args": ["security", "suppressions", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    # ── service-catalog get ───────────────────────────────────────────────
    {
        "label": "service-catalog get",
        "args": ["service-catalog", "get", "{id}"],
        "id_from": {
            "args": ["service-catalog", "list"],
            "json_path": ["data", 0, "attributes", "schema", "dd-service"],
        },
        "category": "auth_required",
        "expect_json": True,
        "skip_regression": True,
        "note": "Uses dd-service name from the first catalog entry; may vary across orgs.",
    },
    # ── slos get/status ───────────────────────────────────────────────────
    {
        "label": "slos get",
        "args": ["slos", "get", "{id}"],
        "id_from": {"args": ["slos", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "slos status",
        "args": ["slos", "status", "--from=1h", "--to=now", "{id}"],
        "id_from": {"args": ["slos", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    # ── static-analysis get commands ──────────────────────────────────────
    {
        "label": "static-analysis ast get",
        "args": ["static-analysis", "ast", "get", "{id}"],
        "id_from": {"args": ["static-analysis", "ast", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "static-analysis coverage get",
        "args": ["static-analysis", "coverage", "get", "{id}"],
        "id_from": {"args": ["static-analysis", "coverage", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "static-analysis custom-rulesets get",
        "args": ["static-analysis", "custom-rulesets", "get", "{id}"],
        "id_from": {
            "args": ["static-analysis", "custom-rulesets", "list"],
            "json_path": ["data", 0, "id"],
        },
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "static-analysis sca get",
        "args": ["static-analysis", "sca", "get", "{id}"],
        "id_from": {"args": ["static-analysis", "sca", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    # ── status-pages get commands ─────────────────────────────────────────
    {
        "label": "status-pages pages get",
        "args": ["status-pages", "pages", "get", "{id}"],
        "id_from": {"args": ["status-pages", "pages", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "status-pages components list",
        "args": ["status-pages", "components", "list", "{id}"],
        "id_from": {"args": ["status-pages", "pages", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "status-pages components get",
        "args": ["status-pages", "components", "get", "{id}"],
        "id_from": {"args": ["status-pages", "degradations", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
        "note": "Uses a degradation/component ID from degradations list; skip if list is empty.",
    },
    {
        "label": "status-pages degradations get",
        "args": ["status-pages", "degradations", "get", "{id}"],
        "id_from": {"args": ["status-pages", "degradations", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "status-pages maintenances get",
        "args": ["status-pages", "maintenances", "get", "{id}"],
        "id_from": {"args": ["status-pages", "maintenances", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    # ── synthetics get commands ───────────────────────────────────────────
    {
        "label": "synthetics suites get",
        "args": ["synthetics", "suites", "get", "{id}"],
        "id_from": {"args": ["synthetics", "suites", "list", "--query=*"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    {
        "label": "synthetics tests get",
        "args": ["synthetics", "tests", "get", "{id}"],
        "id_from": {"args": ["synthetics", "tests", "list"], "json_path": ["tests", 0, "public_id"]},
        "category": "auth_required",
        "expect_json": True,
    },
    # ── users get ─────────────────────────────────────────────────────────
    {
        "label": "users get",
        "args": ["users", "get", "{id}"],
        "id_from": {"args": ["users", "list"], "json_path": ["data", 0, "id"]},
        "category": "auth_required",
        "expect_json": True,
    },
]


# ── dd-auth credential injection ──────────────────────────────────────────────

def fetch_dd_auth_env(domain: str) -> dict[str, str]:
    """
    Run `dd-auth --domain DOMAIN -o` and parse its KEY=value output.

    Returns a dict of env vars (DD_API_KEY, DD_APP_KEY, DD_SITE, ...) or an
    empty dict if dd-auth is not available or returns an error.
    """
    cmd = ["dd-auth"]
    if domain:
        cmd += ["--domain", domain]
    cmd += ["-o"]

    print(f"  Running: {' '.join(cmd)}")
    try:
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=30,
        )
    except FileNotFoundError:
        print("  ⚠  dd-auth not found in PATH — skipping credential injection")
        return {}
    except subprocess.TimeoutExpired:
        print("  ⚠  dd-auth timed out — skipping credential injection")
        return {}

    if result.returncode != 0:
        print(f"  ⚠  dd-auth exited {result.returncode}: {result.stderr.strip()}")
        return {}

    env_vars: dict[str, str] = {}
    for line in result.stdout.splitlines():
        line = line.strip()
        if "=" in line and not line.startswith("#"):
            key, _, val = line.partition("=")
            key = key.strip()
            val = val.strip()
            if key:
                env_vars[key] = val
                print(f"    {key}={'*' * min(len(val), 8)}…")

    return env_vars


# ── pup auth management ───────────────────────────────────────────────────────

def _do_login(subprocess_env: dict[str, str], timeout: int = 120) -> tuple[bool, str]:
    """
    Run `pup auth login` interactively (terminal connected so user can complete
    the browser flow).  Returns (success, description).
    """
    print(f"\n  Running: pup auth login  (complete the browser flow within {timeout}s…)")
    try:
        proc = subprocess.run(
            [str(BINARY), "auth", "login"],
            env=subprocess_env,
            timeout=timeout,
            cwd=REPO_ROOT,
        )
        if proc.returncode == 0:
            print("  ✓ pup auth login succeeded")
            return True, "pup auth login: succeeded"
        print(f"  ⚠  pup auth login exited {proc.returncode}")
        return False, f"pup auth login: failed (exit {proc.returncode})"
    except subprocess.TimeoutExpired:
        print(f"  ⚠  pup auth login timed out after {timeout}s")
        return False, f"pup auth login: timed out after {timeout}s"
    except Exception as exc:
        print(f"  ⚠  pup auth login error: {exc}")
        return False, f"pup auth login: error ({exc})"


def ensure_auth(subprocess_env: dict[str, str], login_timeout: int = 120) -> tuple[bool, str]:
    """
    Ensure a valid OAuth2 token is available.

    Strategy:
      1. Run `pup auth status` — parse JSON to check if already authenticated.
      2. If authenticated, run `pup auth refresh` to freshen the token.
         If refresh succeeds, done.
      3. If not authenticated or refresh failed, fall back to `pup auth login`.

    Returns (success: bool, description: str).
    """
    # Step 1: check current auth status
    print("  Checking auth status (pup auth status)…", flush=True)
    try:
        status_proc = subprocess.run(
            [str(BINARY), "auth", "status"],
            capture_output=True, text=True, timeout=15,
            env=subprocess_env, cwd=REPO_ROOT,
        )
    except subprocess.TimeoutExpired:
        print("  ⚠  pup auth status timed out — will attempt login")
        return _do_login(subprocess_env, login_timeout)
    except Exception as exc:
        print(f"  ⚠  pup auth status error: {exc} — will attempt login")
        return _do_login(subprocess_env, login_timeout)

    authenticated = False
    if status_proc.returncode == 0 and status_proc.stdout.strip():
        try:
            status_data = json.loads(status_proc.stdout)
            # Output may be {"status":"success","data":{...}} or raw {...}
            inner = status_data.get("data", status_data)
            authenticated = bool(inner.get("authenticated", False))
        except (json.JSONDecodeError, AttributeError):
            pass

    if authenticated:
        # Step 2: try to refresh the existing token
        print("  Token present — attempting refresh (pup auth refresh)…", flush=True)
        try:
            refresh_proc = subprocess.run(
                [str(BINARY), "auth", "refresh"],
                capture_output=True, text=True, timeout=30,
                env=subprocess_env, cwd=REPO_ROOT,
            )
        except subprocess.TimeoutExpired:
            print("  ⚠  pup auth refresh timed out — will login")
            return _do_login(subprocess_env, login_timeout)
        except Exception as exc:
            print(f"  ⚠  pup auth refresh error: {exc} — will login")
            return _do_login(subprocess_env, login_timeout)

        if refresh_proc.returncode == 0:
            print("  ✓ Token refreshed successfully")
            return True, "pup auth refresh: succeeded"

        err = (refresh_proc.stderr.strip() or refresh_proc.stdout.strip())[:80]
        print(f"  ⚠  pup auth refresh failed (exit {refresh_proc.returncode}): {err} — will login")
    else:
        print("  No valid token found — will attempt login")

    # Step 3: fall back to interactive login
    return _do_login(subprocess_env, login_timeout)


# ── subprocess environment ────────────────────────────────────────────────────

def build_clean_env(dd_vars: dict[str, str], agent_mode: bool = False) -> dict[str, str]:
    """
    Build a minimal, reproducible subprocess environment containing only:
      - HOME and PATH from the outer environment (for binary execution and
        file-based token storage at ~/.config/pup/)
      - DD_TOKEN_STORAGE=file (prevents macOS keychain prompts)
      - All DD_* variables from the outer environment
      - All variables supplied via dd_vars (override outer DD_* vars)
      - FORCE_AGENT_MODE=1 when agent_mode=True

    No other outer-environment variables are forwarded, ensuring test
    reproducibility and preventing accidental credential leakage.
    """
    env: dict[str, str] = {}
    # Minimal shell context required by the binary
    for key in ("HOME", "PATH"):
        if key in os.environ:
            env[key] = os.environ[key]
    # File-based token storage — avoids interactive keychain dialogs
    env["DD_TOKEN_STORAGE"] = "file"
    # Forward all DD_* variables from the outer environment
    for k, v in os.environ.items():
        if k.startswith("DD_"):
            env[k] = v
    # Apply dd-auth / caller-supplied vars (highest priority)
    env.update(dd_vars)
    # Explicitly control agent mode — never leak it from the outer environment
    if agent_mode:
        env["FORCE_AGENT_MODE"] = "1"
    else:
        env.pop("FORCE_AGENT_MODE", None)
    return env


# ── runner ────────────────────────────────────────────────────────────────────

def run_command(
    binary: Path,
    args: list[str],
    timeout: int,
    env: dict[str, str],
) -> tuple[int, str, str, float]:
    """
    Run a single pup command and return (exit_code, stdout, stderr, duration_ms).

    Uses start_new_session=True so the child runs in its own process group.
    On timeout, the entire process group is killed via os.killpg(SIGKILL) to
    ensure tokio async threads (which hold stdout/stderr file descriptors) are
    also reaped — preventing the Python process from hanging in communicate().
    """
    cmd = [str(binary)] + args
    start = time.monotonic()
    proc = None
    try:
        proc = subprocess.Popen(
            cmd,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            cwd=REPO_ROOT,
            env=env,
            start_new_session=True,   # own process group for reliable killpg
        )
        stdout, stderr = proc.communicate(timeout=timeout)
        elapsed = (time.monotonic() - start) * 1000
        return proc.returncode, stdout, stderr, elapsed
    except subprocess.TimeoutExpired:
        if proc is not None:
            try:
                os.killpg(os.getpgid(proc.pid), signal.SIGKILL)
            except (ProcessLookupError, OSError):
                proc.kill()
            try:
                proc.communicate(timeout=2)
            except Exception:
                pass
        elapsed = (time.monotonic() - start) * 1000
        return -1, "", f"TIMEOUT after {timeout}s", elapsed
    except Exception as exc:
        elapsed = (time.monotonic() - start) * 1000
        return -2, "", f"RUNNER ERROR: {exc}", elapsed


# ── dynamic ID resolution ─────────────────────────────────────────────────────

def resolve_id(
    id_from: dict,
    env: dict[str, str],
    cache: dict,
    timeout: int,
) -> str | None:
    """
    Run a helper pup command, parse its JSON output, and traverse json_path to
    extract a resource ID.  Results are cached by args tuple so the same list
    command is never re-run within a harness invocation.

    id_from schema:
      {
        "args":      ["monitors", "list"],     # pup args to run
        "json_path": [0, "id"],                # key/index path into parsed JSON
      }

    Returns the ID as a string, or None if the command fails, returns no data,
    or the path cannot be traversed.
    """
    key = tuple(id_from["args"])
    if key in cache:
        return cache[key]

    exit_code, stdout, _, _ = run_command(BINARY, list(id_from["args"]), timeout, env)
    if exit_code != 0 or not stdout.strip():
        cache[key] = None
        return None

    try:
        obj: Any = json.loads(stdout.strip())
        for step in id_from["json_path"]:
            obj = obj[step]
        result = str(obj)
        cache[key] = result
        return result
    except Exception:
        cache[key] = None
        return None


# ── untested command discovery ────────────────────────────────────────────────

def get_untested_commands(
    test_env: dict[str, str],
) -> tuple[list[str], list[str]]:
    """
    Run `FORCE_AGENT_MODE=1 pup --help` to get the complete command schema and
    return two lists:
      - uncovered_read_only: read_only=True commands not in the test catalog
      - write_commands:      read_only=False leaf commands (intentionally skipped)
    """
    env = dict(test_env)
    env["FORCE_AGENT_MODE"] = "1"
    try:
        proc = subprocess.run(
            [str(BINARY), "--help"],
            capture_output=True, text=True, timeout=15,
            env=env, cwd=REPO_ROOT,
        )
        if proc.returncode != 0 or not proc.stdout.strip():
            return [], []
        schema = json.loads(proc.stdout)
    except Exception:
        return [], []

    # Collect all leaf commands with their read_only flag.
    def collect_leaves(cmds: list, prefix: str = "") -> list[tuple[str, bool]]:
        results = []
        for cmd in cmds:
            path = (prefix + " " + cmd["name"]).strip()
            subs = cmd.get("subcommands", [])
            if subs:
                results.extend(collect_leaves(subs, path))
            else:
                results.append((path, bool(cmd.get("read_only", False))))
        return results

    all_leaves = collect_leaves(schema.get("commands", []))

    # Build set of command paths covered by the catalog.
    covered = set()
    for tc in READ_COMMANDS:
        label = tc["label"]
        path = label.split("--")[0].strip()
        covered.add(path)
        args = [a for a in tc["args"] if not a.startswith("--")]
        covered.add(" ".join(args))

    uncovered_read_only = sorted(p for p, ro in all_leaves if ro and p not in covered)
    write_commands = sorted(p for p, ro in all_leaves if not ro)
    return uncovered_read_only, write_commands


# ── build ─────────────────────────────────────────────────────────────────────

def build_binary() -> tuple[bool, float]:
    print("▶ Building release binary (cargo build --release)…", flush=True)
    start = time.monotonic()
    proc = subprocess.run(
        ["cargo", "build", "--release"],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
    )
    elapsed = (time.monotonic() - start) * 1000
    if proc.returncode != 0:
        print(f"  ✗ Build failed ({elapsed:.0f}ms)")
        print(proc.stderr[-3000:])
        return False, elapsed
    print(f"  ✓ Build succeeded ({elapsed:.0f}ms)")
    return True, elapsed


# ── HTML report ───────────────────────────────────────────────────────────────

# ── diff helpers ──────────────────────────────────────────────────────────────

_MAX_DIFF_LINES = 500  # per-output cap; beyond this difflib SequenceMatcher is too slow


def _diff_html(human_text: str, agent_text: str) -> str:
    """
    Produce an HTML diff view (both inline and side-by-side) comparing the
    human-mode and agent-mode stdout for a single test.

    Returns the full HTML fragment (two divs, one shown at a time).
    """
    human_lines = human_text.splitlines(keepends=True)
    agent_lines = agent_text.splitlines(keepends=True)

    # Guard against O(n·m) difflib.SequenceMatcher on very large outputs.
    # Commands like `notebooks list` or `dashboards list` can return thousands
    # of lines; diffing them takes many minutes of 100% CPU.
    if len(human_lines) > _MAX_DIFF_LINES or len(agent_lines) > _MAX_DIFF_LINES:
        msg = (
            f"Output too large to diff inline "
            f"(human: {len(human_lines)} lines, agent: {len(agent_lines)} lines). "
            f"Limit is {_MAX_DIFF_LINES} lines per side."
        )
        placeholder = f"<div class='diff-identical'>{msg}</div>"
        return placeholder, placeholder

    # ── inline (unified) diff ─────────────────────────────────────────────────
    unified = list(difflib.unified_diff(
        human_lines, agent_lines,
        fromfile="human", tofile="agent", lineterm="",
    ))

    def esc(s: str) -> str:
        return (s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;"))

    def diff_line_class(line: str) -> str:
        if line.startswith("+++") or line.startswith("---"):
            return "dl-header"
        if line.startswith("@@"):
            return "dl-hunk"
        if line.startswith("+"):
            return "dl-add"
        if line.startswith("-"):
            return "dl-del"
        return "dl-ctx"

    if not unified:
        # No diff — identical output
        inline_html = "<div class='diff-identical'>Identical output in both modes</div>"
        sidebyside_html = "<div class='diff-identical'>Identical output in both modes</div>"
    else:
        # Inline diff
        inline_rows = []
        for line in unified:
            line = line.rstrip("\n")
            cls = diff_line_class(line)
            inline_rows.append(f"<div class='{cls}'>{esc(line)}</div>")
        inline_html = "<div class='diff-inline'>" + "".join(inline_rows) + "</div>"

        # Side-by-side diff using SequenceMatcher
        matcher = difflib.SequenceMatcher(None, human_lines, agent_lines)
        left_rows: list[str] = []
        right_rows: list[str] = []

        for op, i1, i2, j1, j2 in matcher.get_opcodes():
            if op == "equal":
                for ln in human_lines[i1:i2]:
                    ln = ln.rstrip("\n")
                    left_rows.append(f"<div class='dl-ctx'>{esc(ln)}</div>")
                    right_rows.append(f"<div class='dl-ctx'>{esc(ln)}</div>")
            elif op == "replace":
                lh = human_lines[i1:i2]
                la = agent_lines[j1:j2]
                # Pad the shorter side with empty lines so rows align
                n = max(len(lh), len(la))
                for k in range(n):
                    lv = lh[k].rstrip("\n") if k < len(lh) else ""
                    rv = la[k].rstrip("\n") if k < len(la) else ""
                    left_rows.append(f"<div class='dl-del'>{esc(lv)}</div>")
                    right_rows.append(f"<div class='dl-add'>{esc(rv)}</div>")
            elif op == "delete":
                for ln in human_lines[i1:i2]:
                    ln = ln.rstrip("\n")
                    left_rows.append(f"<div class='dl-del'>{esc(ln)}</div>")
                    right_rows.append("<div class='dl-pad'>&nbsp;</div>")
            elif op == "insert":
                for ln in agent_lines[j1:j2]:
                    ln = ln.rstrip("\n")
                    left_rows.append("<div class='dl-pad'>&nbsp;</div>")
                    right_rows.append(f"<div class='dl-add'>{esc(ln)}</div>")

        sidebyside_html = (
            "<div class='diff-side'>"
            "<div class='diff-col'><div class='diff-col-hdr'>Human</div>"
            + "".join(left_rows) +
            "</div>"
            "<div class='diff-col'><div class='diff-col-hdr'>Agent</div>"
            + "".join(right_rows) +
            "</div></div>"
        )

    return inline_html, sidebyside_html


STATUS_COLORS = {
    "pass":      ("#d4edda", "#155724", "✓ PASS"),
    "fail":      ("#f8d7da", "#721c24", "✗ FAIL"),
    "auth_fail": ("#fff3cd", "#856404", "⚠ AUTH"),
    "skipped":   ("#e2e3e5", "#383d41", "– SKIP"),
    "untested":  ("#ffeeba", "#533f03", "? UNTESTED"),
    "write":     ("#d6d8db", "#495057", "✎ WRITE"),
}


def html_escape(s: str) -> str:
    return (s
            .replace("&", "&amp;")
            .replace("<", "&lt;")
            .replace(">", "&gt;")
            .replace('"', "&quot;"))


def generate_report(
    results: list[TestResult],
    build_ok: bool,
    build_time_ms: float,
    total_time_ms: float,
    report_path: Path,
    run_timestamp: str,
    auth_info: str,
) -> None:
    counts: dict[str, int] = {s: 0 for s in STATUS_COLORS}
    for r in results:
        counts[r.status] = counts.get(r.status, 0) + 1

    rows_html = []
    for i, r in enumerate(results):
        bg, fg, label = STATUS_COLORS.get(r.status, ("#fff", "#000", r.status))
        # For auth_fail rows, append the classified HTTP reason to the label.
        if r.status == "auth_fail" and r.auth_reason:
            label = f"{label}<br><small style='font-weight:normal;opacity:0.85'>{html_escape(r.auth_reason)}</small>"
        cmd_str = html_escape("pup " + " ".join(r.args))

        defect_html = ""
        if r.defects:
            items = "".join(f"<li>{html_escape(d)}</li>" for d in r.defects)
            defect_html = f"<ul class='defect-list'>{items}</ul>"

        regression_html = ""
        if r.regression:
            regression_html = (
                "<div class='regression'>"
                "<strong>Schema regression:</strong>"
                f"<pre>{html_escape(r.regression)}</pre>"
                "</div>"
            )

        snapshot_badge = ""
        if r.snapshot_created:
            snapshot_badge = "<span class='badge badge-new'>snapshot created</span>"

        note_html = ""
        msg = r.note or r.skip_reason
        if msg:
            note_html = f"<div class='note'>{html_escape(msg)}</div>"

        stdout_html = ""
        stderr_html = ""
        if r.stdout.strip():
            preview = r.stdout[:2000]
            if len(r.stdout) > 2000:
                preview += f"\n… ({len(r.stdout) - 2000} more bytes truncated)"
            stdout_html = f"""
            <details>
              <summary>stdout ({len(r.stdout)} bytes)</summary>
              <pre class='output'>{html_escape(preview)}</pre>
            </details>"""
        if r.stderr.strip():
            preview = r.stderr[:1000]
            if len(r.stderr) > 1000:
                preview += f"\n… ({len(r.stderr) - 1000} more bytes truncated)"
            stderr_html = f"""
            <details>
              <summary>stderr ({len(r.stderr)} bytes)</summary>
              <pre class='output stderr'>{html_escape(preview)}</pre>
            </details>"""

        synthetic = r.status in ("untested", "write")
        exit_cell = "—" if synthetic else str(r.exit_code)
        time_cell = "—" if synthetic else f"{r.duration_ms:.0f}ms"
        cmd_cell  = "—" if synthetic else f"<code class='cmd'>{cmd_str}</code>"
        tid = r.test_id

        # mode badge — synthetic rows get a neutral dash
        if r.mode == "human":
            mode_badge = "<span class='badge badge-human'>Human</span>"
        elif r.mode == "agent":
            mode_badge = "<span class='badge badge-agent'>Agent</span>"
        else:
            mode_badge = "<span style='color:#aaa'>—</span>"

        # data-mode: actual mode for real runs; "all" for synthetic so they
        # remain visible regardless of which mode filter is active.
        data_mode = r.mode if r.mode else "all"

        # Build diff cell: current run vs previous run (same mode).
        # Synthetic rows (untested/write) have no output to diff.
        diff_cell = "<td></td>"
        if not synthetic and r.row_diff and r.row_diff[0]:
            inline_html, sidebyside_html = r.row_diff
            # Use a unique suffix per row so toggle buttons don't collide
            dsuffix = re.sub(r"[^a-z0-9]", "_", tid)
            diff_cell = f"""<td>
              <div class='diff-toggle'>
                <button id='btn-inline-{dsuffix}' class='active'
                  onclick='showDiff("{dsuffix}","inline")'>Inline</button>
                <button id='btn-side-{dsuffix}'
                  onclick='showDiff("{dsuffix}","side")'>Side-by-side</button>
              </div>
              <div class='diff-wrap'>
                <div id='diff-inline-{dsuffix}'>{inline_html}</div>
                <div id='diff-side-{dsuffix}' style='display:none'>{sidebyside_html}</div>
              </div>
            </td>"""

        rows_html.append(f"""
        <tr id="{tid}" data-status="{r.status}" data-mode="{data_mode}" data-id="{tid}">
          <td class="id-cell"><a class="id-link" href="#{tid}">{tid}</a></td>
          <td class="status-cell" style="color:{fg};font-weight:bold;white-space:nowrap;background:{bg}">{label}</td>
          <td><code>{html_escape(r.label)}</code></td>
          <td style="white-space:nowrap">{mode_badge}</td>
          <td>{cmd_cell}</td>
          <td style="text-align:right">{exit_cell}</td>
          <td style="text-align:right">{time_cell}</td>
          <td>
            {defect_html}
            {regression_html}
            {snapshot_badge}
            {note_html}
            {stdout_html}
            {stderr_html}
          </td>
          {diff_cell}
        </tr>""")

    all_rows = "\n".join(rows_html)
    build_status = "✓ succeeded" if build_ok else "✗ FAILED"
    build_color  = "#155724"    if build_ok else "#721c24"

    html = f"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>pup integration test harness — {run_timestamp}</title>
<style>
  * {{ box-sizing: border-box; }}
  body {{
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
    margin: 0; padding: 24px; background: #f5f5f5; color: #333;
  }}
  h1 {{ font-size: 1.6rem; margin-bottom: 4px; }}
  .meta {{ color: #666; font-size: 0.9rem; margin-bottom: 16px; }}
  .summary-grid {{ display: flex; gap: 16px; flex-wrap: wrap; margin-bottom: 20px; }}
  .summary-card {{
    padding: 16px 24px; border-radius: 8px; min-width: 100px; text-align: center;
    box-shadow: 0 1px 3px rgba(0,0,0,.1); cursor: pointer;
  }}
  .summary-card .num {{ font-size: 2rem; font-weight: bold; line-height: 1; }}
  .summary-card .lbl {{ font-size: 0.8rem; text-transform: uppercase; letter-spacing: .05em; }}
  .build-status {{
    padding: 8px 12px; border-radius: 6px; margin-bottom: 16px;
    font-weight: bold; display: inline-block;
  }}
  .auth-info {{
    background: #e8f4f8; border-left: 3px solid #0d6efd;
    padding: 8px 12px; border-radius: 4px; margin-bottom: 16px;
    font-size: 0.85rem;
  }}
  .filters {{ margin-bottom: 16px; }}
  .filter-row {{ margin-bottom: 8px; }}
  .filter-row:last-child {{ margin-bottom: 0; }}
  .filters button {{
    padding: 4px 12px; margin-right: 8px; border: 1px solid #ccc;
    border-radius: 4px; cursor: pointer; background: white; font-size: 0.85rem;
  }}
  .filters button.active {{ background: #333; color: white; border-color: #333; }}
  .badge-human {{ background: #d1ecf1; color: #0c5460; }}
  .badge-agent {{ background: #d4edda; color: #155724; }}
  /* ── diff view ── */
  .diff-toggle {{ margin-bottom: 4px; }}
  .diff-toggle button {{
    padding: 2px 8px; margin-right: 4px; border: 1px solid #ccc;
    border-radius: 3px; cursor: pointer; background: white; font-size: 0.75rem;
  }}
  .diff-toggle button.active {{ background: #555; color: white; border-color: #555; }}
  .diff-wrap {{ font-family: monospace; font-size: 0.74rem; max-height: 300px;
    overflow: auto; border: 1px solid #e0e0e0; border-radius: 4px; }}
  .diff-identical {{ padding: 6px 10px; color: #6c757d; font-size: 0.8rem; font-style: italic; }}
  /* shared line styles */
  .dl-add  {{ background: #e6ffed; color: #155724; white-space: pre-wrap; word-break: break-word; padding: 0 6px; }}
  .dl-del  {{ background: #ffeef0; color: #721c24; white-space: pre-wrap; word-break: break-word; padding: 0 6px; }}
  .dl-ctx  {{ background: #fff;    color: #333;    white-space: pre-wrap; word-break: break-word; padding: 0 6px; }}
  .dl-hunk {{ background: #dbedff; color: #0369a1; white-space: pre;      padding: 0 6px; }}
  .dl-header {{ background: #f6f8fa; color: #555; white-space: pre; padding: 0 6px; }}
  .dl-pad  {{ background: #f6f8fa; padding: 0 6px; }}
  /* inline */
  .diff-inline {{ display: block; }}
  /* side-by-side */
  .diff-side {{ display: flex; }}
  .diff-col  {{ flex: 1; min-width: 0; overflow: hidden; }}
  .diff-col + .diff-col {{ border-left: 2px solid #ddd; }}
  .diff-col-hdr {{ background: #f6f8fa; color: #555; font-size: 0.72rem;
    padding: 2px 6px; border-bottom: 1px solid #e0e0e0; font-weight: bold; }}
  table {{
    width: 100%; border-collapse: collapse; background: white;
    box-shadow: 0 1px 3px rgba(0,0,0,.1); border-radius: 8px; overflow: hidden;
  }}
  th {{
    background: #333; color: white; padding: 10px 12px;
    text-align: left; font-size: 0.8rem; text-transform: uppercase; letter-spacing: .05em;
  }}
  td {{ padding: 8px 12px; vertical-align: top; border-bottom: 1px solid rgba(0,0,0,.05); }}
  tr:last-child td {{ border-bottom: none; }}
  .cmd {{ font-size: 0.82rem; word-break: break-all; }}
  .defect-list {{ margin: 4px 0; padding-left: 16px; color: #721c24; font-size: 0.85rem; }}
  .regression {{
    background: #fff3cd; border-left: 3px solid #856404;
    padding: 6px 10px; margin: 4px 0; font-size: 0.82rem;
  }}
  .regression pre {{ margin: 4px 0; white-space: pre-wrap; word-break: break-word; }}
  .note {{ color: #555; font-size: 0.82rem; font-style: italic; margin: 2px 0; }}
  .badge {{ display: inline-block; padding: 1px 6px; border-radius: 10px; font-size: 0.75rem; font-weight: bold; }}
  .badge-new {{ background: #cce5ff; color: #004085; }}
  .output {{
    background: #1e1e1e; color: #d4d4d4; padding: 10px; border-radius: 4px;
    font-size: 0.78rem; max-height: 400px; overflow: auto;
    white-space: pre-wrap; word-break: break-word; margin: 4px 0;
  }}
  .stderr {{ background: #2d1b1b; color: #f8b4b4; }}
  details summary {{ cursor: pointer; color: #555; font-size: 0.82rem; padding: 2px 0; user-select: none; }}
  details summary:hover {{ color: #000; }}
  .id-cell {{ white-space: nowrap; width: 1%; }}
  .id-link {{
    font-family: monospace; font-size: 0.75rem; color: #999;
    text-decoration: none; padding: 1px 5px; border-radius: 3px;
    border: 1px solid #e0e0e0; background: #fafafa;
  }}
  .id-link:hover {{ color: #333; border-color: #999; background: #f0f0f0; }}
  tr:target {{ outline: 2px solid #0d6efd; outline-offset: -2px; }}
  .id-search {{
    display: inline-flex; align-items: center; gap: 6px;
    margin-left: 16px; font-size: 0.85rem;
  }}
  .id-search input {{
    padding: 3px 8px; border: 1px solid #ccc; border-radius: 4px;
    font-family: monospace; font-size: 0.82rem; width: 200px;
  }}
</style>
</head>
<body>
<h1>pup integration test harness</h1>
<div class="meta">
  Generated: {run_timestamp} &nbsp;|&nbsp;
  Binary: <code>{html_escape(str(BINARY))}</code> &nbsp;|&nbsp;
  Total time: {total_time_ms/1000:.1f}s
</div>

<div class="build-status" style="background:{'#d4edda' if build_ok else '#f8d7da'};color:{build_color}">
  Build: {build_status} ({build_time_ms:.0f}ms)
</div>

<div class="auth-info">
  <strong>Auth:</strong> {html_escape(auth_info)}
</div>

<div class="summary-grid">
  <div class="summary-card" style="background:#d4edda;color:#155724" onclick="setFilter('status','pass',null)">
    <div class="num">{counts.get('pass', 0)}</div><div class="lbl">Pass</div>
  </div>
  <div class="summary-card" style="background:#f8d7da;color:#721c24" onclick="setFilter('status','fail',null)">
    <div class="num">{counts.get('fail', 0)}</div><div class="lbl">Fail</div>
  </div>
  <div class="summary-card" style="background:#fff3cd;color:#856404" onclick="setFilter('status','auth_fail',null)">
    <div class="num">{counts.get('auth_fail', 0)}</div><div class="lbl">Auth Fail</div>
  </div>
  <div class="summary-card" style="background:#e2e3e5;color:#383d41" onclick="setFilter('status','skipped',null)">
    <div class="num">{counts.get('skipped', 0)}</div><div class="lbl">Skipped</div>
  </div>
  <div class="summary-card" style="background:#ffeeba;color:#533f03" onclick="setFilter('status','untested',null)">
    <div class="num">{counts.get('untested', 0)}</div><div class="lbl">Untested</div>
  </div>
  <div class="summary-card" style="background:#d6d8db;color:#495057" onclick="setFilter('status','write',null)">
    <div class="num">{counts.get('write', 0)}</div><div class="lbl">Write ops</div>
  </div>
  <div class="summary-card" style="background:#cce5ff;color:#004085" onclick="setFilter('status','all',null)">
    <div class="num">{len(results)}</div><div class="lbl">Total</div>
  </div>
</div>

<div class="filters">
  <div class="filter-row">
    <strong>Status:</strong>
    <button class="f-status btn-all active" onclick="setFilter('status','all',this)">All</button>
    <button class="f-status" onclick="setFilter('status','fail',this)">Failures</button>
    <button class="f-status" onclick="setFilter('status','pass',this)">Passes</button>
    <button class="f-status" onclick="setFilter('status','auth_fail',this)">Auth fail</button>
    <button class="f-status" onclick="setFilter('status','skipped',this)">Skipped</button>
    <button class="f-status" onclick="setFilter('status','untested',this)">Untested</button>
    <button class="f-status" onclick="setFilter('status','write',this)">Write ops</button>
    <span class="id-search">
      <label for="id-input">Jump to ID:</label>
      <input id="id-input" type="text" placeholder="e.g. logs-query-human" oninput="jumpToId(this.value)">
    </span>
  </div>
  <div class="filter-row">
    <strong>Mode:</strong>
    <button class="f-mode btn-all active" onclick="setFilter('mode','all',this)">All modes</button>
    <button class="f-mode" onclick="setFilter('mode','human',this)">Human</button>
    <button class="f-mode" onclick="setFilter('mode','agent',this)">Agent</button>
  </div>
</div>

<table>
<thead>
  <tr>
    <th>ID</th><th>Status</th><th>Test</th><th>Mode</th><th>Command</th><th>Exit</th><th>Time</th><th>Details</th><th>Diff (vs prev run)</th>
  </tr>
</thead>
<tbody id="results-body">
{all_rows}
</tbody>
</table>

<script>
// Active filter state — both dimensions must match for a row to be visible.
const activeFilters = {{ status: 'all', mode: 'all' }};

function setFilter(group, val, btn) {{
  activeFilters[group] = val;
  // Update button active state within this filter group only
  document.querySelectorAll('.f-' + group).forEach(b => b.classList.remove('active'));
  if (btn) btn.classList.add('active');
  applyFilters();
}}

function applyFilters() {{
  document.querySelectorAll('#results-body tr').forEach(row => {{
    const rowStatus = row.dataset.status;
    const rowMode   = row.dataset.mode;
    const statusOk  = activeFilters.status === 'all' || rowStatus === activeFilters.status;
    // data-mode="all" marks synthetic rows (untested/write) — always visible
    // regardless of which mode filter is active.
    const modeOk    = activeFilters.mode === 'all' || rowMode === activeFilters.mode || rowMode === 'all';
    row.style.display = (statusOk && modeOk) ? '' : 'none';
  }});
}}

function jumpToId(val) {{
  const id = val.trim().toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
  if (!id) return;
  const row = document.getElementById(id);
  if (row) {{
    // Reset both filters so the target row is guaranteed to be visible
    activeFilters.status = 'all';
    activeFilters.mode   = 'all';
    document.querySelectorAll('.f-status, .f-mode').forEach(b => b.classList.remove('active'));
    document.querySelectorAll('.btn-all').forEach(b => b.classList.add('active'));
    document.querySelectorAll('#results-body tr').forEach(r => r.style.display = '');
    row.scrollIntoView({{behavior: 'smooth', block: 'center'}});
    row.style.outline = '2px solid #0d6efd';
    setTimeout(() => row.style.outline = '', 2000);
  }}
}}
function showDiff(suffix, view) {{
  const inline = document.getElementById('diff-inline-' + suffix);
  const side   = document.getElementById('diff-side-'   + suffix);
  const btnI   = document.getElementById('btn-inline-'  + suffix);
  const btnS   = document.getElementById('btn-side-'    + suffix);
  if (!inline || !side) return;
  inline.style.display = view === 'inline' ? '' : 'none';
  side.style.display   = view === 'side'   ? '' : 'none';
  btnI.classList.toggle('active', view === 'inline');
  btnS.classList.toggle('active', view === 'side');
}}
// On page load, scroll to URL hash if present
window.addEventListener('DOMContentLoaded', () => {{
  if (location.hash) {{
    const el = document.querySelector(location.hash);
    if (el) el.scrollIntoView({{block: 'center'}});
  }}
}});
</script>
</body>
</html>"""

    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(html)
    print(f"\n📄 Report written to: {report_path}")


# ── main ──────────────────────────────────────────────────────────────────────

def main() -> int:
    parser = argparse.ArgumentParser(description="pup integration test harness")
    parser.add_argument("--update-snapshots", action="store_true",
                        help="Accept current output as new snapshot baseline")
    parser.add_argument("--no-build", action="store_true",
                        help="Skip cargo build (use existing binary)")
    parser.add_argument("--no-auth", action="store_true",
                        help="Skip pup auth login and dd-auth credential injection")
    parser.add_argument("--dd-auth-domain", default="app.datadoghq.com", metavar="DOMAIN",
                        help="dd-auth domain (default: app.datadoghq.com)")
    parser.add_argument("--output", default=str(DEFAULT_REPORT), metavar="FILE",
                        help=f"HTML report path (default: {DEFAULT_REPORT})")
    parser.add_argument("--filter", default="", metavar="PATTERN",
                        help="Only run tests whose label contains PATTERN")
    parser.add_argument("--timeout", type=int, default=60, metavar="SECS",
                        help="Per-command timeout in seconds (default: 60)")
    args = parser.parse_args()

    report_path = Path(args.output)
    run_timestamp = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M:%S UTC")

    print(f"pup integration test harness — {run_timestamp}")
    print(f"Repo: {REPO_ROOT}\n")

    # ── 1. build ─────────────────────────────────────────────────────────
    if args.no_build:
        build_ok = BINARY.exists()
        build_time_ms = 0.0
        if not build_ok:
            print(f"✗ Binary not found at {BINARY}. Run without --no-build first.")
            return 1
        print(f"▶ Skipping build — using: {BINARY}")
    else:
        build_ok, build_time_ms = build_binary()
        if not build_ok:
            print("Build failed; skipping tests.")
            generate_report([], build_ok, build_time_ms, build_time_ms,
                            report_path, run_timestamp, "build failed")
            return 1

    # ── 2. credentials ───────────────────────────────────────────────────
    dd_auth_vars: dict[str, str] = {}
    auth_info_parts: list[str] = []

    if args.no_auth:
        print("▶ --no-auth: skipping dd-auth and pup auth login\n")
        auth_info_parts.append("--no-auth (no credential injection)")
    else:
        # 2a. Fetch DD_API_KEY / DD_APP_KEY / DD_SITE via dd-auth
        print(f"▶ Fetching credentials from dd-auth (domain: {args.dd_auth_domain})…")
        dd_auth_vars = fetch_dd_auth_env(args.dd_auth_domain)
        if dd_auth_vars:
            auth_info_parts.append("dd-auth: credentials injected")
        else:
            auth_info_parts.append("dd-auth: unavailable or failed")

        # Build the env that auth commands will use
        login_env = build_clean_env(dd_auth_vars, agent_mode=False)

        # 2b. Ensure a valid OAuth2 token (status → refresh → login)
        print(f"\n▶ Ensuring OAuth2 token is valid…")
        _, auth_desc = ensure_auth(login_env)
        auth_info_parts.append(auth_desc)
        print()

    auth_info = " | ".join(auth_info_parts) if auth_info_parts else "outer environment only"

    # Build mode-specific environments (human has no FORCE_AGENT_MODE)
    human_env = build_clean_env(dd_auth_vars, agent_mode=False)
    agent_env = build_clean_env(dd_auth_vars, agent_mode=True)

    # ── 2c. discover untested commands ───────────────────────────────────
    if not args.filter:
        untested_commands, write_commands = get_untested_commands(human_env)
    else:
        untested_commands, write_commands = [], []

    # ── 3. run tests ─────────────────────────────────────────────────────
    test_start = time.monotonic()
    results: list[TestResult] = []

    tests = [t for t in READ_COMMANDS
             if args.filter.lower() in t["label"].lower()]

    print(f"▶ Running {len(tests)} tests × 2 modes (timeout: {args.timeout}s each)…\n")

    # Cache for id_from resolutions — keyed by tuple(args), always resolved
    # using human_env since resource IDs are identical across modes.
    id_cache: dict[tuple, str | None] = {}

    for mode, env in [("human", human_env), ("agent", agent_env)]:
        print(f"  ── {mode.upper()} mode {'─'*20}")
        for i, tc in enumerate(tests):
            label           = tc["label"]
            category        = tc.get("category", "auth_required")
            expect_json     = tc.get("expect_json", True)
            expect_exit     = tc.get("expect_exit", None)
            skip_regression      = tc.get("skip_regression", False)
            max_regression_depth = tc.get("max_regression_depth", 0)
            note                 = tc.get("note", "")
            cmd_timeout          = tc.get("timeout", args.timeout)

            print(f"  [{i+1:3d}/{len(tests)}] {label} … ", end="", flush=True)

            # Resolve {id} placeholder via id_from before running the test.
            test_args = list(tc["args"])
            id_from_spec = tc.get("id_from")
            if id_from_spec and any("{id}" in a for a in test_args):
                resolved = resolve_id(id_from_spec, human_env, id_cache, cmd_timeout)
                if resolved is None:
                    skip_msg = f"id_from: no ID from '{' '.join(id_from_spec['args'])}'"
                    result = TestResult(
                        label=label, args=test_args, category=category,
                        exit_code=0, stdout="", stderr="", duration_ms=0,
                        note=note, mode=mode,
                    )
                    result.skipped = True
                    result.skip_reason = skip_msg
                    print(f"- ({skip_msg})")
                    results.append(result)
                    continue
                test_args = [a.replace("{id}", resolved) for a in test_args]

            exit_code, stdout, stderr, duration_ms = run_command(
                BINARY, test_args, cmd_timeout, env,
            )

            result = TestResult(
                label=label,
                args=test_args,
                category=category,
                exit_code=exit_code,
                stdout=stdout,
                stderr=stderr,
                duration_ms=duration_ms,
                expect_exit=expect_exit,
                note=note,
                mode=mode,
            )

            if exit_code == -1:
                result.defects.append(f"Command timed out after {cmd_timeout}s")
            elif exit_code == -2:
                result.defects.append(f"Runner error: {stderr}")
            else:
                result.defects = check_defects(result, expect_json, expect_exit)
                if not skip_regression:
                    regression, created = check_regression(
                        result, args.update_snapshots, max_regression_depth
                    )
                    result.regression = regression
                    result.snapshot_created = created

            # Classify auth failures with the specific HTTP status code.
            if result.status == "auth_fail":
                result.auth_reason = classify_auth_reason(stdout, stderr)

            # Per-row diff: current stdout vs last saved output for this mode.
            prev = load_last_output(label, mode)
            if prev is not None and stdout:
                result.row_diff = _diff_html(prev, stdout)
            elif stdout and prev is None:
                placeholder = "<div class='diff-identical'>No prior run captured yet</div>"
                result.row_diff = (placeholder, placeholder)
            if stdout:
                save_last_output(label, mode, stdout)

            sym = {"pass": "✓", "fail": "✗", "auth_fail": "⚠", "skipped": "-"}
            suffix = ""
            if result.defects:
                suffix += f"  [{', '.join(result.defects[:2])}]"
            if result.regression:
                suffix += " [regression]"
            if result.snapshot_created:
                suffix += " [snapshot]"
            print(f"{sym.get(result.status, '?')} ({duration_ms:.0f}ms){suffix}")

            results.append(result)
        print()

    total_time_ms = (time.monotonic() - test_start) * 1000

    # ── 4a. append synthetic rows for untested / write commands ──────────
    # Synthetic rows have mode="" so they appear under all mode filter views.
    for cmd in untested_commands:
        results.append(TestResult(
            label=cmd,
            args=cmd.split(),
            category="untested",
            exit_code=-1,
            stdout="",
            stderr="",
            duration_ms=0.0,
            skip_reason=(
                "No catalog entry — command has read_only=true but is not yet tested. "
                "Add an entry to READ_COMMANDS in scripts/test_harness.py to cover it."
            ),
            status_override="untested",
            mode="",
        ))

    for cmd in write_commands:
        results.append(TestResult(
            label=cmd,
            args=cmd.split(),
            category="write",
            exit_code=-1,
            stdout="",
            stderr="",
            duration_ms=0.0,
            skip_reason=(
                "Write/mutating command (read_only=false) — excluded from automated "
                "testing to avoid unintended side effects. Run manually to verify."
            ),
            status_override="write",
            mode="",
        ))

    # ── 4b. summary ──────────────────────────────────────────────────────
    passes     = sum(1 for r in results if r.status == "pass")
    fails      = sum(1 for r in results if r.status == "fail")
    auth_fails = sum(1 for r in results if r.status == "auth_fail")
    new_snaps  = sum(1 for r in results if r.snapshot_created)
    n_untested = len(untested_commands)
    n_write    = len(write_commands)
    n_run      = len(results) - n_untested - n_write  # 2× catalog (human + agent)

    print(f"\n── Summary {'─'*30}")
    print(f"  Pass:          {passes}  ({passes // 2} human + {passes - passes // 2} agent)")
    print(f"  Fail:          {fails}")
    print(f"  Auth fail:     {auth_fails}")
    print(f"  Tested total:  {n_run} ({n_run // 2} tests × 2 modes)")
    print(f"  Untested:      {n_untested}")
    print(f"  Write ops:     {n_write}")
    print(f"  New snapshots: {new_snaps}")
    print(f"  Time:          {total_time_ms/1000:.1f}s")

    if fails:
        print(f"\n✗ Failures:")
        for r in results:
            if r.status == "fail":
                regression_flag = " + regression" if r.regression else ""
                print(f"  - {r.label}: {'; '.join(r.defects)}{regression_flag}")

    # Per-row diffs are already stored on each result (current vs previous run,
    # same mode).  No cross-mode diff building needed.

    # ── 5. report ────────────────────────────────────────────────────────
    generate_report(
        results=results,
        build_ok=build_ok,
        build_time_ms=build_time_ms,
        total_time_ms=total_time_ms,
        report_path=report_path,
        run_timestamp=run_timestamp,
        auth_info=auth_info,
    )

    return 1 if fails else 0


if __name__ == "__main__":
    sys.exit(main())
