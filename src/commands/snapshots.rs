use anyhow::{bail, Context, Result};
use clap::{Args, Subcommand};
use serde_json::{json, Value};
use std::time::Duration;

use crate::{config::Config, formatter, raw_client, util, util_ext};

#[derive(Debug, Subcommand)]
pub enum SnapshotActions {
    /// Queue a widget PNG; return its URL or wait and save it with --out
    Create(CreateArgs),
}

#[derive(Debug, Args)]
pub struct CreateArgs {
    /// Bare widget definition JSON file, or - for stdin
    #[arg(long)]
    file: String,
    /// Start time (relative, RFC3339, or Unix seconds/milliseconds)
    #[arg(long, default_value = "1h")]
    from: String,
    /// End time
    #[arg(long, default_value = "now")]
    to: String,
    #[arg(long, default_value_t = 1000, value_parser = clap::value_parser!(u32).range(32..=2000))]
    width: u32,
    #[arg(long, default_value_t = 400, value_parser = clap::value_parser!(u32).range(32..=1000))]
    height: u32,
    /// Stored image retention
    #[arg(long, default_value = "60d", value_parser = ["30d", "60d", "90d", "1y", "2y", "inf"])]
    ttl: String,
    /// Allow anyone with the URL to view the image without authentication
    #[arg(long)]
    public: bool,
    /// Additional configuration JSON file (notebook_id, template_variables, etc.)
    #[arg(long)]
    additional_config: Option<String>,
    /// Wait for rendering and save PNG to a new local file
    #[arg(long)]
    out: Option<String>,
    /// Maximum seconds to wait for rendering with --out
    #[arg(long, default_value_t = 120, value_parser = clap::value_parser!(u64).range(1..=3600))]
    timeout: u64,
}

fn attributes(args: &CreateArgs, widget: Value) -> Result<Value> {
    if widget
        .get("type")
        .and_then(Value::as_str)
        .is_none_or(str::is_empty)
    {
        bail!("expected a bare widget definition with a string 'type'; extract the 'definition' field from dashboard widgets");
    }
    let start = util_ext::parse_time_to_unix_millis(&args.from)?;
    let end = util_ext::parse_time_to_unix_millis(&args.to)?;
    if start >= end {
        bail!("--from must be earlier than --to");
    }
    let additional: Value = match &args.additional_config {
        Some(path) => util::read_json_file(path)?,
        None => json!({}),
    };
    if !additional.is_object() {
        bail!("--additional-config must contain a JSON object");
    }
    Ok(json!({
        "source": "api",
        "is_authenticated": !args.public,
        "ttl": args.ttl,
        "widget_definition": widget,
        "start": start,
        "end": end,
        "width": args.width,
        "height": args.height,
        "additional_config": additional,
    }))
}

fn view_path(url: &str) -> Result<String> {
    let parsed = reqwest::Url::parse(url).context("snapshot response contains an invalid URL")?;
    let path = parsed.path();
    if !matches!(parsed.scheme(), "https" | "http")
        || !(path.starts_with("/api/v2/snapshot/view/public/")
            || path.starts_with("/api/v2/snapshot/view/authenticated/"))
        || !path.ends_with(".png")
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        bail!("snapshot response contains an unexpected view URL");
    }
    // App URLs and API URLs have different origins. Fetch only this path on the
    // configured API host, so a response cannot redirect credentials to its host.
    Ok(path.to_owned())
}

async fn wait_for_png(cfg: &Config, path: &str, interval: Duration) -> Result<Vec<u8>> {
    loop {
        let response =
            raw_client::raw_request(cfg, "GET", path, &[], None, None, "*/*", &[]).await?;
        let media_type = response.content_type.split(';').next().unwrap_or("").trim();
        match media_type {
            "image/png" => {
                if !response.bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
                    bail!("snapshot returned invalid PNG bytes");
                }
                return Ok(response.bytes);
            }
            "application/vnd.api+json" | "application/json" => {
                let pending: Value = serde_json::from_slice(&response.bytes)
                    .context("invalid snapshot status response")?;
                if pending
                    .pointer("/data/attributes/status")
                    .and_then(Value::as_str)
                    != Some("pending")
                {
                    bail!("unexpected snapshot status: {pending}");
                }
            }
            _ => bail!(
                "unexpected snapshot content type: {}",
                response.content_type
            ),
        }
        tokio::time::sleep(interval).await;
    }
}

pub async fn run(cfg: &Config, action: SnapshotActions) -> Result<()> {
    match action {
        SnapshotActions::Create(args) => create(cfg, &args).await,
    }
}

