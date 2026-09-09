//! Image upload for embedding into notebook cells.
//!
//! This is not part of the public, documented Datadog API — it's the same
//! internal endpoint the notebooks UI itself uses to upload images — so
//! it's called via `raw_client` rather than the typed SDK, and its shape
//! could change without notice.
//!
//! Upload is a three-step, presigned-URL dance so image bytes never transit
//! through this call as a Datadog-authenticated request body:
//!   1. POST /api/ui/images        -> {img_uuid, upload_url} (Datadog auth)
//!   2. PUT  <upload_url>          -> raw bytes straight to blob storage (no Datadog auth)
//!   3. POST /api/ui/image_status/{img_uuid} -> report success/failure (Datadog auth)
//!
//! The resulting `/api/v2/images/{img_uuid}` path is what a notebook cell
//! should reference; fetching it 302s to a freshly-signed, short-lived
//! download URL.

use anyhow::{Context, Result};
use serde_json::json;

use crate::config::Config;
use crate::formatter;
use crate::raw_client;

const IMAGES_PATH: &str = "/api/ui/images";

/// Resolves a user-supplied image reference to the `/api/v2/images/{uuid}`
/// path: accepts a bare UUID, a `content_url`-style path, or a full URL.
fn resolve_image_path(reference: &str) -> String {
    if reference.starts_with('/') {
        reference.to_string()
    } else if let Some(idx) = reference.find("/api/v2/images/") {
        reference[idx..].to_string()
    } else {
        format!("/api/v2/images/{reference}")
    }
}

fn normalize_format(raw: &str) -> Result<String> {
    let lower = raw.trim_start_matches('.').to_ascii_lowercase();
    match lower.as_str() {
        "png" | "jpeg" | "jpg" | "gif" => Ok(lower),
        other => anyhow::bail!(
            "unsupported image format {other:?}; supported formats: png, jpeg, jpg, gif"
        ),
    }
}

fn infer_format(file: &str, format: Option<&str>) -> Result<String> {
    if let Some(explicit) = format {
        return normalize_format(explicit);
    }
    let ext = std::path::Path::new(file)
        .extension()
        .and_then(|ext| ext.to_str())
        .ok_or_else(|| {
            anyhow::anyhow!("could not infer image format from {file:?}; pass --format explicitly")
        })?;
    normalize_format(ext)
}

fn mime_for(format: &str) -> &'static str {
    match format {
        "png" => "image/png",
        "jpeg" => "image/jpeg",
        "jpg" => "image/jpg",
        "gif" => "image/gif",
        other => unreachable!("normalize_format should have rejected {other:?}"),
    }
}

pub async fn upload(cfg: &Config, file: &str, format: Option<&str>) -> Result<()> {
    let img_format = infer_format(file, format)?;
    let bytes =
        std::fs::read(file).with_context(|| format!("failed to read image file {file:?}"))?;

    let created = raw_client::raw_post_jsonapi(
        cfg,
        IMAGES_PATH,
        "img-upload-request",
        json!({ "img_format": img_format }),
    )
    .await
    .map_err(|e| anyhow::anyhow!("failed to request an image upload URL: {e}"))?;

    let attrs = created
        .pointer("/data/attributes")
        .ok_or_else(|| anyhow::anyhow!("unexpected image upload response: {created}"))?;
    let upload_url = attrs
        .get("upload_url")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("image upload response missing upload_url"))?;
    let img_uuid = attrs
        .get("img_uuid")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("image upload response missing img_uuid"))?
        .to_string();

    // The presigned URL is signed with this exact content type baked in, so a
    // mismatched Content-Type here fails the storage provider's signature
    // check rather than the upload itself.
    let put_result = reqwest::Client::new()
        .put(upload_url)
        .header("Content-Type", mime_for(&img_format))
        .body(bytes)
        .send()
        .await;

    let upload_succeeded = matches!(&put_result, Ok(resp) if resp.status().is_success());

    // Best-effort: tell the backend how the direct-to-storage upload went so
    // it doesn't retain metadata for a blob that was never written.
    let status_path = format!("/api/ui/image_status/{img_uuid}");
    let _ = raw_client::raw_post_jsonapi(
        cfg,
        &status_path,
        "img-status-update-request",
        json!({ "status": if upload_succeeded { "success" } else { "failure" } }),
    )
    .await;

    match put_result {
        Ok(resp) if resp.status().is_success() => {}
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("uploading image bytes failed (HTTP {status}): {body}");
        }
        Err(e) => anyhow::bail!("uploading image bytes failed: {e}"),
    }

    let content_url = format!("/api/v2/images/{img_uuid}");
    let payload = json!({
        "img_uuid": img_uuid,
        "img_format": img_format,
        "content_url": content_url,
    });
    formatter::format_and_print(
        &payload,
        &cfg.output_format,
        cfg.agent_mode,
        None,
        cfg.jq.as_deref(),
    )
}

/// Downloads an image previously uploaded via [`upload`] to a local file.
///
/// Reads the response as raw bytes rather than text — the endpoint 302s to
/// blob storage and returns binary image data, which corrupts silently if
/// routed through anything that treats the body as a UTF-8 string first.
pub async fn download(cfg: &Config, image_ref: &str, out: &str) -> Result<()> {
    let path = resolve_image_path(image_ref);
    let resp = raw_client::raw_request(cfg, "GET", &path, &[], None, None, "*/*", &[])
        .await
        .map_err(|e| anyhow::anyhow!("failed to download image: {e}"))?;

    std::fs::write(out, &resp.bytes)
        .with_context(|| format!("failed to write downloaded image to {out:?}"))?;

    let payload = json!({
        "path": out,
        "bytes_written": resp.bytes.len(),
        "content_type": resp.content_type,
    });
    formatter::format_and_print(
        &payload,
        &cfg.output_format,
        cfg.agent_mode,
        None,
        cfg.jq.as_deref(),
    )
}

