#!/usr/bin/env python3
from __future__ import annotations

import argparse
import ast
import json
from pathlib import Path
import subprocess


FAMILIES = {
    "Streams": ["log_stream", "list_stream"],
    "Queries": ["timeseries", "bar_chart", "query_value", "toplist", "query_table", "change"],
    "Analysis": [
        "distribution", "heatmap", "geomap", "hostmap", "scatterplot", "sunburst",
        "treemap", "sankey", "funnel", "cohort", "retention_curve", "wildcard",
    ],
    "Status": [
        "alert_graph", "alert_value", "check_status", "manage_status", "slo", "slo_list",
        "topology_map", "trace_service",
    ],
    "Content": ["image", "iframe", "run_workflow"],
}

INTENT = {
    "log_stream": "Use for a direct stream of matching logs.",
    "list_stream": "Use for recent log, span, RUM, security, CI, or other event records.",
    "timeseries": "Use for values over time. Line, area, bars, and overlay are display forms of this type.",
    "bar_chart": "Use for scalar values grouped into categorical bars.",
    "query_value": "Use for one headline scalar value.",
    "toplist": "Use for a ranked set of groups.",
    "query_table": "Use for detailed grouped scalar rows and multiple value columns.",
    "change": "Use for current-versus-reference change.",
    "distribution": "Use for a histogram or value distribution.",
    "heatmap": "Use for density over time or two axes.",
    "geomap": "Use for values grouped by geographic facets.",
    "hostmap": "Use for hosts grouped visually with separate fill and size signals.",
    "scatterplot": "Use to correlate two or three measures.",
    "sunburst": "Use for hierarchical part-to-whole data in radial form.",
    "treemap": "Use for hierarchical part-to-whole data as nested rectangles.",
    "sankey": "Use for flows or journeys between ordered steps.",
    "funnel": "Use for conversion through ordered journey steps.",
    "cohort": "Use for cohort-based product analysis.",
    "retention_curve": "Use for retention over time by cohort.",
    "wildcard": "Use only for a known-good custom Vega definition.",
    "alert_graph": "Use for monitor history as a timeseries or toplist.",
    "alert_value": "Use for the current value or status of one monitor.",
    "check_status": "Use for service-check health.",
    "manage_status": "Use for a monitor summary.",
    "slo": "Use for one SLO and its time windows.",
    "slo_list": "Use for a list of SLOs.",
    "topology_map": "Use for service or resource topology.",
    "trace_service": "Use for a service summary.",
    "image": "Use to display an image URL.",
    "iframe": "Use to embed an external web page.",
    "run_workflow": "Use to run a configured workflow.",
}


def _constant_text(node: ast.AST) -> str | None:
    if isinstance(node, ast.Constant) and isinstance(node.value, str):
        return node.value
    if isinstance(node, ast.JoinedStr):
        return None
    return None


def _string_constants(path: Path) -> tuple[dict[str, str], list[str]]:
    """Collect top-level `NAME = "..."` docs and the COMMON_SECTIONS assembly order.

    Widget Expert injects these before the model writes anything. Harvesting them
    keeps one source of truth instead of a hand-written paraphrase that drifts.
    """
    tree = ast.parse(path.read_text(encoding="utf-8"))
    docs: dict[str, str] = {}
    order: list[str] = []
    for statement in tree.body:
        targets = []
        if isinstance(statement, ast.Assign):
            targets = [t for t in statement.targets if isinstance(t, ast.Name)]
        elif isinstance(statement, ast.AnnAssign) and isinstance(statement.target, ast.Name):
            targets = [statement.target]
        if not targets:
            continue
        name = targets[0].id
        text = _constant_text(statement.value)
        if text:
            docs[name] = text.strip()
        elif name == "COMMON_SECTIONS" and isinstance(statement.value, ast.List):
            order = [e.id for e in statement.value.elts if isinstance(e, ast.Name)]
    return docs, order


