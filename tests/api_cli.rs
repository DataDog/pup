use mockito::Matcher::UrlEncoded;
use std::process::Command;

#[tokio::test]
async fn api_cli_sends_fields_and_headers() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/api/v2/monitors")
        .match_query(UrlEncoded("tag".into(), "env:prod".into()))
        .match_header("x-test", "yes")
        .with_status(200)
        .with_body("{}")
        .create_async()
        .await;

    let output = Command::new(env!("CARGO_BIN_EXE_pup"))
        .args("api v2/monitors -F tag=env:prod -H x-test:yes".split_whitespace())
        .env("PUP_MOCK_SERVER", server.url())
        .env("DD_API_KEY", "test-api-key")
        .env("DD_APP_KEY", "test-app-key")
        .env_remove("DD_ACCESS_TOKEN")
        .output()
        .expect("run pup");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    mock.assert_async().await;
}