async fn create(cfg: &Config, args: &CreateArgs) -> Result<()> {
    if let Some(path) = &args.out {
        if std::path::Path::new(path).exists() {
            bail!("output file {path:?} already exists; choose a new --out path");
        }
    }
    let widget = if args.file == "-" {
        serde_json::from_str(&util_ext::read_to_string(
            std::io::stdin(),
            "read widget JSON from stdin",
        )?)
        .context("invalid widget JSON")?
    } else {
        util::read_json_file(&args.file)?
    };
    let response = raw_client::raw_post_jsonapi(
        cfg,
        "/api/v2/snapshot",
        "create_snapshot",
        attributes(args, widget)?,
    )
    .await?;
    let url = response
        .pointer("/data/attributes/url")
        .and_then(Value::as_str)
        .context("snapshot response missing URL")?;
    let path = view_path(url)?;
    let id = response
        .pointer("/data/id")
        .and_then(Value::as_str)
        .context("snapshot response missing ID")?;
    let mut output =
        json!({"id": id, "url": url, "is_authenticated": !args.public, "ttl": args.ttl});
    if let Some(out) = &args.out {
        let bytes = tokio::time::timeout(
            Duration::from_secs(args.timeout),
            wait_for_png(cfg, &path, Duration::from_secs(2)),
        )
        .await
        .with_context(|| {
            format!("timed out waiting for snapshot; rendering may still finish: {url}")
        })?
        .with_context(|| format!("failed to download snapshot: {url}"))?;
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(out)
            .with_context(|| format!("failed to create {out:?}; snapshot: {url}"))?;
        if let Err(error) = file.write_all(&bytes) {
            drop(file);
            let _ = std::fs::remove_file(out);
            return Err(error).with_context(|| format!("failed to save {out:?}; snapshot: {url}"));
        }
        output["path"] = json!(out);
        output["bytes_written"] = json!(bytes.len());
    }
    formatter::output(cfg, &output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;
    use clap::Parser;

    const VIEW: &str = "/api/v2/snapshot/view/authenticated/60d/org/image.png";
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\xff\x00";

    fn args() -> CreateArgs {
        let cli = crate::Cli::try_parse_from([
            "pup",
            "snapshots",
            "create",
            "--file",
            "widget.json",
            "--from",
            "1700000000",
            "--to",
            "1700003600",
        ])
        .unwrap();
        let crate::Commands::Snapshots {
            action: SnapshotActions::Create(args),
        } = cli.command
        else {
            panic!("expected snapshots create");
        };
        args
    }

    async fn mock_create(server: &mut mockito::Server, authenticated: bool) -> mockito::Mock {
        server
            .mock("POST", "/api/v2/snapshot")
            .match_header("content-type", "application/vnd.api+json")
            .match_header(
                "authorization",
                if authenticated {
                    mockito::Matcher::Missing
                } else {
                    mockito::Matcher::Exact("Bearer test-oauth-token".into())
                },
            )
            .match_body(mockito::Matcher::Json(json!({"data": {
                "type": "create_snapshot",
                "attributes": {
                    "source": "api", "is_authenticated": authenticated, "ttl": "60d",
                    "widget_definition": {"type": "timeseries", "requests": []},
                    "start": 1700000000000_i64, "end": 1700003600000_i64,
                    "width": 1000, "height": 400, "additional_config": {}
                }
            }})))
            .with_status(200)
            .with_header("content-type", "application/vnd.api+json")
            .with_body(
                json!({"data": {"id": "image", "attributes": {
                    "url": format!("https://app.datadoghq.com{VIEW}")
                }}})
                .to_string(),
            )
            .expect(1)
            .create_async()
            .await
    }

    fn input(temp: &TempDir) -> CreateArgs {
        let mut args = args();
        let file = temp.path().join("widget.json");
        std::fs::write(&file, r#"{"type":"timeseries","requests":[]}"#).unwrap();
        args.file = file.to_str().unwrap().to_owned();
        args
    }

    #[test]
    fn validates_cli_options_and_write_classification() {
        assert!(crate::is_write_command("snapshots create", "create"));
        for (flag, value) in [
            ("--width", "31"),
            ("--height", "1001"),
            ("--ttl", "7d"),
            ("--timeout", "0"),
        ] {
            assert!(crate::Cli::try_parse_from([
                "pup",
                "snapshots",
                "create",
                "--file",
                "-",
                flag,
                value
            ])
            .is_err());
        }
    }

    #[test]
    fn validates_widget_time_and_additional_configuration() {
        let mut args = args();
        for widget in [
            json!({"definition": {"type": "timeseries"}}),
            json!({"type": ""}),
            json!([]),
        ] {
            assert!(attributes(&args, widget).is_err());
        }
        let widget = json!({"type": "timeseries"});
        args.from = args.to.clone();
        assert!(attributes(&args, widget.clone())
            .unwrap_err()
            .to_string()
            .contains("earlier"));
        args.from = "not-a-time".into();
        assert!(attributes(&args, widget.clone()).is_err());
        let temp = TempDir::new("snapshot_config");
        let config = temp.path().join("config.json");
        args = self::args();
        args.additional_config = Some(config.to_str().unwrap().to_owned());
        std::fs::write(&config, "[]").unwrap();
        assert!(attributes(&args, widget.clone()).is_err());
        std::fs::write(&config, r#"{"notebook_id":123,"template_variables":[]}"#).unwrap();
        assert_eq!(
            attributes(&args, widget).unwrap()["additional_config"]["notebook_id"],
            123
        );
    }

    #[test]
    fn restricts_download_to_snapshot_view_path() {
        assert_eq!(
            view_path(&format!("https://app.datadoghq.eu{VIEW}")).unwrap(),
            VIEW
        );
        for url in [
            "https://app.datadoghq.com/api/v2/users",
            "file:///tmp/image.png",
            "not-a-url",
            "https://app.datadoghq.com/api/v2/snapshot/view/public/../../users.png",
        ] {
            assert!(view_path(url).is_err());
        }
    }

    #[tokio::test]
    async fn creates_url_without_polling_using_oauth_and_public_flag() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let mut cfg = test_config(&server.url());
        cfg.access_token = Some("test-oauth-token".into());
        let temp = TempDir::new("snapshot_url");
        let mut args = input(&temp);
        args.public = true;
        let post = mock_create(&mut server, false).await;
        let get = server.mock("GET", VIEW).expect(0).create_async().await;
        create(&cfg, &args).await.unwrap();
        post.assert_async().await;
        get.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn waits_through_pending_and_saves_binary_png() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let temp = TempDir::new("snapshot_png");
        let mut args = input(&temp);
        let out = temp.path().join("saved.png");
        args.out = Some(out.to_str().unwrap().to_owned());
        let post = mock_create(&mut server, true).await;
        let pending = server
            .mock("GET", VIEW)
            .with_status(200)
            .with_header("content-type", "application/vnd.api+json")
            .with_body(r#"{"data":{"attributes":{"status":"pending"}}}"#)
            .expect(1)
            .create_async()
            .await;
        let ready = server
            .mock("GET", VIEW)
            .with_status(200)
            .with_header("content-type", "image/png")
            .with_body(PNG)
            .expect(1)
            .create_async()
            .await;
        create(&cfg, &args).await.unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), PNG);
        post.assert_async().await;
        pending.assert_async().await;
        ready.assert_async().await;
        assert!(create(&cfg, &args)
            .await
            .unwrap_err()
            .to_string()
            .contains("already exists"));
        assert_eq!(std::fs::read(&out).unwrap(), PNG);
        cleanup_env();
    }

    #[tokio::test]
    async fn timeout_keeps_url_and_does_not_create_file() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let temp = TempDir::new("snapshot_timeout");
        let mut args = input(&temp);
        let out = temp.path().join("saved.png");
        args.out = Some(out.to_str().unwrap().to_owned());
        args.timeout = 1;
        let post = mock_create(&mut server, true).await;
        let get = server
            .mock("GET", VIEW)
            .with_status(200)
            .with_header("content-type", "application/vnd.api+json")
            .with_body(r#"{"data":{"attributes":{"status":"pending"}}}"#)
            .create_async()
            .await;
        let error = create(&cfg, &args).await.unwrap_err().to_string();
        assert!(
            error.contains("timed out") && error.contains(VIEW),
            "{error}"
        );
        assert!(!out.exists());
        post.assert_async().await;
        get.assert_async().await;
        cleanup_env();
    }

    #[tokio::test]
    async fn rejects_failed_or_unexpected_downloads() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        for (status, content_type, body) in [
            (404, "application/json", "{}"),
            (424, "application/json", "{}"),
            (200, "text/html", "login"),
            (200, "image/png", "not a PNG"),
            (200, "application/json", "invalid JSON"),
            (
                200,
                "application/json",
                r#"{"data":{"attributes":{"status":"failed"}}}"#,
            ),
        ] {
            let mock = server
                .mock("GET", VIEW)
                .with_status(status)
                .with_header("content-type", content_type)
                .with_body(body)
                .create_async()
                .await;
            assert!(wait_for_png(&cfg, VIEW, Duration::ZERO).await.is_err());
            mock.assert_async().await;
            mock.remove_async().await;
        }
        cleanup_env();
    }

    #[tokio::test]
    async fn rejects_create_api_errors_and_malformed_responses() {
        let _lock = lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let cfg = test_config(&server.url());
        let temp = TempDir::new("snapshot_bad_response");
        let args = input(&temp);
        for (status, body) in [
            (403, "{}"),
            (404, "{}"),
            (200, "{}"),
            (
                200,
                r#"{"data":{"attributes":{"url":"https://example.org/api/v2/users"}}}"#,
            ),
        ] {
            let mock = server
                .mock("POST", "/api/v2/snapshot")
                .with_status(status)
                .with_header("content-type", "application/vnd.api+json")
                .with_body(body)
                .create_async()
                .await;
            assert!(create(&cfg, &args).await.is_err());
            mock.assert_async().await;
            mock.remove_async().await;
        }
        cleanup_env();
    }
}