def _expert_sections(path: Path) -> tuple[dict[str, list[str]], dict[str, str]]:
    tree = ast.parse(path.read_text(encoding="utf-8"))
    pitfalls: dict[str, list[str]] = {}
    examples: dict[str, str] = {}
    for statement in tree.body:
        if not isinstance(statement, ast.AnnAssign) or not isinstance(statement.target, ast.Name):
            continue
        if statement.target.id == "WIDGET_PITFALLS" and isinstance(statement.value, ast.Dict):
            for key_node, value_node in zip(statement.value.keys, statement.value.values):
                if not isinstance(key_node, ast.Call) or not key_node.args:
                    continue
                collection = key_node.args[0]
                if not isinstance(collection, (ast.Set, ast.List, ast.Tuple)):
                    continue
                widget_types = [
                    item.value
                    for item in collection.elts
                    if isinstance(item, ast.Constant) and isinstance(item.value, str)
                ]
                text = _constant_text(value_node)
                if text:
                    for widget_type in widget_types:
                        pitfalls.setdefault(widget_type, []).append(text.strip())
        if statement.target.id == "WIDGET_EXAMPLES" and isinstance(statement.value, ast.Dict):
            for key_node, value_node in zip(statement.value.keys, statement.value.values):
                if not isinstance(key_node, ast.Constant) or not isinstance(key_node.value, str):
                    continue
                text = _constant_text(value_node)
                if text:
                    examples[key_node.value] = text.strip()
    return pitfalls, examples


HANDBOOK_PREAMBLE = """\
# Widget construction handbook

Read this before writing any telemetry request. It is generated from the Datadog
graphing widget reference, so it is the same guidance Ask Widget Expert receives.

Two notebook adjustments apply throughout, and they override the general text
below wherever they disagree.

**Envelopes.** Wrap every definition in a `notebook_cells` envelope. Never add
dashboard `layout`, widget identity fields, or `group` widgets.

**Timeframes.** "Setting the Widget Timeframe" below opens with "leave `time`
undefined so the widget inherits the page's global timeframe selector". That is
the default only when the request names no timeframe. Decide from what the
request actually names:

- Names a timeframe — "for the last 15 minutes", "over the past hour",
  "yesterday", "between 2pm and 4pm". Set `time` at the widget definition root.
  Omitting it is wrong.
- Names none — "over time", "request rate", "show me errors". Omit widget-level
  `time` and let the notebook's own `time` apply.

Both the modern `{type, value, unit}` and the legacy `{live_span}` shapes are
accepted. Never put `time` on an individual request.
"""

# Dashboard-only guidance. Notebooks have no `group` widget and no `layout`, so
# harvesting this section verbatim would contradict the envelope rule above.
EXCLUDED_SUBSECTIONS = ("### group widget",)


def _drop_subsections(text: str) -> str:
    kept: list[str] = []
    skipping = False
    for line in text.splitlines():
        if line.startswith("### "):
            skipping = line.startswith(EXCLUDED_SUBSECTIONS)
        if not skipping:
            kept.append(line)
    return "\n".join(kept).rstrip()


def _handbook(docs: dict[str, str], order: list[str]) -> str:
    names = list(order) + ["GENERAL_PITFALLS_DOC", "CROSS_CUTTING_EXAMPLES"]
    missing = [name for name in names if name not in docs]
    if missing:
        raise SystemExit(f"widget reference is missing expected sections: {missing}")
    parts = [HANDBOOK_PREAMBLE]
    for name in names:
        parts.append(_drop_subsections(docs[name]))
    return "\n\n".join(parts).rstrip() + "\n"


