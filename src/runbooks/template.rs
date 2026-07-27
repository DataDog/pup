use std::collections::HashMap;

/// Render a template string replacing `{{ VAR }}` and `{{ VAR | default: "x" }}` tokens.
/// Unknown variables with no default are left as-is.
pub fn render(template: &str, vars: &HashMap<String, String>) -> String {
    let mut result = String::new();
    let mut rest = template;

    while let Some(start) = rest.find("{{") {
        result.push_str(&rest[..start]);
        rest = &rest[start + 2..];

        if let Some(end) = rest.find("}}") {
            let inner = &rest[..end];
            rest = &rest[end + 2..];
            result.push_str(&resolve_token(inner.trim(), vars));
        } else {
            // No closing braces — emit as-is
            result.push_str("{{");
            result.push_str(rest);
            return result;
        }
    }

    result.push_str(rest);
    result
}

/// Resolve a single `{{ ... }}` token interior.
/// Supports `VAR` and `VAR | default: "value"`.
fn resolve_token(inner: &str, vars: &HashMap<String, String>) -> String {
    if let Some(pipe_pos) = inner.find('|') {
        let var_name = inner[..pipe_pos].trim();
        let filter = inner[pipe_pos + 1..].trim();
        if let Some(val) = vars.get(var_name) {
            return val.clone();
        }
        // Try `default: "value"` or `default: value`
        if let Some(rest) = filter.strip_prefix("default:") {
            let default_val = rest.trim().trim_matches('"');
            return default_val.to_string();
        }
        // Unknown filter — return empty string
        String::new()
    } else {
        let var_name = inner.trim();
        vars.get(var_name)
            .cloned()
            .unwrap_or_else(|| format!("{{{{ {var_name} }}}}"))
    }
}

/// Parse a duration string into a `std::time::Duration`.
///
/// Delegates to `crate::util_ext::parse_duration_to_millis` — accepts the same
/// formats (30s, 5m, 1h, 7d, 1w, "5 minutes", etc.).
pub fn parse_duration(s: &str) -> anyhow::Result<std::time::Duration> {
    let ms = crate::util_ext::parse_duration_to_millis(s)?;
    Ok(std::time::Duration::from_millis(ms as u64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_simple() {
        let mut vars = HashMap::new();
        vars.insert("NAME".into(), "world".into());
        assert_eq!(render("Hello, {{ NAME }}!", &vars), "Hello, world!");
    }

    #[test]
    fn test_render_default_used() {
        let vars = HashMap::new();
        assert_eq!(
            render("Hello, {{ NAME | default: \"stranger\" }}!", &vars),
            "Hello, stranger!"
        );
    }

    #[test]
    fn test_render_default_overridden() {
        let mut vars = HashMap::new();
        vars.insert("NAME".into(), "pup".into());
        assert_eq!(
            render("Hello, {{ NAME | default: \"stranger\" }}!", &vars),
            "Hello, pup!"
        );
    }

    #[test]
    fn test_render_unknown_left_as_is() {
        let vars = HashMap::new();
        assert_eq!(render("{{ MISSING }}", &vars), "{{ MISSING }}");
    }

    #[test]
    fn test_parse_duration() {
        assert_eq!(
            parse_duration("30s").unwrap(),
            std::time::Duration::from_secs(30)
        );
        assert_eq!(
            parse_duration("5m").unwrap(),
            std::time::Duration::from_secs(300)
        );
        assert_eq!(
            parse_duration("1h").unwrap(),
            std::time::Duration::from_secs(3600)
        );
        assert_eq!(
            parse_duration("7d").unwrap(),
            std::time::Duration::from_secs(7 * 86400)
        );
        assert_eq!(
            parse_duration("1w").unwrap(),
            std::time::Duration::from_secs(7 * 86400)
        );
    }

    #[test]
    fn test_parse_duration_invalid() {
        assert!(parse_duration("abc").is_err());
    }
}
