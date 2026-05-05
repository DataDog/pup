use serde_json::Value;

#[allow(dead_code)]
pub struct McpMapping {
    pub tool_name: &'static str,
    pub translate_args: fn(&[(&str, Value)]) -> Value,
}

#[allow(dead_code)]
pub fn lookup(command_path: &str) -> Option<McpMapping> {
    match command_path {
        "security findings analyze" => Some(McpMapping {
            tool_name: "analyze_security_findings",
            translate_args: translate_findings_analyze,
        }),
        "security findings search" => Some(McpMapping {
            tool_name: "search_security_findings",
            translate_args: translate_findings_search,
        }),
        "security findings schema" => Some(McpMapping {
            tool_name: "security_findings_schema",
            translate_args: translate_findings_schema,
        }),
        _ => None,
    }
}

fn translate_findings_analyze(args: &[(&str, Value)]) -> Value {
    let mut obj = serde_json::Map::new();
    for (key, val) in args {
        match *key {
            "query" => {
                obj.insert("sql_query".to_string(), val.clone());
            }
            _ => {}
        }
    }
    obj.insert(
        "telemetry".to_string(),
        serde_json::json!({"intent": "pup-cli-proxy"}),
    );
    Value::Object(obj)
}

fn translate_findings_search(args: &[(&str, Value)]) -> Value {
    let mut obj = serde_json::Map::new();
    for (key, val) in args {
        match *key {
            "query" => {
                obj.insert("filter".to_string(), val.clone());
            }
            "limit" => {
                obj.insert("limit".to_string(), val.clone());
            }
            _ => {}
        }
    }
    obj.insert(
        "telemetry".to_string(),
        serde_json::json!({"intent": "pup-cli-proxy"}),
    );
    Value::Object(obj)
}

fn translate_findings_schema(_args: &[(&str, Value)]) -> Value {
    serde_json::json!({
        "include_description": true,
        "telemetry": {"intent": "pup-cli-proxy"},
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lookup_known_commands() {
        assert!(lookup("security findings analyze").is_some());
        assert!(lookup("security findings search").is_some());
        assert!(lookup("security findings schema").is_some());
    }

    #[test]
    fn test_lookup_unknown_command() {
        assert!(lookup("monitors list").is_none());
        assert!(lookup("").is_none());
    }

    #[test]
    fn test_translate_analyze() {
        let mapping = lookup("security findings analyze").unwrap();
        let args = vec![(
            "query",
            Value::String("SELECT * FROM dd.security_findings()".into()),
        )];
        let result = (mapping.translate_args)(&args);
        assert_eq!(result["sql_query"], "SELECT * FROM dd.security_findings()");
        assert_eq!(result["telemetry"]["intent"], "pup-cli-proxy");
    }

    #[test]
    fn test_translate_search() {
        let mapping = lookup("security findings search").unwrap();
        let args = vec![
            ("query", Value::String("@status:open".into())),
            ("limit", Value::Number(50.into())),
        ];
        let result = (mapping.translate_args)(&args);
        assert_eq!(result["filter"], "@status:open");
        assert_eq!(result["limit"], 50);
        assert_eq!(result["telemetry"]["intent"], "pup-cli-proxy");
    }

    #[test]
    fn test_translate_schema() {
        let mapping = lookup("security findings schema").unwrap();
        let result = (mapping.translate_args)(&[]);
        assert_eq!(result["include_description"], true);
        assert_eq!(result["telemetry"]["intent"], "pup-cli-proxy");
    }

    #[test]
    fn test_tool_names() {
        assert_eq!(
            lookup("security findings analyze").unwrap().tool_name,
            "analyze_security_findings"
        );
        assert_eq!(
            lookup("security findings search").unwrap().tool_name,
            "search_security_findings"
        );
        assert_eq!(
            lookup("security findings schema").unwrap().tool_name,
            "security_findings_schema"
        );
    }
}