def _schema(pup: Path, widget_type: str) -> dict:
    completed = subprocess.run(
        # --no-agent keeps the payload flat. Pup wraps output in a status/data/metadata
        # envelope whenever it detects an AI session, which depends on the caller's
        # environment rather than on anything about this script.
        [str(pup), "widgets", "schema", widget_type, "--surface", "notebooks", "--no-agent"],
        check=True,
        capture_output=True,
        text=True,
    )
    payload = json.loads(completed.stdout)
    return payload.get("data", payload) if "schema" not in payload else payload


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--pup", type=Path, required=True)
    parser.add_argument("--dd-source", type=Path, required=True)
    args = parser.parse_args()

    skill_root = Path(__file__).resolve().parents[1]
    output_root = skill_root / "references" / "widget-types"
    output_root.mkdir(parents=True, exist_ok=True)
    widget_tools = args.dd_source / "domains/graphing/shared/py/ai_tools/widget_tools"
    expert_file = widget_tools / "full_widget_reference.py"
    pitfalls, examples = _expert_sections(expert_file)
    docs, common_order = _string_constants(expert_file)

    handbook_path = skill_root / "references" / "widgets" / "handbook.md"
    handbook_path.parent.mkdir(parents=True, exist_ok=True)
    handbook_path.write_text(_handbook(docs, common_order), encoding="utf-8")

    vega = (widget_tools / "wildcard_vega_reference.md").read_text(encoding="utf-8").strip()

    index = [
        "# Widget type index",
        "",
        "Choose one exact type, then read only its linked reference. Each reference carries the",
        "supported data sources, rules, warnings, examples, and the command that fetches the",
        "exact schema for your data source.",
        "",
        "Read [the construction handbook](widgets/handbook.md) first. It carries the query,",
        "formula, sort, unit, palette, and timeframe rules that every type shares.",
        "",
    ]
    for family, widget_types in FAMILIES.items():
        index.extend([f"## {family}", ""])
        for widget_type in widget_types:
            index.append(
                f"- [`{widget_type}`](widget-types/{widget_type}.md): {INTENT[widget_type]}"
            )
        index.append("")
    (skill_root / "references" / "widget-types.md").write_text(
        "\n".join(index).rstrip() + "\n", encoding="utf-8"
    )

    for widget_type in [item for values in FAMILIES.values() for item in values]:
        payload = _schema(args.pup.resolve(), widget_type)
        data_sources = payload.get("available_data_sources") or []
        lines = [
            f"<!-- Generated by scripts/generate-widget-type-skills.py. Do not edit by hand. -->",
            "",
            f"# {widget_type}",
            "",
            INTENT[widget_type],
            "",
            "Wrap the completed definition in one `notebook_cells` envelope. Do not add dashboard",
            "layout or widget identity fields.",
            "",
            "Cross-cutting query, formula, sort, unit, and palette rules live in",
            "[the construction handbook](../widgets/handbook.md).",
            "",
            "## Supported data sources",
            "",
            ", ".join(f"`{source}`" for source in data_sources) if data_sources else "This type has no telemetry data source.",
            "",
            "## Rules and warnings",
            "",
        ]
        if pitfalls.get(widget_type):
            lines.extend(["\n\n".join(pitfalls[widget_type]), ""])
        else:
            lines.extend([
                "Follow the schema exactly. Do not invent identifiers, requests, query fields, or",
                "configuration values. Preserve the user's source, filters, grouping, and display intent.",
                "",
            ])
        if examples.get(widget_type):
            lines.extend(["## Examples", "", examples[widget_type], ""])
        if widget_type == "wildcard":
            lines.extend(["## Vega-Lite reference", "", vega, ""])
        lines.extend([
            "## Fetch the schema",
            "",
            "Choose the data source first, then fetch only the layer you need. Both flags",
            "matter: `--data-source` drops the other sources' query types, and `--section`",
            "drops the styling types. Together they are about an eighth of the full schema.",
            "",
            "```bash",
            f"pup widgets schema {widget_type} --surface notebooks --data-source SOURCE --section request",
            "```",
            "",
            "That returns the definition, its request model, and the query and formula types.",
            "It is what you need to write a working widget.",
            "",
            "Fetch more only when the request calls for it:",
            "",
            "- styling, palettes, units, axes, markers, conditional formats, cell display,",
            f"  or a widget-level timeframe: add `--section presentation`",
            "- a visualization over an earlier analysis cell:"
            f" add `--section local-dataset`",
            "",
            "Omitted layers are listed at the end of the output with the flag that returns",
            "them. Read that line: if a field you need is missing, it is in a layer you did",
            "not ask for.",
            "",
            f"Schema snapshot `{payload.get('schema_version')}`, source `{payload.get('source')}`.",
            "",
        ])
        (output_root / f"{widget_type}.md").write_text("\n".join(lines), encoding="utf-8")


if __name__ == "__main__":
    main()
