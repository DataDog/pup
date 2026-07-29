use anyhow::Result;
use datadog_api_client::datadogV2::api_authn_mappings::{
    AuthNMappingsAPI, ListAuthNMappingsOptionalParams,
};
use datadog_api_client::datadogV2::model::{AuthNMappingCreateRequest, AuthNMappingUpdateRequest};

use crate::config::Config;
use crate::formatter;
use crate::util;

pub async fn list(cfg: &Config) -> Result<()> {
    let api = crate::make_api!(AuthNMappingsAPI, cfg);
    let resp = api
        .list_authn_mappings(ListAuthNMappingsOptionalParams::default())
        .await
        .map_err(|e| anyhow::anyhow!("failed to list AuthN mappings: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn get(cfg: &Config, mapping_id: &str) -> Result<()> {
    let api = crate::make_api!(AuthNMappingsAPI, cfg);
    let resp = api
        .get_authn_mapping(mapping_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to get AuthN mapping: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn create(cfg: &Config, file: &str) -> Result<()> {
    let body: AuthNMappingCreateRequest = util::read_json_file(file)?;
    let api = crate::make_api!(AuthNMappingsAPI, cfg);
    let resp = api
        .create_authn_mapping(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create AuthN mapping: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn update(cfg: &Config, mapping_id: &str, file: &str) -> Result<()> {
    let body: AuthNMappingUpdateRequest = util::read_json_file(file)?;
    let api = crate::make_api!(AuthNMappingsAPI, cfg);
    let resp = api
        .update_authn_mapping(mapping_id.to_string(), body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update AuthN mapping: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn delete(cfg: &Config, mapping_id: &str) -> Result<()> {
    let api = crate::make_api!(AuthNMappingsAPI, cfg);
    api.delete_authn_mapping(mapping_id.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete AuthN mapping: {e:?}"))?;
    println!("AuthN mapping '{mapping_id}' deleted.");
    Ok(())
}

#[cfg(test)]
mod tests {

    use crate::test_support::*;

    /// Sample create-request body matching the AuthNMappingCreateRequest schema.
    /// Used by create-path tests to exercise the JSON parser in `util::read_json_file`.
    const SAMPLE_CREATE_JSON: &str = r#"{
      "data": {
        "type": "authn_mappings",
        "attributes": {
          "attribute_key": "member-of",
          "attribute_value": "engineering"
        },
        "relationships": {
          "role": {
            "data": { "type": "roles", "id": "11111111-1111-1111-1111-111111111111" }
          }
        }
      }
    }"#;

    /// Sample update-request body matching the AuthNMappingUpdateRequest schema.
    const SAMPLE_UPDATE_JSON: &str = r#"{
      "data": {
        "type": "authn_mappings",
        "id": "abc-123",
        "attributes": {
          "attribute_key": "member-of",
          "attribute_value": "security"
        },
        "relationships": {
          "role": {
            "data": { "type": "roles", "id": "22222222-2222-2222-2222-222222222222" }
          }
        }
      }
    }"#;

    // -----------------------------------------------------------------------
    // list()
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_authn_mappings_list_success() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "GET", r#"{"data":[]}"#).await;
        let result = super::list(&cfg).await;
        assert!(
            result.is_ok(),
            "authn mappings list failed: {:?}",
            result.err()
        );
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_authn_mappings_list_accepts_oauth_bearer_token() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let mut cfg = test_config(&server.url());
        // Simulate OAuth-only auth: bearer token configured, no API/APP keys.
        cfg.api_key = None;
        cfg.app_key = None;
        cfg.access_token = Some("oauth-bearer-token".into());
        std::env::remove_var("DD_API_KEY");
        std::env::remove_var("DD_APP_KEY");

        let _mock = server
            .mock("GET", mockito::Matcher::Any)
            .match_header("Authorization", "Bearer oauth-bearer-token")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data":[]}"#)
            .create_async()
            .await;

        let result = super::list(&cfg).await;
        assert!(
            result.is_ok(),
            "authn mappings list with OAuth bearer failed: {:?}",
            result.err()
        );
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_authn_mappings_list_error() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(403)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors":["Forbidden"]}"#)
            .create_async()
            .await;
        let result = super::list(&cfg).await;
        assert!(result.is_err(), "expected error for 403 response");
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("failed to list AuthN mappings"));
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    // -----------------------------------------------------------------------
    // get()
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_authn_mappings_get_success() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(
            &mut server,
            "GET",
            r#"{"data":{"type":"authn_mappings","id":"abc-123","attributes":{}}}"#,
        )
        .await;
        let result = super::get(&cfg, "abc-123").await;
        assert!(
            result.is_ok(),
            "authn mappings get failed: {:?}",
            result.err()
        );
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_authn_mappings_get_error_404() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(404)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors":["Not Found"]}"#)
            .create_async()
            .await;
        let result = super::get(&cfg, "missing").await;
        assert!(result.is_err(), "expected error on 404");
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("failed to get AuthN mapping"));
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    // -----------------------------------------------------------------------
    // create()
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_authn_mappings_create_success() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(
            &mut server,
            "POST",
            r#"{"data":{"type":"authn_mappings","id":"new-1","attributes":{}}}"#,
        )
        .await;

        let path = write_temp_json("pup_authn_create_ok.json", SAMPLE_CREATE_JSON);
        let result = super::create(&cfg, path.to_str().unwrap()).await;
        assert!(
            result.is_ok(),
            "authn mappings create failed: {:?}",
            result.err()
        );

        let _ = std::fs::remove_file(&path);
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_authn_mappings_create_missing_file() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        // Nonexistent path — file read must fail before any HTTP call.
        let result = super::create(&cfg, "/tmp/__pup_authn_missing_fixture__.json").await;
        assert!(result.is_err(), "expected error for missing file");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("failed to read file"),
            "error should mention file read failure: {err_msg}"
        );
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_authn_mappings_create_invalid_json() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());

        // Well-formed JSON that doesn't match the expected schema.
        let path = write_temp_json("pup_authn_create_bad.json", r#"{"nope":true}"#);
        let result = super::create(&cfg, path.to_str().unwrap()).await;
        assert!(result.is_err(), "expected error for invalid schema");
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("failed to parse JSON"));

        let _ = std::fs::remove_file(&path);
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_authn_mappings_create_api_error() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = server
            .mock("POST", mockito::Matcher::Any)
            .with_status(409)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors":["Conflict"]}"#)
            .create_async()
            .await;

        let path = write_temp_json("pup_authn_create_conflict.json", SAMPLE_CREATE_JSON);
        let result = super::create(&cfg, path.to_str().unwrap()).await;
        assert!(result.is_err(), "expected error on 409");
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("failed to create AuthN mapping"));

        let _ = std::fs::remove_file(&path);
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    // -----------------------------------------------------------------------
    // update()
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_authn_mappings_update_success() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(
            &mut server,
            "PATCH",
            r#"{"data":{"type":"authn_mappings","id":"abc-123","attributes":{}}}"#,
        )
        .await;

        let path = write_temp_json("pup_authn_update_ok.json", SAMPLE_UPDATE_JSON);
        let result = super::update(&cfg, "abc-123", path.to_str().unwrap()).await;
        assert!(
            result.is_ok(),
            "authn mappings update failed: {:?}",
            result.err()
        );

        let _ = std::fs::remove_file(&path);
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_authn_mappings_update_missing_file() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let result = super::update(&cfg, "abc-123", "/tmp/__pup_authn_update_missing__.json").await;
        assert!(result.is_err(), "expected error for missing file");
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("failed to read file"));
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_authn_mappings_update_api_error() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = server
            .mock("PATCH", mockito::Matcher::Any)
            .with_status(404)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors":["Not Found"]}"#)
            .create_async()
            .await;

        let path = write_temp_json("pup_authn_update_404.json", SAMPLE_UPDATE_JSON);
        let result = super::update(&cfg, "missing", path.to_str().unwrap()).await;
        assert!(result.is_err(), "expected error on 404");
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("failed to update AuthN mapping"));

        let _ = std::fs::remove_file(&path);
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    // -----------------------------------------------------------------------
    // delete()
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_authn_mappings_delete_success() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = mock_any(&mut server, "DELETE", "").await;
        let result = super::delete(&cfg, "abc-123").await;
        assert!(
            result.is_ok(),
            "authn mappings delete failed: {:?}",
            result.err()
        );
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }

    #[tokio::test]
    async fn test_authn_mappings_delete_error_404() {
        let _lock = lock_env().await;
        std::env::set_var("DD_TOKEN_STORAGE", "file");
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let _mock = server
            .mock("DELETE", mockito::Matcher::Any)
            .with_status(404)
            .with_header("content-type", "application/json")
            .with_body(r#"{"errors":["Not Found"]}"#)
            .create_async()
            .await;
        let result = super::delete(&cfg, "missing").await;
        assert!(result.is_err(), "expected error on 404");
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("failed to delete AuthN mapping"));
        cleanup_env();
        std::env::remove_var("DD_TOKEN_STORAGE");
    }
}