#[cfg(test)]
mod tests {
    use crate::test_support::*;

    #[tokio::test]
    async fn test_upload_infers_format_and_returns_content_url() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());

        let path = std::env::temp_dir().join("__pup_test_image__.png");
        std::fs::write(&path, b"fake-png-bytes").unwrap();

        let upload_url = format!("{}/fake-presigned-put", s.url());
        let create_mock = s
            .mock("POST", super::IMAGES_PATH)
            .with_status(200)
            .with_header("content-type", "application/vnd.api+json")
            .with_body(
                serde_json::json!({
                    "data": {
                        "type": "img-upload-response",
                        "id": "1",
                        "attributes": {
                            "img_format": "png",
                            "upload_url": upload_url,
                            "img_uuid": "11111111-1111-1111-1111-111111111111",
                        }
                    }
                })
                .to_string(),
            )
            .create_async()
            .await;

        let put_mock = s
            .mock("PUT", "/fake-presigned-put")
            .match_header("content-type", "image/png")
            .with_status(200)
            .create_async()
            .await;

        let status_mock = s
            .mock(
                "POST",
                "/api/ui/image_status/11111111-1111-1111-1111-111111111111",
            )
            .with_status(200)
            .with_header("content-type", "application/vnd.api+json")
            .with_body(r#"{"data":{"type":"img-metadata","id":"1","attributes":{}}}"#)
            .create_async()
            .await;

        let result = super::upload(&cfg, path.to_str().unwrap(), None).await;
        std::fs::remove_file(&path).ok();

        assert!(result.is_ok(), "upload should succeed: {:?}", result);
        create_mock.assert_async().await;
        put_mock.assert_async().await;
        status_mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_upload_rejects_unrecognized_format() {
        let _lock = lock_env().await;
        let s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());

        let result = super::upload(&cfg, "notes.txt", None).await;
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("unsupported"),
            "should reject unrecognized extensions"
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_upload_reports_storage_failure() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());

        let path = std::env::temp_dir().join("__pup_test_image_fail__.png");
        std::fs::write(&path, b"fake-png-bytes").unwrap();

        let upload_url = format!("{}/fake-presigned-put-fail", s.url());
        s.mock("POST", super::IMAGES_PATH)
            .with_status(200)
            .with_header("content-type", "application/vnd.api+json")
            .with_body(
                serde_json::json!({
                    "data": {
                        "type": "img-upload-response",
                        "id": "1",
                        "attributes": {
                            "img_format": "png",
                            "upload_url": upload_url,
                            "img_uuid": "22222222-2222-2222-2222-222222222222",
                        }
                    }
                })
                .to_string(),
            )
            .create_async()
            .await;

        s.mock("PUT", "/fake-presigned-put-fail")
            .with_status(403)
            .with_body("SignatureDoesNotMatch")
            .create_async()
            .await;

        let status_mock = s
            .mock(
                "POST",
                "/api/ui/image_status/22222222-2222-2222-2222-222222222222",
            )
            .match_body(mockito::Matcher::JsonString(
                serde_json::json!({
                    "data": {
                        "type": "img-status-update-request",
                        "attributes": { "status": "failure" }
                    }
                })
                .to_string(),
            ))
            .with_status(200)
            .with_header("content-type", "application/vnd.api+json")
            .with_body(r#"{"data":{"type":"img-metadata","id":"1","attributes":{}}}"#)
            .create_async()
            .await;

        let result = super::upload(&cfg, path.to_str().unwrap(), None).await;
        std::fs::remove_file(&path).ok();

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("uploading image bytes failed"));
        status_mock.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn test_download_writes_raw_bytes_including_non_utf8() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());

        // A PNG header starts with byte 0x89, which is invalid UTF-8 on its
        // own; this is exactly the byte a lossy string conversion would
        // corrupt into the U+FFFD replacement character.
        let png_bytes: &[u8] = &[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

        let uuid = "33333333-3333-3333-3333-333333333333";
        let get_mock = s
            .mock("GET", format!("/api/v2/images/{uuid}").as_str())
            .with_status(200)
            .with_header("content-type", "image/png")
            .with_body(png_bytes)
            .create_async()
            .await;

        let out_path = std::env::temp_dir().join("__pup_test_download__.png");
        let result = super::download(&cfg, uuid, out_path.to_str().unwrap()).await;

        assert!(result.is_ok(), "download should succeed: {:?}", result);
        let written = std::fs::read(&out_path).unwrap();
        std::fs::remove_file(&out_path).ok();
        assert_eq!(written, png_bytes, "bytes must round-trip exactly");
        get_mock.assert_async().await;
        cleanup_env();
    }

    #[test]
    fn test_resolve_image_path_accepts_uuid_content_url_or_full_url() {
        assert_eq!(
            super::resolve_image_path("11111111-1111-1111-1111-111111111111"),
            "/api/v2/images/11111111-1111-1111-1111-111111111111"
        );
        assert_eq!(
            super::resolve_image_path("/api/v2/images/11111111-1111-1111-1111-111111111111"),
            "/api/v2/images/11111111-1111-1111-1111-111111111111"
        );
        assert_eq!(
            super::resolve_image_path(
                "https://api.datadoghq.com/api/v2/images/11111111-1111-1111-1111-111111111111"
            ),
            "/api/v2/images/11111111-1111-1111-1111-111111111111"
        );
    }
}
