//! jq filter support via the [`jaq`](https://github.com/01mf02/jaq) engine.
//!
//! [`apply_jq`] is the sole public entry point; the jaq API is encapsulated
//! here so version-churn in jaq-core never spreads across the codebase.

use anyhow::{bail, Result};
use jaq_core::{
    data,
    load::{Arena, File, Loader},
    unwrap_valr, Compiler, Ctx, Vars,
};
use jaq_json::Val;

/// Apply a jq expression to a JSON value.
///
/// The jq output stream is collapsed to a single `serde_json::Value`:
/// - 0 outputs → `Value::Null`
/// - 1 output  → the value itself (not wrapped)
/// - ≥ 2 outputs → `Value::Array` of all outputs
///
/// The cardinality-dependent shape (1 vs. 2+) only surfaces for json/yaml
/// output; the table/csv/tsv renderers already normalise both shapes via
/// `extract_rows`.
///
/// Returns a clean [`anyhow::Error`] — never panics — on parse, compile, or
/// runtime errors.
pub fn apply_jq(value: serde_json::Value, expr: &str) -> Result<serde_json::Value> {
    // ---- compile --------------------------------------------------------
    let program = File {
        code: expr,
        path: (),
    };

    let defs = jaq_core::defs()
        .chain(jaq_std::defs())
        .chain(jaq_json::defs());
    let funs = jaq_core::funs::<data::JustLut<Val>>()
        .chain(jaq_std::funs())
        .chain(jaq_json::funs());

    let arena = Arena::default();
    let loader = Loader::new(defs);

    let modules = loader.load(&arena, program).map_err(|errs| {
        let msg = errs
            .into_iter()
            .map(|(_, e)| format!("{e:?}"))
            .collect::<Vec<_>>()
            .join("; ");
        anyhow::anyhow!("invalid --jq expression: {msg}")
    })?;

    let filter = Compiler::default()
        .with_funs(funs)
        .compile(modules)
        .map_err(|errs| {
            let msg = errs
                .into_iter()
                .map(|(_, e)| format!("{e:?}"))
                .collect::<Vec<_>>()
                .join("; ");
            anyhow::anyhow!("invalid --jq expression: {msg}")
        })?;

    // ---- convert input --------------------------------------------------
    // Val implements Deserialize (jaq-json `serde` feature), so we can
    // convert directly through serde's in-memory data model.
    let input: Val = serde_json::from_value(value)
        .map_err(|e| anyhow::anyhow!("--jq input conversion error: {e}"))?;

    // ---- run filter -----------------------------------------------------
    let ctx = Ctx::<data::JustLut<Val>>::new(&filter.lut, Vars::new([]));

    let outputs: Vec<serde_json::Value> = filter
        .id
        .run((ctx, input))
        .map(unwrap_valr)
        .map(|r| match r {
            // Val implements Display as compact JSON; parse back to Value.
            Ok(v) => serde_json::from_str(&v.to_string())
                .map_err(|e| anyhow::anyhow!("--jq output conversion error: {e}")),
            Err(e) => bail!("--jq filter error: {e}"),
        })
        .collect::<Result<Vec<_>>>()?;

    // ---- collapse cardinality -------------------------------------------
    Ok(match outputs.len() {
        0 => serde_json::Value::Null,
        1 => outputs.into_iter().next().unwrap(),
        _ => serde_json::Value::Array(outputs),
    })
}

#[cfg(test)]
mod tests {
    use super::apply_jq;
    use serde_json::json;

    fn jq(v: serde_json::Value, expr: &str) -> serde_json::Value {
        apply_jq(v, expr).unwrap()
    }

    // --- positive tests --------------------------------------------------

    #[test]
    fn extract_field_array() {
        let input = json!([{"name": "foo"}, {"name": "bar"}]);
        assert_eq!(jq(input, ".[].name"), json!(["foo", "bar"]));
    }

    #[test]
    fn select_endswith() {
        // mirrors the user's example: select(endswith("baz"))
        let input = json!([
            {"name": "foo-baz"},
            {"name": "foo-bar"},
            {"name": "baz"},
        ]);
        let result = jq(input, "[.[] | select(.name | endswith(\"baz\"))]");
        assert_eq!(result, json!([{"name": "foo-baz"}, {"name": "baz"}]));
    }

    #[test]
    fn scalar_result_unwrapped() {
        let input = json!({"foo": 42});
        // single output is NOT wrapped in an array
        assert_eq!(jq(input, ".foo"), json!(42));
    }

    #[test]
    fn empty_result_is_null() {
        let input = json!([]);
        assert_eq!(jq(input, ".[] | select(. > 100)"), json!(null));
    }

    #[test]
    fn two_outputs_wrapped_in_array() {
        let input = json!([1, 2]);
        assert_eq!(jq(input, ".[]"), json!([1, 2]));
    }

    #[test]
    fn single_output_unwrapped() {
        let input = json!([42]);
        // only one element → not wrapped
        assert_eq!(jq(input, ".[]"), json!(42));
    }

    // --- negative tests --------------------------------------------------

    #[test]
    fn invalid_expression_returns_error_no_panic() {
        let result = apply_jq(json!(null), "this is not jq . . .");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("invalid --jq"),
            "expected 'invalid --jq' in error, got: {msg}"
        );
    }

    #[test]
    fn unclosed_bracket_returns_error() {
        let result = apply_jq(json!({}), ".foo | [");
        assert!(result.is_err());
    }
}
