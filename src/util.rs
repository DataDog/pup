use anyhow::Result;

/// Read a JSON file and deserialize into the specified type.
/// Used by create/update commands that accept `--file` input.
pub fn read_json_file<T: serde::de::DeserializeOwned>(path: &str) -> Result<T> {
    let contents = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("failed to read file {path:?}: {e}"))?;
    serde_json::from_str(&contents)
        .map_err(|e| anyhow::anyhow!("failed to parse JSON from {path:?}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_json_file_missing() {
        let result: Result<serde_json::Value> = read_json_file("/tmp/__pup_nonexistent__.json");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("failed to read"));
    }

    #[test]
    fn test_read_json_file_invalid_json() {
        let path = std::env::temp_dir().join("__pup_test_invalid__.json");
        std::fs::write(&path, "not json").unwrap();
        let result: Result<serde_json::Value> = read_json_file(path.to_str().unwrap());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("failed to parse"));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_read_json_file_valid() {
        let path = std::env::temp_dir().join("__pup_test_valid__.json");
        std::fs::write(&path, r#"{"name": "test"}"#).unwrap();
        let result: Result<serde_json::Value> = read_json_file(path.to_str().unwrap());
        assert!(result.is_ok());
        assert_eq!(result.unwrap()["name"], "test");
        std::fs::remove_file(&path).ok();
    }
}
