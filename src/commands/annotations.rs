use anyhow::Result;
use datadog_api_client::datadogV2::api_annotations::{
    AnnotationsAPI, ListAnnotationsOptionalParams,
};
use datadog_api_client::datadogV2::model::{AnnotationCreateRequest, AnnotationUpdateRequest};

use crate::config::Config;
use crate::formatter;
use crate::util;

pub async fn list(
    cfg: &Config,
    page_id: &str,
    start_time: i64,
    end_time: i64,
    widget_id: Option<String>,
) -> Result<()> {
    let api = crate::make_api!(AnnotationsAPI, cfg);
    let params = if let Some(wid) = widget_id {
        ListAnnotationsOptionalParams::default().widget_id(wid)
    } else {
        ListAnnotationsOptionalParams::default()
    };
    let resp = api
        .list_annotations(page_id.to_string(), start_time, end_time, params)
        .await
        .map_err(|e| anyhow::anyhow!("failed to list annotations: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn get_page(cfg: &Config, page_id: &str, start_time: i64, end_time: i64) -> Result<()> {
    let api = crate::make_api!(AnnotationsAPI, cfg);
    let resp = api
        .get_page_annotations(page_id.to_string(), start_time, end_time)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get page annotations: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn create(cfg: &Config, file: &str) -> Result<()> {
    let api = crate::make_api!(AnnotationsAPI, cfg);
    let body: AnnotationCreateRequest = util::read_json_file(file)?;
    let resp = api
        .create_annotation(body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create annotation: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn update(cfg: &Config, annotation_id: uuid::Uuid, file: &str) -> Result<()> {
    let api = crate::make_api!(AnnotationsAPI, cfg);
    let body: AnnotationUpdateRequest = util::read_json_file(file)?;
    let resp = api
        .update_annotation(annotation_id, body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to update annotation: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn delete(cfg: &Config, annotation_id: uuid::Uuid) -> Result<()> {
    let api = crate::make_api!(AnnotationsAPI, cfg);
    api.delete_annotation(annotation_id)
        .await
        .map_err(|e| anyhow::anyhow!("failed to delete annotation: {e:?}"))?;
    println!("Successfully deleted annotation {annotation_id}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::test_support::*;

    #[tokio::test]
    async fn test_annotations_list() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, r#"{"data":[]}"#).await;
        let result = super::list(&cfg, "test-page", 0, 3600, None).await;
        assert!(
            result.is_ok(),
            "annotations list failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_annotations_list_error() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        s.mock("GET", mockito::Matcher::Any)
            .with_status(403)
            .with_body("forbidden")
            .create_async()
            .await;
        let result = super::list(&cfg, "test-page", 0, 3600, None).await;
        assert!(result.is_err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_annotations_get_page() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(
            &mut s,
            r#"{"data":{"id":"p1","type":"page_annotations","attributes":{"annotations":{},"global_annotations":[],"widget_mapping":{}}}}"#,
        )
        .await;
        let result = super::get_page(&cfg, "test-page", 0, 3600).await;
        assert!(
            result.is_ok(),
            "get page annotations failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_annotations_get_page_error() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        s.mock("GET", mockito::Matcher::Any)
            .with_status(404)
            .with_body(r#"{"errors":["not found"]}"#)
            .create_async()
            .await;
        let result = super::get_page(&cfg, "missing-page", 0, 3600).await;
        assert!(result.is_err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_annotations_create() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(
            &mut s,
            r#"{"data":{"id":"00000000-0000-0000-0000-000000000001","type":"annotation","attributes":{"author_id":"u1","color":"blue","created_at":0,"description":"x","end_time":0,"modified_at":0,"page_id":"p1","start_time":0,"type":"pointInTime"}}}"#,
        )
        .await;
        let tmp = write_temp_json(
            "annotation_create.json",
            r#"{"data":{"type":"annotation","attributes":{"color":"blue","description":"deploy","page_id":"p1","start_time":0,"type":"pointInTime"}}}"#,
        );
        let result = super::create(&cfg, tmp.to_str().unwrap()).await;
        assert!(
            result.is_ok(),
            "annotation create failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_annotations_create_bad_file() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, "{}").await;
        let result = super::create(&cfg, "/nonexistent/file.json").await;
        assert!(result.is_err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_annotations_update() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(
            &mut s,
            r#"{"data":{"id":"00000000-0000-0000-0000-000000000001","type":"annotation","attributes":{"author_id":"u1","color":"blue","created_at":0,"description":"x","end_time":0,"modified_at":0,"page_id":"p1","start_time":0,"type":"pointInTime"}}}"#,
        )
        .await;
        let tmp = write_temp_json(
            "annotation_update.json",
            r#"{"data":{"type":"annotation","attributes":{"color":"green","description":"rollback","page_id":"p1","start_time":1000,"type":"pointInTime"}}}"#,
        );
        let id = uuid::Uuid::nil();
        let result = super::update(&cfg, id, tmp.to_str().unwrap()).await;
        assert!(
            result.is_ok(),
            "annotation update failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_annotations_update_bad_file() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, "{}").await;
        let id = uuid::Uuid::nil();
        let result = super::update(&cfg, id, "/nonexistent/file.json").await;
        assert!(result.is_err());
        cleanup_env();
    }

    #[tokio::test]
    async fn test_annotations_delete() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        mock_all(&mut s, "").await;
        let id = uuid::Uuid::nil();
        let result = super::delete(&cfg, id).await;
        assert!(
            result.is_ok(),
            "annotation delete failed: {:?}",
            result.err()
        );
        cleanup_env();
    }

    #[tokio::test]
    async fn test_annotations_delete_error() {
        let _lock = lock_env().await;
        let mut s = mockito::Server::new_async().await;
        let cfg = test_config(&s.url());
        s.mock("DELETE", mockito::Matcher::Any)
            .with_status(404)
            .with_body(r#"{"errors":["not found"]}"#)
            .create_async()
            .await;
        let id = uuid::Uuid::nil();
        let result = super::delete(&cfg, id).await;
        assert!(result.is_err());
        cleanup_env();
    }
}
