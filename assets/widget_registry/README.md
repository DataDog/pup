# Bundled widget registry

These files are copied without modification from:

`domains/graphing/shared/libs/node/widget_schema_generator/generated/`

in the local `dd-source` checkout:

- `generated__widget_schemas_compressed.ts`
- `widget_data_sources.json`
- `widget_deps.json`

The current snapshot comes from dd-source commit
`bd59608d073aaa44e848b8d2b82b08814211eb9a`. Pup returns this revision as
`schema_version` and reports `source: "bundled"`. A future HTTP provider must
return the same response contract with a backend schema version.

Refresh all three together after regenerating the upstream registry. The Rust
tests verify that every registered widget has at least one bundled schema
definition.
